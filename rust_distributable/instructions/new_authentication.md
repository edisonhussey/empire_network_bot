# Two-phase offline licence binding

## Implemented OpenAuto model

OpenAuto now uses OA2/schema-2 signed licences and a one-way two-stage state
machine:

- Stage 0 / unactivated: the vendor-signed token contains the server and
  bootstrap Green main-castle coordinates. It intentionally contains no game
  player ID. Only the authenticated bootstrap connection is permitted.
- Stage 1 / activated: the first successful gbd whose real server and Green
  main-castle coordinates match the signed stage-0 values supplies gpi.PID.
  OpenAuto stores that player ID as the permanent local binding.
- Later connections ignore castle coordinates and require the authenticated
  server and gpi.PID to match the stored binding. This permits relocation but
  rejects using the copied licence with another game account.
- Expiry remains enforced in both stages. Automation, packet injection, account
  data APIs, and other protected operations require stage 1.
- Renewal uses the same license_id with a higher revision and retains the
  existing player binding.

The Ventrilo test facts are:

    stage 0 signed input: server=US1, bootstrap main castle=509:405
    first authenticated identity: gpi.PID=16862926
    main castle id observed in the same gbd: 16011862

The issued stage-0 test token is admin/ventrilo-stage0-4-day.token. Its decoded
signed claims contain no player_id; that value must be learned through
initialization.

Implementation map:

- Signed claim validation: crates/empire-core/src/licence.rs
- GBD identity extraction: crates/empire-core/src/account.rs
- Durable binding migration/storage: crates/empire-core/src/store/schema.rs
  and crates/empire-core/src/store.rs
- Stage transition and runtime enforcement:
  crates/empire-daemon/src/licence.rs
- Binding before post-login traffic continues:
  crates/empire-daemon/src/direct.rs
- Offline issuer: crates/empire-license-admin/src/main.rs

## Design rationale

Use a two-phase offline license model:
1. Purchase-time license creation
The buyer gives you only the information they already know:
Server: GB1
Main castle coordinates: 517:842
Duration: 4 days

Your license generator creates a signed license containing:
license_id
server
bootstrap_castle_x
bootstrap_castle_y
issued_at
expires_at
features

Example logical payload:
{
  "license_id": "OA-82F1",
  "server": "GB1",
  "bootstrap_x": 517,
  "bootstrap_y": 842,
  "issued_at": 1791072000,
  "expires_at": 1791417600,
  "features": ["attack", "recruit"]
}

Sign this payload with your private Ed25519 key.
The distributed app contains only the corresponding public key.
2. First launch: bootstrap verification
The license starts in:
UNACTIVATED

The app must not trust coordinates typed by the user.
It should:
1. Load license.
2. Verify vendor signature.
3. Check expiry.
4. Connect to the real Goodgame websocket.
5. Wait for authenticated player/castle state.
6. Read:
   - server
   - current main castle coordinates
   - stable player/account ID
7. Compare websocket server + coordinates
   against the signed bootstrap values.

Required condition:
websocket.server == license.server
AND
websocket.main_castle_x == license.bootstrap_x
AND
websocket.main_castle_y == license.bootstrap_y

If false:
Activation rejected
Bot remains disabled

If true:
Activation succeeds

3. On successful first activation
Once the coordinate check succeeds, capture the stable account identity exposed by the websocket.
For example:
player_id = 83917462
server = GB1

Then create a local activation record:
{
  "license_id": "OA-82F1",
  "player_id": "83917462",
  "server": "GB1",
  "activated_at": 1791080000
}

The important transition is:
BEFORE ACTIVATION:
license is bound to coordinates

AFTER ACTIVATION:
license is bound to player/account ID

Coordinates are never the permanent identifier.
4. Store activated state
The app now stores:
Original vendor-signed license
+
Local activation record

Conceptually:
License
├── vendor payload
│   ├── license_id
│   ├── expiry
│   ├── bootstrap coordinates
│   └── features
│
├── vendor signature
│
└── activation
    ├── player_id
    ├── server
    └── activation timestamp

The original vendor-signed section must never be editable without invalidating the signature.
5. Every later launch
Once activated, do not check the old castle coordinates anymore.
The runtime process becomes:
1. Load vendor license.
2. Verify vendor signature.
3. Check expiry.
4. Load activation record.
5. Connect to Goodgame websocket.
6. Read current player/account ID.
7. Compare against activated player ID.
8. If equal:
       run bot.
   Otherwise:
       reject license for this account.

The core condition becomes:
if (ws.player_id != activation.player_id)
    reject_license();

Not:
if (ws.castle_x != original_x)
    reject_license();

6. Castle relocation
Example:
Initial purchase:
GB1
517:842

First activation:
Websocket:
castle = 517:842
player_id = 83917462

Activation succeeds.
Later the user relocates:
733:191

Next launch:
Websocket:
castle = 733:191
player_id = 83917462

Result:
player_id matches
→ license valid

The coordinate change is ignored.
7. License sharing attempt
Original user:
player_id = 83917462

Friend receives copied app/license and logs into:
player_id = 27461983

Runtime check:
27461983 != 83917462

Result:
License rejected
Bot disabled

So copying the activated license does not let another account use it.
8. Recommended state machine
Implement the license as three states:
UNACTIVATED
ACTIVATED
EXPIRED

Transition:
                 websocket coordinates match
UNACTIVATED ---------------------------------> ACTIVATED
     |                                             |
     | expiry reached                              | expiry reached
     v                                             v
   EXPIRED <------------------------------------- EXPIRED

An activated license should never return to UNACTIVATED through normal application logic.
9. Suggested implementation structure
Something like:
struct VendorLicense {
    std::string license_id;
    std::string server;

    int bootstrap_x;
    int bootstrap_y;

    uint64_t issued_at;
    uint64_t expires_at;

    uint32_t feature_flags;

    std::array<uint8_t, 64> signature;
};

Activation:
struct ActivationRecord {
    std::string license_id;
    std::string server;
    std::string player_id;
    uint64_t activated_at;
};

Runtime websocket identity:
struct AccountIdentity {
    std::string server;
    std::string player_id;

    int main_castle_x;
    int main_castle_y;
};

10. First activation logic
bool activate(
    const VendorLicense& license,
    const AccountIdentity& account)
{
    if (!verifyVendorSignature(license))
        return false;

    if (isExpired(license))
        return false;

    if (account.server != license.server)
        return false;

    if (account.main_castle_x != license.bootstrap_x)
        return false;

    if (account.main_castle_y != license.bootstrap_y)
        return false;

    ActivationRecord activation {
        .license_id = license.license_id,
        .server = account.server,
        .player_id = account.player_id,
        .activated_at = currentTime()
    };

    saveActivationRecord(activation);

    return true;
}

11. Normal runtime logic
bool validateRuntime(
    const VendorLicense& license,
    const ActivationRecord& activation,
    const AccountIdentity& account)
{
    if (!verifyVendorSignature(license))
        return false;

    if (isExpired(license))
        return false;

    if (activation.license_id != license.license_id)
        return false;

    if (account.server != activation.server)
        return false;

    if (account.player_id != activation.player_id)
        return false;

    return true;
}

12. Important security rule
Do not let this happen:
delete activation file
→ app thinks license is new
→ activate on another account

Otherwise sharing remains easy.
So once activation occurs, the existence/state of activation needs tamper resistance.
At minimum, bind the activation record to:
license_id
player_id
server

and protect its integrity.
But because the whole system is local, remember the limit:
you can make resetting/rebinding difficult, but you cannot make it cryptographically impossible against someone who fully controls and reverse-engineers the binary.
For normal customers, though, the intended model is very clear:
PURCHASE:
coordinates identify who may activate

FIRST LOGIN:
coordinates verified from real websocket

ACTIVATION:
capture permanent player ID

EVERY FUTURE LOGIN:
verify player ID

CASTLE MOVES:
no problem

LICENSE COPIED TO ANOTHER ACCOUNT:
rejected

That is the implementation structure I would use.






after you finished doing that a few thinsgs to clear up 
allow green ice fire, to be init scanned, since you know how to jitter space and slowly scan them . 

in  attack , and recruit remove reference to copy json and paste json. 

instead add a new major tab called Manage presets
grouped by logical components. 
at this stage they can delete or copy config, and at top a medium sized, box import config

config must be smarter, since idea is to share they cant be account dependent. like have source target. 
instead have things like kingdom id if a task or if a recruit bot the fact that it is main castle, ice , sands, fire
outposts can because we cant tell who is who

and multi layer like config can represent an attack strcture, or even the whole bot (combination of chosen atk bot and recruit.)

minor thing in create task 
uncheck use main castle makes the coordinates visible 

not need to explain each kingdom has 1 type of fortress, remove that text. 
crename ruby 1 -> ruby slow 
ruby 2 -> ruby fast 
save task button at bottom left rhgouthout as its more readable. and should be final things. all mangement now moved to management tab. 
rename start tab to Control Panel , and green circle/red  to make clear if connection is already active , and greyed out as it already is same for automation. just to make it very clear on Control Panel . start and close should be next to each other and below like how step 2 is structured. 
in dashboard make it more compact and friendly with 1080 1920 . 

we instead track cumulative rubies earned over entire lifetime. bounded both sides. try fit the stuff so not necessarily scollable, except the built in widget of the human readbale logs. then add final tab for 'Support' that's empty for now 
might be worth making the dashboard more even, and smaller text just slightly 
