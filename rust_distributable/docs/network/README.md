# Network protocol reference

The one place for what is known about the raw packets of the Goodgame Empire protocol: **observed behaviour**,
payload structure and edge cases. Implementing something new should start here and not need a search through
the code. How the bot *uses* the packets is in [`../architecture/`](../architecture/README.md), not here.

## Layout (the rule, so new files go in the right place)

```text
docs/network/
  README.md            this index and the conventions
  <command>.md         ONE file per packet command, named exactly as on the wire (lowercase), all in the same template
  captures/            the raw packets, one packet per file, exactly as recorded
  sequences/           several packets that work together, named for the purpose (support, kingdom_transfer)
  topics/              notes that cut across commands (incoming movements, alliance fields)
```

**A command file** always has this shape, so any of them can be read the same way:

```text
# `cmd`: Title
| Direction | client to server / server to client (push) / both |
| Evidence  | proven live / captured / client code only |
| Code      | the files that build, read or handle it |
## Purpose
## Request   (a real example, then a field table)      <- or "## Push" for an unasked packet
## Reply
## Rules and behaviour      (what must hold; edge cases; statuses)
## Handling in the code
## Captures                 (links into captures/)
## Open questions           (what is NOT known)
```

**Evidence** says how far to trust a file: *proven live* (we sent it and the server answered), *captured* (seen in a
recording), *client code only* (read from the game client, never seen on the wire). A fact that is only a guess is
under **Open questions**, never in the body.

**Captures** are named `<command>_request.txt` (client to server), `<command>_response.txt` (the reply to a request),
`<command>_push.txt` (an unasked server message), or `<command>_<what>.txt` for a variant. One packet per file, with no
annotations; tests include some of them (`include_str!`), so renaming one means fixing those paths.

**Related commands share a file** only when they have the same payload shape and one has nothing else to say
(`acl`, the chat history, is a section of [`acm`](acm.md)).

## Commands

### Attack lifecycle
* [`adi`](adi.md): inspect a target before attacking
* [`cra`](cra.md): create the attack
* [`gam`](gam.md): march updates, including other players' movements
* [`cat`](cat.md): a march landed and is returning
* [`mrm`](mrm.md): an attack was retreated

### Map
* [`gaa`](gaa.md): map viewport (towers, fortresses, castles)

### Support (auto-station)
* [`dcl`](dcl.md): every castle and its units
* [`sdi`](sdi.md): open the support page
* [`cds`](cds.md): send the support march
* [`mcm`](mcm.md): recall it

### Kingdom transfers and account state
* [`kut`](kut.md): send troops to another kingdom
* [`msk`](msk.md): spend a minute-skip item on a transfer
* [`kpi`](kpi.md): kingdoms and what is travelling between them
* [`gcu`](gcu.md): coins and rubies
* [`sce`](sce.md): item stock
* [`rue`](rue.md): a castle's unit count after recruiting

### Alliance
* [`acm`](acm.md) (and `acl`): alliance chat

## Sequences
* [`sequences/support.md`](sequences/support.md): `dcl`, `sdi`, `cds`, `mcm` when an owned castle is attacked
* [`sequences/kingdom_transfer.md`](sequences/kingdom_transfer.md): `dcl`, `kpi`, `kut`, `msk`

## Topics
* [`topics/incoming_movements.md`](topics/incoming_movements.md): what `gam` says about incoming movements
* [`topics/alliance_fields.md`](topics/alliance_fields.md): the alliance fields the PVP tab uses

## Elsewhere
* Server `status` codes: [`../architecture/errors.md`](../architecture/errors.md). `0` is success; others reject (for example `95` target cooling down, `93` commander travelling, `256` lord in use).
* Kingdom and context switching (`gbl`, `upt`, `gbd`): [`../architecture/navigation.md`](../architecture/navigation.md).
* Game rules read from the client (levels, prices): [`../game_infrastructure/`](../game_infrastructure/).
* `proxy_packet_monitor/learned_info/` holds that tool's own earlier copies of some of these notes; this folder is the maintained reference.
