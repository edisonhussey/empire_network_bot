# development/notes

Structured notes about how this system works and what is currently broken.

These exist because the alternative — conclusions scattered across commit
messages, chat history and a memory file — makes every question expensive to
re-answer. Each file here is meant to be **edited in place** as facts change,
not appended to.

## Files

| file | what it answers |
|---|---|
| `architecture.md` | What the processes are, who owns the database, how a request travels from a click to a socket. |
| `fortress-discovery.md` | The complete discovery model: the lattice, the sweep, the request shape, and what is proven versus assumed. |
| `open-defects.md` | The live register of known defects, each with the evidence that found it. |

Related, elsewhere:

- `development/lab/README.md` — the local workbench and the offline simulator.
- `rust_distributable/docs/network_requests.md` — per-command protocol facts,
  tagged `[capture]` / `[code]` / `[unknown]`. **When a capture contradicts the
  code, the capture wins.** That rule is worth keeping.
- `/memories/repo/empire_bot.md` — session-to-session memory. It should hold
  *pointers and traps*, not explanations; those live here.

## The rule for writing in here

State the **evidence**, not just the conclusion.

Bad: "the wide window is fine."

Good: "asked 42×42 at 262,886; the reply spanned 262..303 × 886..927 with 370
objects. Repeated for 11 windows, all 42 wide. Source: `network_message` rows,
paired by kingdom and sequence."

The second one can be re-checked, argued with and built on. The first is a
rumour that will cost someone an hour later.
