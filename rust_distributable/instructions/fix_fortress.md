What's actually needed
Create an attack bot with a fortress task in Sands and start it. Expect roughly 400 slots ≈ 10 minutes at the ~1 s pacing — and since the block is solid, the count should rise steadily rather than stall.

Watch Done on the dashboard: if it sits on Mapping fortress coordinates while Next fortress counts down Mapping · N areas left, that's the walk running correctly. The number to check if you doubt it is completed vs slots in fortress_scan_frontier — that's the honest progress meter, and right now it reads 0 of 126.

3:28 AM
24s
•
DeepSeek V4.1 Flash
no but look intialization of acc > scans coordinates. then once stored and known for ice sand fire --> the bot knows just to scan for pure reason of cooldown timers , right?

3:29 AM


























































































That's the right question, and checking it properly just surfaced a serious bug in what I added earlier. Let me verify.

Your model is correct in intent: initialisation discovers coordinates; after that the only reason to scan is cooldown timers. But the "pre-flight recheck" I added has a flaw — let me prove it with a test before I claim it:

















































Confirmed — and it's worse than a design question. My test fails on the first assertion: a fortress whose cooldown we just read is queued again immediately. That's an infinite request loop.

The chain: the refresh writes refresh_due_ms = +10min, but the GAA response's upsert then resets it to 0 for any positive cooldown, and my per-pass queue_fortress_recheck re-queues on 0. So the bot would read cooldowns ~1/second forever, starving discovery and attacks. Fixing it:







The store edit missed its anchor. Let me read the exact current text:


















Now the constants — let me place them at module level:






Fixed and verified. Now the full suite, then a rebuild — because the installed build still has the looping code:































Two follow-ups: an older test asserted the buggy behaviour, and an unused closure param. Fixing both:

















All 141 pass, clippy clean. Now bumping versions and rebuilding — the installed build still contains the loop:



















Yes — that's exactly the design, and asking it uncovered a real bug.

Your model is right
Initialisation gives you both halves in one pass. The gaa response that reveals a fortress carries its remaining cooldown in the same row ([11, x, y, -1, level, cooldown_s, occupier, kingdom]), so we store an absolute available_at_ms. Once coordinates are known, the bot never needs to rediscover them — it already knows when each one opens.

So after that, a gaa is sent for exactly three reasons, and no others:

reason	when
Confirm a window about to open	within 10 min of available_at
Background sanity read	not looked at for 6 hours
Revalidate right before attacking	required by the 1-minute window
No periodic coordinate re-scan. Your understanding is correct.

But checking it found a bug I introduced
My "build state before start" step was gating its queue on refresh_due_ms = 0 — and the refresh response's upsert resets that flag to 0 for any positive cooldown. So every pass re-queued the same fortresses, forever: the bot would have spent its entire time re-reading cooldowns at roughly one request per second, never discovering and never attacking.

I wrote a test for it first and it failed on the very first assertion — "a fortress whose cooldown we just read must not be read again" — so it was real, not theoretical.

The fix gates on observation age instead of a flag, which is what makes a read unable to schedule its own successor:


That also means a freshly discovered fortress is not re-read at all — its cooldown came straight from the server seconds ago. Which is precisely the model you described.

Delivered
141 tests pass (3 new: the loop regression guard, the growth proof, plus the corrected earlier test), clippy clean.
Rebuilt as 0.1.23, service api_version: 25, licence still active — installed, because the build you had (0.1.22) contained the looping code.
Distributable: OpenAuto_0.1.23_aarch64.dmg.
One thing that hasn't changed: only kingdoms with a fortress task get walked, so all three of Ice/Sands/Fire need a fortress task for their coordinates to be discovered. And the walk still hasn't run once — 0 of 126 probes — so the count you see is still just what the radius-50 scan picked up along the way.

3:34 AM
5m 40s
•
DeepSeek V4.1 Flash
