# The support sequence (`dcl`, `sdi`, `cds`, `mcm`)

How the packets in this folder fit together when troops are sent out of an attacked
castle and called back. Each packet has its own note ([dcl](dcl.md), [sdi](sdi.md),
[cds](cds.md), [mcm](mcm.md), with the replies saved as `dcl.txt`, `sdi_response.txt`,
`cds.txt`, `mcm_response.txt`); this page is the order and the roles. They were
captured with a mitmproxy tool, and the Rust service now sends the same frames
itself (`crates/empire-core/src/station.rs` builds and reads them).

| Step | We send | The server answers | Used for |
| --- | --- | --- | --- |
| 0 | `gam` (inbound) | | An attack on an owned castle: [incoming_gam.md](incoming_gam.md) |
| 1 | `%xt%<header>%dcl%1%{"CD":1}%` | `PID`, `C[].KID`, `C[].AI[].AID`, `AC`, resources | Every castle's stock; `PID` must be the account |
| 2 | `%xt%<header>%sdi%1%{"SX","SY","TX","TY"}%` | `SCID`, `gui.I`, `gui.TU` | Opens the support page; fresh stock of the **source** castle |
| 3 | `%xt%<header>%cds%1%{"SID","TX","TY","LID","WT":1,"HBW":-1,"BPC":0,"PTT":0,"SD":0,"A":[[unit,count],...]}%` | `A.M` (MID, PT, TT, KID, `SA`, `TA`), `A.A` | Commits the support march; the reply carries the **support MID** |
| 4 | `%xt%<header>%mcm%1%{"MID":<support MID>}%` | `A.M` (MID, PT, TT, `D:1`) | Recalls the support march; `TT - PT` is the way home |
| | `mrm` (inbound) | | The attacker retreated: [mrm.md](mrm.md) |

`<header>` is the world's server header (`EmpireEx_49` in these captures; the replies
carry none). The request id is `1` throughout.

## Facts the Rust code relies on

* **`sdi` opens, `cds` dispatches.** Earlier notes had it the other way round. `sdi`
  sends nothing; `cds` is the send, and only its success reply confirms a march.
* **Two MIDs.** The attack's MID (from `gam`) is what `mrm` names; the support
  march has its own, which only the `cds` reply gives, and `mcm` must name that one. A
  movement's MID is also replaced when it lands and starts back, so a MID from one leg
  is never carried to the next.
* **`SID` in `cds` is the castle**, the area id: `437391` here, the same as the `sdi`
  reply's `SCID`, the `dcl` area `AID` and `TA[3]` in the attack's `gam`. It is not a
  movement id.
* **`AC` in `dcl` mixes troops and tools** (`602` is a troop; `614` scaling ladder,
  `660` banner and `637` are tools). Only ids in the troop table are sent as support.
  A row with count `0` is not available.
* **`gui.I` of `sdi` is home stock; `gui.TU` is troops already out.** Support uses
  `gui.I`, taken at send time, and where a `dcl` for the same castle (matched by
  kingdom and area id) is to hand, the smaller of the two counts.
* **Recall returns after the elapsed travel.** A support march recalled seconds after
  it left is home seconds later, so the commander is out for well under a minute even
  though the outbound journey (`TT` 4509 s in the capture) is long.
* **`cds` destination is only coordinates.** The capture sends to `(468, 507)`,
  another player's castle in the same alliance; the packets do not show what a valid
  destination must be.

## Not established

* The most troop types or the largest total one support march may carry. The proxy
  prototype used up to ten distinct types at their full home counts; the Rust code does
  the same and records any refusal rather than repeating the request.
* Whether the server pushes anything when the *attacker* retreats other than `mrm`.
* What `LID` the server accepts for a support march other than a commander id from the
  account's own roster. The capture used `0`.
* Status codes of refusals for `sdi`, `cds` and `mcm`: none have been captured. A
  non-zero status is treated as a refusal and never retried unchanged.
