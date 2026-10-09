maintain existing features like config, extend features/ refactors are ok if they preserve critical code that prevents complete failure. 

# Codex Implementation Specification — Stochastic Action Scheduler & Development Dashboard

Refactor and extend the existing automation system with a modular stochastic action scheduler, a geographical movement visualisation, and a dedicated Development tab.

**Primary priorities:** performance, readability, correctness, maintainability, and seamless integration with the existing architecture.

Inspect the existing implementation before making changes. Reuse established modules, types, naming conventions, event systems, and database structures wherever appropriate.

Do not create duplicate scheduling systems, redundant state, or unnecessary configuration options.

## 1. Architecture

Separate the system into four logical responsibilities:

**Action Scheduler**
- Determines when the next logical action may execute.
- Maintains stochastic timing state.
- Enforces global and action-specific rate limits.
- Prevents overlapping actions where sequencing is required.

**Target Selector**
- Uses existing geographical spotlight, velocity, cooldown, and weighted-selection logic.
- Supplies eligible targets when an action becomes ready.
- Must remain independent of timing generation.

**Network Executor**
- Converts a logical action into its required packet sequence.
- Preserves packet ordering and required acknowledgements.
- Reports completion, failure, timeout, and relevant response metadata.
- Reuses the existing network transport.

**Development Dashboard**
- Observes scheduler, network, and geographical state.
- Displays diagnostic information and live visualisations.
- Does not independently control execution state or duplicate business logic.

These represent responsibilities, not necessarily four new modules. Follow the project's established architecture.

---

## 2. Stochastic timing generator

Implement a lightweight time-dependent timing signal composed of a small number of continuously evolving waves.

Use a positive interval transformation:

\[
D(t)=D_{\min}+D_0\exp\left(\sum_{k=1}^{n}A_k\sin(\theta_k(t))+\epsilon_t\right)
\]

Where:

- \(D_{\min}\): configured minimum stochastic interval.
- \(D_0\): baseline interval scale.
- \(A_k\): wave amplitude.
- \(\theta_k\): continuously evolving phase.
- \(\epsilon_t\): stochastic residual noise.

Use **0.4 seconds as the initial stochastic minimum**, unless an existing configuration requires a greater minimum.

This is a lower bound, not a fixed delay. Do not confuse it with the global action limit.

### Wave generation

Avoid hardcoding individual wave frequencies, amplitudes, and phases.

At initialization:

- Generate a small collection of waves, initially three unless existing configuration specifies otherwise.
- Randomise their initial phases and frequencies within sensible configurable bounds.
- Allocate amplitudes from a shared variability budget.
- Use a suitable seeded random generator already available in the project.

The number of waves should be configurable without introducing unnecessary complexity.

### Parameter evolution

Advance phases according to actual monotonic elapsed time:

\[
\theta_k \leftarrow \theta_k+2\pi f_k\Delta t
\]

Allow frequencies to drift gradually through a bounded, mean-reverting stochastic process.

Prefer fixed amplitudes initially. Avoid unnecessary amplitude-drift state.

Important requirements:

- Maintain phase continuity when frequency changes.
- Prevent unbounded parameter drift.
- Keep frequencies positive and within configured bounds.
- Avoid abrupt waveform regeneration during normal operation.
- Advance the model using actual elapsed time rather than assuming fixed intervals.
- Use monotonic time for scheduling; use Unix epochs for persistence and display.
- Handle sleeping, reconnection, and long pauses without numerical instability.

Do not numerically integrate or normalise a probability-density function. Evaluate the signal directly to produce each new interval.

Keep computation and memory effectively constant per generated interval.

---

## 3. Action and packet scheduling

Integrate stochastic timing at the **logical human-action level**, not indiscriminately across every transport packet.

Distinguish between:

- User-like logical actions, such as initiating an attack or collecting resources.
- Dependent packets belonging to one logical action.
- Protocol traffic, including heartbeats, authentication, acknowledgements, and synchronisation.

A logical action may contain multiple packets.

Preserve the existing network protocol's required ordering and timing.

### Scheduling lifecycle

Use the following conceptual sequence:

1. Complete or resolve the currently executing logical action.
2. Generate the next stochastic interval.
3. Calculate the earliest permitted execution time.
4. Wait asynchronously until that time.
5. Recheck global limits, relevant cooldowns, and current execution state.
6. Select an eligible target using the existing geographical selector.
7. Execute the action through the existing network layer.
8. Await the required completion or failure outcome.
9. Repeat.

Do not continuously poll or busy-wait.

Do not launch another sequential action while the previous one is unresolved.

Do not accumulate a backlog of overdue stochastic actions.

### Strict global limits

Existing server and application restrictions always override stochastic timing.

For example, if attacks require a minimum separation of four seconds, enforce:

\[
t_{\text{next}}\geq
\max(t_{\text{stochastic}},t_{\text{last attack}}+4s)
\]

Treat four seconds as an example. Prefer the existing configured limit, if present.

If an interval would violate a limit:

- Do not send the action.
- Wait until the relevant limit permits execution.
- Revalidate the action immediately before execution.
- Do not issue catch-up bursts afterwards.
- Do not allow multiple overdue actions to fire simultaneously.

Maintain rate limits at the relevant scope: global, per action type, or per endpoint, according to existing protocol requirements.

Transport-level retries and protocol responses must not bypass these restrictions.

---

## 4. Geographical selection integration

Reuse the existing spatial selection implementation.

Maintain separation between:

- Tower eligibility and cooldown.
- Weighted geographical target selection.
- Spotlight position and velocity.
- Local depletion and relocation.
- Action timing.

The timing scheduler should not modify geographical scores.

Update spotlight and momentum only following the existing successful scheduling or execution semantics.

Changing kingdoms must reset the geographical state:

- Spotlight returns to the kingdom's main castle.
- Previous velocity is discarded.
- A new small random velocity vector is initialised.
- Previous kingdom coordinates must never influence the new kingdom.

Do not reset the stochastic timing generator unnecessarily when changing kingdoms. Timing state and geographical state have different lifecycles.

---

## 5. Development tab

Add a dedicated **Development** tab following the existing application's navigation and visual design conventions.

Use the established UI framework, theme, state management, and component patterns.

The tab should be clean, minimal, and information-dense, resembling a professional real-time systems monitoring dashboard.

Prefer a monochrome visual style with restrained accent colours, thin strokes, subtle animations, and clearly organised panels.

The tab is primarily observational.

### A. Live waveform panel

Render the current combined stochastic timing waveform.

Display:

- Recent waveform history.
- A short projected waveform continuation based on current parameters.
- Current evaluated waveform value.
- Actual generated interval.
- Baseline interval.
- Global minimum permitted interval.
- Current scheduling state.
- Countdown until the next permitted action.

Distinguish the deterministic wave component from stochastic residual noise.

Projected values must be labelled as projections, not guaranteed future intervals.

Use a scrolling time-series graph rather than redrawing an entire chart unnecessarily.

The waveform should evolve against elapsed time even when no action occurs.

Avoid high-frequency backend updates simply to animate the visualisation.

### B. Scheduler diagnostics

Show useful live operational information:

- Current action state.
- Last completed action.
- Next scheduled execution.
- Time remaining.
- Active global restriction.
- Last action interval.
- Recent execution intervals.
- Waiting, executing, cooldown-blocked, failed, or idle status.
- Current kingdom.
- Eligible tower count.
- Local spotlight availability.

Distinguish the stochastic interval from additional waiting imposed by global limits.

Do not expose credentials, tokens, authentication messages, or unnecessary sensitive packet payloads.

### C. Simplified two-dimensional kingdom map

Create a lightweight, interactive map visualisation using the existing rendering technology where practical.

This is a diagnostic spatial map, not a recreation of the game's graphical interface.

Use:

- Small white circles for tower locations.
- A distinct white square for the main castle.
- A subtle marker or outline for the current spotlight.
- A restrained indicator for the current velocity direction.
- Thin lines and minimal labels.
- A dark background consistent with the Development tab.

### Viewport

Default to a **100 × 100 logical-coordinate viewport**, centred on the relevant geographical focus.

Allow adjustment of:

- Viewport width and height or a unified zoom level.
- Pan and recenter.
- Reset to the default view.
- Centre on main castle or current spotlight.

Preserve correct coordinate proportions. Do not stretch map geometry to fill the UI.

Render only objects intersecting the visible viewport when practical.

Reuse existing tower coordinates; do not introduce a separate map database.

### D. Outbound and return movement visualisation

Display active outgoing and returning movements as directional paths between the main castle and their respective targets.

For each movement:

- Draw a subtle path between origin and destination.
- Indicate travel direction.
- Show remaining duration beside the path or in a compact contextual label.
- Show progress using a moving or progressively shortening line.
- Support both outbound and return journeys.
- Distinguish active, completed, and unavailable movement states.

For example:

**Outbound:** Main castle → Tower

**Return:** Tower → Main castle

Animation should reflect known movement timestamps and durations rather than invented progress.

Progress should be computed using:

\[
p=\operatorname{clamp}
\left(
\frac{t_{\text{now}}-t_{\text{start}}}
{t_{\text{end}}-t_{\text{start}}},
0,1
\right)
\]

Use actual timestamps when available.

If return timing is unknown, display an unknown state rather than estimating silently.

Keep animations smooth and minimal. A line that gradually disappears behind a moving marker is preferable to excessive visual effects.

Optimise for multiple simultaneous movements without creating one backend timer per movement.

Use the UI rendering loop to interpolate visual progress from timestamps.

---

## 6. Null responses and connection health

Add lightweight monitoring for missing or null responses associated with expected packet results.

Do not treat all empty payloads as failures; distinguish valid empty protocol responses from missing acknowledgements, timeouts, and unexpected null results.

Maintain a rolling one-hour count of qualifying unexpected null-response incidents.

The requested initial tolerance is **two incidents per rolling hour**.

- First qualifying incident: record and expose a warning.
- Second incident: record and indicate that the tolerance has been reached.
- Third incident within the rolling hour: stop initiating new bot actions and transition to a safe paused/error state.

Do not terminate the application process or discard data merely because the tolerance is exceeded.

Allow already-running protocol operations to resolve safely where possible.

Clearly distinguish:

- Expected empty response.
- Missing response.
- Timeout.
- Connection loss.
- Explicit server rejection.
- Unexpected null response.

Use existing error handling, logging, and reconnection infrastructure.

Make the threshold configurable, defaulting to two qualifying incidents per rolling hour unless an existing policy takes precedence.

Show the count and recent incident information in the Development tab.

Do not automatically resume activity after a threshold-triggered stop unless the existing application's recovery policy explicitly permits it.

---

## 7. State and event architecture

Prefer a single authoritative backend state for each subsystem.

Expose concise diagnostic snapshots or events for the Development tab.

Suggested conceptual groups:

**Timing state**
- Wave parameters and phase.
- Current waveform value.
- Generated interval.
- Next scheduled action timestamp.
- Active rate-limit restriction.

**Spatial state**
- Active kingdom.
- Main castle coordinates.
- Spotlight coordinates.
- Velocity.
- Current selected tower.
- Local availability.

**Movement state**
- Source and destination.
- Direction.
- Start and expected completion timestamps.
- Current status.

**Connection health**
- Recent unexpected null incidents.
- Rolling-window count.
- Error status.
- Whether action scheduling is paused.

These are conceptual fields, not mandatory new database tables or public API structures.

Use existing nomenclature wherever a matching concept already exists.

Avoid persisting values that can be cheaply derived from existing state.

Prefer an existing application event bus, subscription mechanism, or state snapshot interface instead of introducing a separate diagnostics transport.

---

## 8. Resource and performance constraints

The Development tab must not materially affect scheduler or networking performance.

Requirements:

- No busy-waiting.
- No per-tower timers.
- No per-movement backend timers for animation.
- No redundant database scans for each UI frame.
- No excessive logging or unbounded history buffers.
- Bound diagnostic history using an appropriate ring buffer.
- Reuse snapshots and event-driven updates.
- Keep rendering independent of network execution.

When the Development tab is hidden, reduce or suspend purely visual work while preserving necessary backend scheduling and health monitoring.

Use monotonic clocks for internal duration calculations and appropriate wall-clock timestamps for display.

---

## 9. Validation

Add focused tests using the project's existing testing conventions.

Verify:

- Intervals remain above the configured stochastic lower bound.
- Wave parameters stay finite and within permitted bounds.
- Frequency changes preserve phase continuity.
- Timing state handles elapsed-time jumps correctly.
- Global rate limits are never violated.
- Sequential actions do not overlap.
- Overdue actions do not generate catch-up bursts.
- Kingdom changes reset spotlight and velocity.
- Tower eligibility is checked against current cooldown values.
- Map coordinates and viewport scaling remain correct.
- Outbound and return progress are calculated correctly.
- Missing timestamps are handled safely.
- Null-response counts expire correctly after one rolling hour.
- The third qualifying incident triggers the configured safe pause.
- Closing the Development tab does not interrupt backend execution.

Use deterministic random seeds and a controllable clock for tests where appropriate.

---

## 10. Expected result

On first launch, the Development tab should provide:

1. A live, smoothly evolving stochastic waveform.
2. The actual next-action countdown and any global-limit restrictions.
3. A 100 × 100 logical map containing white circular tower markers and a square main-castle marker.
4. A visible spotlight and directional momentum indicator.
5. Animated outbound and return movement paths with remaining duration and progress.
6. Scheduler and network health information, including the rolling null-response count.

The existing automation should operate through the new timing layer without altering the underlying protocol sequence.

**Final implementation guidance:** inspect the current codebase, adapt the design to its existing structures, and prefer the smallest coherent change that delivers these capabilities. Do not force the proposed architecture onto the project when equivalent components already exist. Keep the code modular, readable, testable, and suitable for continued development.
