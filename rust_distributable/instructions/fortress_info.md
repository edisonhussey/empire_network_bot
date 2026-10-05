Yes—fortresses can be identified directly from gaa; no gbl name lookup is needed during normal scanning.
The decisive difference is the first value of each AI row:
Robber Baron Castle:
[2, x, y, ...]

Fortress:
[11, x, y, -1, 45, cooldown_seconds, occupier_player_id, kingdom_id]
For example, the captured fortress row was:
[11, 594, 633, -1, 45, 33814, 17236867, 1]
A separate historical gbl response returned that exact row with:
"N": "Desert fortress"
That proves 11 is the fortress area type—it is not merely inferred from its coordinates.
The important fields are:
- row[0] == 11: fortress.
- row[1], row[2]: coordinates.
- row[4] == 45: fixed Sands fortress/NPC level in these captures.
- row[5]: remaining cooldown in seconds.
- row[6]: current/raw player or occupier ID.
- row[7]: kingdom ID; 1 is Burning Sands.
The cooldown interpretation is also supported by repeated captures: its value decreases exactly with elapsed time, then resets after another player defeats the fortress. OpenAuto can therefore calculate:
next_available_at = gaa_observed_at + row[5] seconds
I implemented this in Rust:
- Added a distinct type-11 fortress parser in [account.rs (line 195)](/Users/edisonhussey/Desktop/github_gge_bot/empire_network_bot/rust_distributable/crates/empire-core/src/account.rs:195).
- Added a separate fortress_target database table instead of mixing fortresses with RBCs.
- Persisted the server cooldown as an absolute ready timestamp.
- Connected live gaa messages to fortress discovery and storage.
- Added the confirmed protocol behaviour to [docs.md (line 96)](/Users/edisonhussey/Desktop/github_gge_bot/empire_network_bot/rust_distributable/docs.md:96).
- Added parser, cooldown, and database migration tests.
One evidence distinction: the historical named lookup I found is for 594:633. Your newly proven 594:594 did not appear in that particular named capture, but it will be recognized automatically when its gaa row starts with type 11.
All Rust workspace tests pass: 120 passed, 0 failed, and the diff check is clean. This implements discovery and durable availability tracking; it does not yet make the attack scheduler actively select fortress targets.



Yes—the Fire data confirms the fortress classifier.
Your latest Ventrilo gaa traffic contains four Fire Peaks fortresses:
Coordinates	Type	NPC level	Cooldown remaining	Estimated ready time
692:575	11	55	47,500 seconds	~13:42:17
672:594	11	55	54,673 seconds	~15:41:50
711:594	11	55	54,301 seconds	~15:35:38
731:575	11	55	21,252 seconds	~06:24:51


The captured row format is therefore:
[11, x, y, -1, npc_level, cooldown_remaining_seconds, raw_player_id, kingdom_id]
For example:
[11,692,575,-1,55,47500,16859207,3]
This confirms:
- 11 identifies a fortress across kingdoms.
- 3 identifies Fire Peaks.
- Field 4 is the NPC level:
  - Sands fortress: level 45
  - Fire fortress: level 55
- Field 5 is the server-provided remaining cooldown.
- Ordinary Fire RBCs remain type 2, so they cannot be confused with these fortresses.
The earlier repeated Sands observations also prove the cooldown interpretation numerically:
00:30:29  82797
00:30:35  82792
00:30:35  82791
00:30:36  82790
The value decreases with elapsed wall time. The four Fire fortresses were each encountered only once in this movement session, so we do not yet have a second Fire observation showing their individual countdowns falling—but their structure matches the already-proven Sands countdown field exactly.
I updated the Rust implementation to retain the NPC level as well as coordinates, kingdom, cooldown, and raw player ID. The exact Fire rows are now regression-test fixtures in [account.rs (line 278)](/Users/edisonhussey/Desktop/github_gge_bot/empire_network_bot/rust_distributable/crates/empire-core/src/account.rs:278), and the findings are recorded in [docs.md (line 96)](/Users/edisonhussey/Desktop/github_gge_bot/empire_network_bot/rust_distributable/docs.md:96).
Verification: 105 tests passed, 0 failed, including database migration, Fire classification, and absolute ready-time persistence.
