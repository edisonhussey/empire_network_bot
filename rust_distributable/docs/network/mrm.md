# MRM: retreat an attack

The supplied [MRM example](mrm.txt) is:

```text
%xt%mrm%1%0%{"MID":155312686}%
```

## Observed structure

| Part | Example | Meaning |
| --- | --- | --- |
| XT command | `mrm` | Movement retreat command, as identified by the user. |
| JSON `MID` | `155312686` | Movement ID of the attack being retreated. This matches the `M.MID` field used by [GAM movement records](incoming_gam.md). |

The user has confirmed that `mrm` **logically retreats the attack**. Once this action is taken, it **cannot be undone** and that MID is **finished permanently**. Treat the MID as terminal after a confirmed retreat; do not resume or reuse it as an active attack. A new attack would need a new MID.

The single example does not establish whether this exact frame is a client request, server response, or both, nor whether a separate acknowledgement confirms success. Code handling live traffic should distinguish a sent command from a confirmed retreat before marking an attack terminal. The command's remaining XT header fields are left uninterpreted here.
