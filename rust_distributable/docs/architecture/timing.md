# Timing: human-paced waits from ten superimposed waves

How long the bot waits between the steps of an attack. Code: `empire-core/src/timing.rs`
(the generator, no I/O, no clock) and `empire-daemon/src/direct/scheduler.rs` (using it
for the real waits and recording what happened).

This is for **logical human actions** only: the pause before an attack after inspecting
its target, and the pause after an attack is acknowledged. Heartbeats, login,
acknowledgements and other protocol traffic are not randomised.

## The signal

```text
S(t) = sum over 10 waves  A_k * shape_k(omega_k * t + phi_k)
D(t) = m + exp( offset + S(t) + eps )
```

* **`t` counts actions, not seconds.** Each wait takes the next value and `t = t + 1`.
  Sampled at whole steps, sines and cosines of unrelated frequencies look random, and ten
  of them do not visibly repeat. (The previous design was three slow waves on a clock,
  which draws as a smooth sinusoid.)
* **Three shapes**, so the sum is not made of one: `sin`, `cos`, and a folded wave
  `asin(b sin x) / asin(b)` with `b` between 0.55 and 0.92, a rounded triangle with
  sharper corners than a sine.
* **`exp(...)` makes the wait positive and low-heavy**: most waits are short, a minority
  long. **`m = 0.5 + rand()/2` seconds** is the floor, drawn once per session, so no wait
  is shorter than half a second and the floor differs between sessions.
* **`offset` (0.5, in log space) is the lever.** It sets where the bulk sits; the wave
  spread (`wave_sigma = 1.5`) sets how wide it is. Measured over 160,000 waits:

  | | |
  | --- | ---: |
  | between 1 s and 5 s | 61 % |
  | under 1 s | 11 % |
  | over 5 s | 27 % |
  | over 10 s | 13 % |
  | over 30 s | 3 % |
  | median / mean | 2.4 s / 5.8 s |
  | longest allowed | 120 s |

* `eps` is symmetric Gaussian noise (sigma 0.3), kept apart so it can be shown apart.

## Parameters adjust over time

Every step each frequency is pulled gently back towards where it started and nudged
(sigma 0.004 rad), and phases advance by the frequency **in force**, so a change never
makes a wave jump (phase continuity). Frequencies stay in 0.9 to 5.3 rad per step and
never settle on an exact repeat. Amplitudes are fixed and share one budget so the wave sum
has the configured spread whatever the mix of shapes. The wave count is configurable
(`wave_count`, default 10).

## Why the frequencies are chosen, not just drawn

Sampled at whole steps, a frequency near a whole turn (6.28) looks like a *slow* wave
(`sin(6t)` is `-sin(0.28t)`), one near pi alternates every step, and one near a simple
fraction of a turn repeats every few steps. So the range stops short of a turn, those
neighbourhoods are avoided, and the drawn set is tuned (moving one frequency at a time and
keeping moves that help) until the correlation between waits `lag` steps apart,
`sum(w_k cos(lag * omega_k)) / sum(w_k)` over lags 1 to 12, is as small as ten waves allow.

That floor is real: a random draw has a worst lag around 0.5, tuning reaches about 0.3, and
ten angles cannot satisfy twelve lags much better than that. Successive waits are therefore
only *mildly* correlated at the worst lag (a single wave would be 0.9 and above), which is
checked in the tests.

## Using it

* `next_interval(protocol_floor)` returns the next value and moves on one step. The
  action's protocol minimum (2.0 s before an attack, 0.75 s after an acknowledgement, the
  values that applied before this existed) lengthens a wait that is shorter; the sample is
  still used up, never skipped.
* The **global 4 s `cra` spacing** is applied afterwards (`PacingPolicy::cra_due_at_with`)
  and again immediately before sending. Limits only ever make a wait longer. When one
  decided the time, the scheduler records "global cra spacing". Each wait is generated
  fresh from "now" when the previous step resolves, and the attack machine is
  single-flight, so an overdue action cannot queue a backlog or fire a burst, and nothing
  busy-waits.
* `project(n)` works out the next `n` waits from the current parameters, with no noise.
  Because the signal is deterministic given its parameters this is exactly what comes next
  until the parameters drift a little; the Development tab draws it as hollow points,
  labelled a projection.

## Effect on throughput

The waits are longer on average (mean about 5.8 s raw, against about 3.1 s and 1.6 s
before), but attack creation is not what limits sand: its 17 commanders are the limit (see
the analysis in `docs/problems/`). A queue model of the real pipeline (17 commanders, a
784 s lap, single-flight handshakes) gives 76.9 attacks per hour with the old waits and
76.2 with these, about 1 % fewer.

## Tests

Ten waves of three kinds with the configured spread; the wave count is configurable; the
floor is in 0.5 to 1.0 s and no wait goes below it; the 61 % / tail figures above; successive
waits are not strongly correlated at any of the first 12 lags across twelve seeds; chosen
frequencies never alias to a slow or alternating wave; no repeating cycle up to period 40
and almost every wait distinct; noise is symmetric with the configured spread; a protocol
minimum lengthens a wait without skipping a sample; parameters drift but stay in bounds with
continuous phase; the projection equals what comes next when nothing drifts and stays close
for a few steps when it does; identical seeds repeat and different seeds differ; extreme
settings still give finite positive waits; the folded wave is bounded and normalised.
