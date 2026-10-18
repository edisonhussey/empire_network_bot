# `acm`: Alliance Chat Message (and `acl`, the history)

| | |
| --- | --- |
| Direction | server to client (push) for a new message; the same command name is how the client sends one |
| Evidence | captured (one push) plus the client code (`ChatMessageVO`, `ACMCommand`, `ACLCommand`) |
| Code | `empire-core/src/chat.rs` (parse), `store/chat.rs` (`alliance_chat`), `empire-daemon/src/direct/chat.rs` (listener), `empire-daemon/src/chat.rs` (`GET /v1/chat`), `desktop/src/views/chat.js` (the Chat tab) |

## Purpose

A new message in the account's alliance chat. It arrives on its own, with no request, so it is heard in the background.

## Push

```
%xt%acm%1%0%{"CM":{"PID":4616599,"PN":"Dawn Reaver","MA":0,"MT":"Can I get rage bonus please"}}%
```

| Field | Meaning |
| --- | --- |
| `CM` | the message |
| `CM.PID` | the sender's **player** id (the same kind of id as `dcl.PID`); a negative id is the game itself (a system line) |
| `CM.PN` | the sender's name |
| `CM.MT` | the text, escaped (below) |
| `CM.MA` | the message's **age in seconds** when it was sent to us; `0` for a live one |

### No clock

There is no timestamp. The client works out when a message was written as `now - MA` seconds, and so does the code: `sent_at = arrival - MA`. For a live `acm` that is the arrival time.

### Escapes in `MT`

The server replaces the characters the packet format cannot carry; the client undoes them before showing the text (`TextValide.parseChatJSONMessage`), in this order:

| In `MT` | Shown as |
| --- | --- |
| `&percnt;` | `%` |
| `&quot;` | `"` |
| `&145;` | `'` |
| `<br />` | a new line |
| `%5C` | `\` |
| `[` and `]` | a space |

## `acl`: the history (from the client code, not captured)

`acl` is what the client asks for when chat opens; the reply is `{"CM":[message, ...]}`, every message in the same shape (so each has its own `MA`). The code reads it if it ever arrives and skips any message already held. Nothing asks for it today.

## Rules and behaviour

* One packet is one message; they can be handled the moment they arrive, in any order.
* The same sender saying the same thing within five seconds of a stored row is the same message, so a history read does not double what was heard live.
* Kept per account, the newest 2,000.

## Handling in the code

Heard by the account's own session while it is connected (a message sent while it is offline is not stored unless a history is read). The window's Chat tab shows name, time and text.

## Captures

[`acm_push.txt`](captures/acm_push.txt).

## Open questions

The shape of `acl` as the server really sends it; what sending a message (`acm` from the client) looks like (not needed, and never sent).
