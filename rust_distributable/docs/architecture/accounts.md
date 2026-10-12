# Several accounts at once

The service runs any number of game accounts side by side; the window shows one at a
time. Choosing the account in the header changes what is *shown*. Every account's
connection, bot and background work carry on in the service whichever one is in view.

## Sessions

`daemon/src/sessions.rs` holds one session per account, keyed by the canonical account id
(trimmed, lower case). A session is `direct::run` (the reconnecting supervisor that always
was per account) plus four cells it owns: its status, its injection queue, its diagnostic
snapshot and its task. Sessions share nothing but the database and the licence gate:

* Connecting an account replaces only that account's session; others are untouched.
* A failed session stays in the list, so the window can say "needs attention", until it is
  reconnected or ended.
* Ending a session (`DELETE /v1/direct?account_id=`) also stops that account's attack,
  recruit and station bots. Without `account_id` the request is only accepted when exactly
  one session exists.
* The account id is on the status from the first moment, before the socket opens.

State that used to be one-per-service is now per account: the recent-packet window (200 rows
**per account**, so a busy account cannot push another's history out), the run heartbeat
(`hunt.heartbeat_ms.<account>`), the Overview refresh request, the login lock, and the
Development snapshot. Rate limits, the global `cra` spacing, commanders, targets and every
table keyed by `account_id` were already per account.

## API

| Endpoint | Behaviour |
| --- | --- |
| `GET /v1/direct?account_id=` | That account's status; a disconnected stand-in carrying its id when it has no session. With no id: the only (or most recently) connected account |
| `GET /v1/direct/all` | Every session, for the account switcher |
| `POST /v1/direct`, `POST /v1/accounts` | Connect or initialise one account |
| `DELETE /v1/direct?account_id=` | End one account's session and bots |
| `GET /v1/dev`, `/v1/dev/map`, `/v1/dev/movements`, `/v1/dev/shared` | Take `account_id` |
| `GET /v1/messages?account_id=` | One account's packets |
| `POST /v1/injections` | Body takes `account_id`; the packet goes to that account's connection |
| `GET /v1/pvp`, `/v1/overview`, `/v1/station/jobs` | Already took `account_id` |

## Licences and tiers

Each game account has its own licence: a signed token for one server and one main castle,
bound permanently to one player the first time that account signs in. Any number can be
installed side by side, each with its own tier and features (Control Panel > add a licence,
or the licence prompt). A session records the tier and features of *its own* licence, and
that is what is checked:

* A bot starts only for an account whose own licence includes `automation`. A free account
  can sign in, be watched and appear on the maps, but a pro account's licence is never
  borrowed for it. The service refuses with the tier named; the Control Panel disables Start
  and says why.
* Presets (attacks, tasks, attack bots, recruitments, recruit bots, support bots) are one
  shared library in the database, usable by any account.
* A licence for one account expiring ends only that account's session.

A single token covering several accounts would need a new token schema and the issuing
tool; it is not implemented. Several tokens, one per account, is.

## The window

The header has an account menu to the left of Log in. It lists every stored account with its
colour, world, state (bot running, connected, connecting, needs attention, offline) and tier,
and "Add account...". The choice is remembered. Everything that reads an account reads the
one in view (`desktop/src/account-context.js`): Dashboard, Initialize (the scan plan and its
progress), Control Panel (one card, for that account), Logs, Overview, PVP and Development.
Log in, Log out and the bot menu in the header act on it too. Switching reloads those views
for the new account and nothing else.

Each account has a colour, kept stable (a new account takes the lowest unused one, so adding
one never recolours another). Red and green are left to hostile and allied movements.

## Colour-coded maps (Development)

The Development tab shows one kingdom map per signed-in account **on the viewed account's
world**, the viewed one first. A map is that account's own (its towers, its spotlight), with
the movements of every account on the world drawn over it in that account's colour:

* its armies out and coming back,
* the threats against it and its allies, once each (accounts in one alliance record the same
  movement, so a MID is drawn once, in the colour of the account it concerns),
* its stationed castles and support marches (the sending, the recall and the return),
* its castle, named when it is not the map's own.

Arrowheads and labels stay red when hostile and green when allied. Which accounts may be shown
is decided by the service (`GET /v1/dev/shared`): the viewed account plus every account with a
live session on the same game endpoint. An account on another world is never included, and a
signed-out account is not shown to others (the viewed account always is, from stored data).
Maps are created as accounts sign in and removed as they sign out. They poll every two seconds
and animate from timestamps; nothing is sent to the game.

## Tests

`sessions.rs` (independence, replacement, default account, per-account injection),
`dev_shared.rs` (same world, other worlds, signed-out accounts, per-account threats),
the store (per-account packet window and heartbeat), and `account-context.test.js`,
`shared-layers.test.js` in the window.
