# Chat

The **Chat** tab shows the alliance messages the account in view has heard, with who said them and when. Read-only:
nothing is ever sent to the game chat. Packet: [`../network/acm.md`](../network/acm.md).

## Who owns what

| Piece | File | Owns |
| --- | --- | --- |
| Parsing | `empire-core/src/chat.rs` | The message shape, the text escapes, "when it was written". Pure |
| Storage | `empire-core/src/store/chat.rs`, migration V27 | `alliance_chat`: de-duplication and the 2,000-message bound |
| Listener | `empire-daemon/src/direct/chat.rs` | Stores `acm` (and `acl`) as they arrive; sends nothing |
| API | `empire-daemon/src/chat.rs` | `GET /v1/chat?account_id=&limit=` (newest `limit`, oldest first) |
| Window | `desktop/src/views/chat.js`, `chat-model.js` | The tab: polls while open, draws name, time, text |

## How it works

* The server pushes each new alliance message to every online member as `acm`. The account's own session hears it
  in the background, the same way it hears everything else; `direct/chat.rs` stores it next to the other observers
  and nothing waits for it.
* There is no timestamp in the packet, only `MA`, the message's age in seconds. A message was written at
  `arrival - MA`, which is the arrival time for a live one and an earlier time for one read from a history.
* A message is **per account**: each account is in its own alliance. Switching the account in the window redraws from
  that account's rows.
* Bounded: the newest 2,000 messages per account are kept; older ones are deleted as new ones arrive.
* The same sender saying the same thing within five seconds of a stored row counts as the same message, so a history
  read cannot double what was heard live.

## The tab

One line per message: the time (`09:13`, or `Oct 16 · 23:59` for another day), the sender, the text. A system line
(negative player id) reads "Game". The list follows new messages when scrolled to the bottom and stays put when the
reader has scrolled up. It polls every three seconds while the tab is open and never when it is hidden.

## Limits

* Only messages heard while the session was connected are stored. Nothing asks for the history (`acl`) yet; if a
  history ever arrives it is stored without doubling.
* Sending chat is not offered and not implemented.
* Not run against the live server through this code: the packet is the one captured on 2026-10-17, and the listener is
  tested with it and with the chat the operator pasted (nine messages, three senders).
