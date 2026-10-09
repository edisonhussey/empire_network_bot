# Timing: the stochastic scheduler

How long the bot waits between the steps of an attack. Code: `empire-core/src/timing.rs`
(the generator, no I/O) and `empire-daemon/src/direct/scheduler.rs` (using it for the
real waits and recording what happened).

## The interval

```text
D(t) = D_min + D0 * exp( sum_k A_k sin(theta_k(t)) + eps )
```

* `D_min` is the stochastic lower bound, **0.4 s** by default. It is a bound, not a
  fixed delay. An action with a larger protocol minimum passes it as `floor_s` and the
  larger of the two applies.
* `D0` is the baseline scale of the action.
* Three waves (configurable) with random starting phases and frequencies, periods from
  15 s to 10 minutes. Amplitudes are fixed and share one variability budget (0.6 in
  log space, so an interval moves between about 0.55x and 1.8x its baseline).
* Each frequency drifts by a bounded, mean-reverting random process (an exact
  Ornstein-Uhlenbeck step), clamped to its range. Phases integrate the *current*
  frequency over each step, so a frequency change never makes a wave jump.
* `eps` is small independent noise, kept apart from the wave sum so the two can be
  shown separately.

The signal is evaluated directly. There is no density to integrate or normalise, and
work per interval is constant.

## Time

The model advances by real elapsed time on a monotonic clock, passed in as seconds so
tests can drive it. It copes with sleeps and reconnects: a long gap is one bounded
step, never a numerical blow-up, and time that does not move forward is ignored. Unix
epochs are used only for persistence and display.

## Where it is used

The existing attack sequence is unchanged (`adi`, wait, `cra`, acknowledgement). The two
human-scale waits in it now come from the generator instead of uniform draws:

| Wait | Protocol minimum | Baseline | Typical |
| --- | ---: | ---: | ---: |
| After `adi`, before `cra` | 2.0 s | 1.0 s | about 3.1 s |
| After a `cra` acknowledgement, before the next handshake | 0.75 s | 0.8 s | about 1.6 s |

Those typical values are where the old uniform ranges sat, so throughput is unchanged
on average; what changes is that successive waits drift smoothly.

Heartbeats, login, acknowledgements and the other protocol traffic are **not**
randomised; only these logical-action waits are.

## Limits always win

The generator only proposes a wait. The global `cra` spacing (4 s plus jitter) is applied
afterwards (`PacingPolicy::cra_due_at_with`) and once more immediately before sending.
When a limit, not the stochastic interval, decided the time, the scheduler records
"global cra spacing" and the Development tab shows it separately from the stochastic
interval. Each wait is generated fresh from "now" once the previous step resolves, and
the attack machine is single-flight, so an overdue action cannot queue a backlog or
fire a burst afterwards, and nothing busy-waits.

## Tests

Intervals stay above the stochastic minimum and above an action's own minimum; waves
stay finite and in bounds over 100,000 s; the signal is continuous under frequency drift;
huge time jumps are stable; non-monotonic time is ignored; the same seed gives the same
sequence; the wave count is configurable; the projection matches the signal; the global
spacing is never violated; waits recorded with the limit that decided them.
