# `acm`: an alliance chat message arrives

Receive only; nothing is sent to read it. Self-contained, so it can be handled the moment it arrives.

```
%xt%acm%1%0%{"CM":{"PID":4616599,"PN":"Dawn Reaver","MA":0,"MT":"Can I get rage bonus please"}}%
```

| Field | Meaning |
| --- | --- |
| `PID` | the sender's player id (same kind of id as `dcl.PID`) |
| `PN` | sender's player name |
| `MT` | the message text; call it `message_text` |
| `MA` | not known (0 in the capture) |
