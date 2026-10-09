this applies for robber baron castle entities in green, ice, fire, sand NOT fortresses
# Codex Instructions — Spatial Tower Selection

Refactor the existing tower-selection algorithm into a modular, lightweight, distance-biased stochastic scheduler with a moving spotlight, directional momentum, and local depletion awareness.

Prioritise simplicity, performance, and reuse of existing infrastructure. Do not introduce clustering, spatial trees, or unnecessary abstractions.

## 1. Required state

Each tower needs only the relevant fields:

- `kingdom_id`
- `x`, `y` — map coordinates
- `cooldown_until` — Unix epoch indicating when the tower becomes available
- `home_distance` — distance from the kingdom's main castle, precomputed where appropriate

Maintain a small, kingdom-specific selection state:

- `spotlight_x`, `spotlight_y`
- `velocity_x`, `velocity_y`
- `active_kingdom`

Use existing database structures where possible. Adjust row initialisation, loading, and cooldown updates to support these fields. Avoid duplicating existing data.

The spotlight and velocity can remain in memory; they do not require database persistence.

## 2. Kingdom changes and initialisation

Whenever the active kingdom changes:

1. Discard the previous kingdom's spotlight and velocity state.
2. Initialise the spotlight at the main castle's coordinates in the newly selected kingdom.
3. Generate a random initial velocity vector with a small, bounded magnitude and uniformly random direction.
4. Begin selecting towers using the new kingdom's data.

The initial velocity creates a weak directional preference without requiring historical selections.

Do not carry geographical momentum between kingdoms.

## 3. Tower selection

Perform a single linear scan over towers belonging to the active kingdom.

Exclude towers whose `cooldown_until > now`.

For every eligible tower, calculate:

- Distance from the main castle (preferably cached).
- Distance from the current spotlight.

Assign a weight:

\[
w_i=\frac{1}{(d_{\text{home},i}+c)(d_{\text{spotlight},i}+c)}
\]

Here, `c` is a small positive offset preventing division by zero.

Use **weighted reservoir sampling** to select one tower in the same traversal:

- Maintain a cumulative weight.
- For each eligible tower, replace the current candidate with probability `weight / cumulative_weight`.
- Return the final candidate after the scan.

This produces an exact weighted random sample in \(O(n)\) time and \(O(1)\) additional working memory.

Avoid sorting, building candidate lists, or performing a separate probability-normalisation pass.

## 4. Local depletion

During that same scan, track how many eligible towers lie within a configurable radius of the spotlight.

This represents the remaining local resources.

- If local resources remain, retain the current geographical focus.
- When the local region becomes depleted, allow the spotlight to relocate towards another eligible tower.
- Select relocation candidates using home-distance weighting, independently of spotlight proximity.

Maintain a second weighted reservoir during the same scan for relocation candidates, so no additional traversal is needed.

Prefer a simple local-availability condition over a full potential-field calculation.

## 5. Spotlight momentum

Use velocity, not acceleration.

After choosing a tower, update the spotlight with a lightweight directional-persistence rule:

\[
v_{t+1}=\mu v_t+(1-\mu)(x_{\text{selected}}-x_{\text{previous}})
\]

\[
p_{t+1}=x_{\text{selected}}+\gamma v_{t+1}
\]

Apply independently to X and Y.

- `μ` controls how much previous velocity persists.
- `γ` controls how far the spotlight looks ahead.
- Clamp velocity to prevent excessive geographical jumps.
- If no previous selection exists, retain the random initial velocity.

When the current region is depleted, relocate the spotlight to the selected relocation candidate and reset velocity to a small random vector.

The spotlight should generally follow productive geographical regions rather than jumping independently between every selected tower.

## 6. Database integration

Preserve the existing database architecture.

Ensure:

- Tower coordinates and cooldown epochs are loaded correctly.
- Home distances are computed or cached when needed.
- Cooldown epochs are updated through the existing attack lifecycle.
- Selection excludes towers still on cooldown.
- Kingdom changes properly initialise the new geographical state.
- New or migrated rows have valid initial values.

Do not introduce a separate persistent momentum database unless the existing architecture requires it.

## 7. Modularity

Separate responsibilities into small logical components:

- `initialize_spotlight` — kingdom initialisation and random velocity.
- `select_tower` — availability filtering, weighting, and reservoir sampling.
- `update_spotlight` — position and velocity updates.
- `reset_spotlight` — relocation and kingdom transitions.
- Existing database layer — tower loading and cooldown persistence.

Adapt function names and module boundaries to the existing codebase rather than creating unnecessary new files.

## 8. Performance and correctness

Target:

- **O(n)** selection time.
- **O(1)** additional memory.
- One traversal per selection.
- No sorting or clustering.
- No allocations proportional to the number of candidates.
- Precomputed home distances where beneficial.

Handle empty candidate sets, zero distances, invalid coordinates, and kingdom transitions safely.

Only update geographical state after a tower has actually been accepted for scheduling. Preserve the existing behaviour if the selection or scheduling operation fails.

## 9. Expected behaviour

The resulting scheduler should:

1. Prefer towers closer to the main castle.
2. Exhibit a probabilistic preference for towers near its current spotlight.
3. Gradually move through geographical concentrations of towers.
4. Remain in productive regions while eligible towers remain.
5. Relocate when local resources are exhausted.
6. Maintain weak directional momentum without explicit clustering.
7. Completely reset geographical momentum when switching kingdoms.

Keep tuning parameters minimal, with sensible named defaults.

**Implementation priority:** clean integration, minimal state, and computational efficiency over elaborate simulation. Reuse existing structures wherever possible.
