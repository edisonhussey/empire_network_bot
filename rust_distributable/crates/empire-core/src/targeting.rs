//! Which ready Robber Baron castle to attack next.
//!
//! This is for tower entities in the four kingdoms (green, ice, fire, sand), not
//! fortresses. It is pure and database-free: the store streams the towers that are
//! already eligible (cooldown over, not leased, level in range) past a [`Scanner`],
//! which keeps the winner as it goes. Choosing a target costs no network requests;
//! the only packets are the ones that attack it.
//!
//! # The `advanced` algorithm: a moving spotlight
//!
//! A tower's weight is
//!
//! ```text
//! w = 1 / ((d_home + c) * (d_spotlight + c))
//! ```
//!
//! (`d_home + c` is raised to [`SpotlightParams::home_exponent`], which is 1 by
//! default, so this is the formula as written.) `d_home` is its distance from the
//! kingdom's main castle, `d_spotlight` its
//! distance from a point that follows productive ground. Near towers are strongly
//! preferred, near-the-spotlight towers more so, and everything stays possible, so
//! the choice is distance-biased and random rather than a fixed order.
//!
//! One pass chooses with weighted reservoir sampling: keep a running total of the
//! weights and let each eligible tower replace the current pick with probability
//! `weight / total`. That is an exact weighted draw in O(n) time and O(1) extra
//! memory; there is no sorting, no candidate list and no normalising pass.
//!
//! The same pass counts eligible towers within [`SpotlightParams::local_radius`] of
//! the spotlight and keeps a second reservoir weighted by home distance alone. While
//! anything is left locally the spotlight stays; once the neighbourhood is used up,
//! the second pick is where it relocates.
//!
//! After a tower is actually accepted the spotlight moves with velocity, not
//! acceleration:
//!
//! ```text
//! v' = mu * v + (1 - mu) * (selected - previous_selected)
//! p' = selected + gamma * v'
//! ```
//!
//! so it drifts through concentrations of towers instead of jumping between every
//! pick. Geographical state belongs to one kingdom: changing kingdom discards it and
//! starts again at that kingdom's main castle with a small random velocity.
//!
//! The geographical state is deliberately separate from timing ([`crate::timing`])
//! and from tower eligibility (the store). Nothing here changes a cooldown.

use crate::pacing::Rng;
use crate::planning::TargetAlgorithm;

impl TargetAlgorithm {
    /// Names come from the database; an unknown one falls back to the default
    /// rather than stopping a run that is already going.
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "closest" => Self::Closest,
            "random" => Self::Random,
            _ => Self::Advanced,
        }
    }
}

/// Tuning, all with named defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotlightParams {
    /// Offset `c` in the weight, tiles. Keeps a tower on top of the castle or the
    /// spotlight from taking an unbounded weight.
    pub offset: f64,
    /// Strength of the preference for towers near the main castle: the weight's
    /// home term is `(d_home + c)^home_exponent`. 1 is the specified formula. A
    /// larger value concentrates picks nearer home; see the notes in
    /// `docs/architecture/target-selection.md` for what each value does to march
    /// lengths before changing it.
    pub home_exponent: f64,
    /// Radius around the spotlight that counts as "local", tiles.
    pub local_radius: f64,
    /// Velocity persistence `mu`, 0 to 1.
    pub momentum: f64,
    /// How far ahead the spotlight looks, `gamma`.
    pub lookahead: f64,
    /// Largest velocity component, tiles per pick.
    pub max_velocity: f64,
    /// Speed of the random velocity given at (re)initialisation, tiles per pick.
    pub initial_speed: (f64, f64),
}

impl Default for SpotlightParams {
    fn default() -> Self {
        Self {
            offset: 5.0,
            home_exponent: 1.0,
            local_radius: 12.0,
            momentum: 0.7,
            lookahead: 2.0,
            max_velocity: 8.0,
            initial_speed: (1.0, 3.0),
        }
    }
}

/// A tower offered to the scanner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub x: i64,
    pub y: i64,
    pub level: Option<i64>,
    /// Epoch milliseconds of our last attack on it; 0 when never attacked.
    pub last_attacked_ms: i64,
}

/// What one pass saw, whether or not it found a tower.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanStats {
    /// Eligible towers offered.
    pub eligible: u64,
    /// Of those, how many lie within the local radius of the spotlight (counted
    /// only by `advanced`).
    pub local: u64,
}

/// The chosen tower and whether the spotlight should relocate to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub tower: Candidate,
    /// True when the neighbourhood of the spotlight was used up, so the tower is
    /// a relocation target rather than a normal pick.
    pub relocate: bool,
}

/// Where the spotlight is and which way it is heading, for one kingdom.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionState {
    pub active_kingdom: Option<i64>,
    pub spotlight: (f64, f64),
    pub velocity: (f64, f64),
    /// The previously accepted tower, used for the momentum update.
    last_selected: Option<(i64, i64)>,
}

impl SelectionState {
    /// Start a kingdom: the spotlight sits on the main castle with a small random
    /// velocity, which gives a weak directional preference with no history.
    pub fn initialize_spotlight(
        &mut self,
        kingdom_id: i64,
        main_castle: (i64, i64),
        params: &SpotlightParams,
        rng: &mut Rng,
    ) {
        self.active_kingdom = Some(kingdom_id);
        self.spotlight = (main_castle.0 as f64, main_castle.1 as f64);
        self.velocity = random_velocity(params, rng);
        self.last_selected = None;
    }

    /// Make this state belong to `kingdom_id`. Switching kingdom throws away the
    /// previous kingdom's geography entirely.
    pub fn enter_kingdom(
        &mut self,
        kingdom_id: i64,
        main_castle: (i64, i64),
        params: &SpotlightParams,
        rng: &mut Rng,
    ) {
        if self.active_kingdom != Some(kingdom_id) {
            self.initialize_spotlight(kingdom_id, main_castle, params, rng);
        }
    }

    /// Relocate to `to` after the local region ran out: the spotlight moves there
    /// and the old direction is forgotten.
    pub fn reset_spotlight(&mut self, to: (i64, i64), params: &SpotlightParams, rng: &mut Rng) {
        self.spotlight = (to.0 as f64, to.1 as f64);
        self.velocity = random_velocity(params, rng);
        self.last_selected = Some(to);
    }

    /// Move the spotlight after a tower was accepted. With no previous selection
    /// the initial random velocity is kept.
    pub fn update_spotlight(&mut self, selected: (i64, i64), params: &SpotlightParams) {
        if let Some(previous) = self.last_selected {
            let step = ((selected.0 - previous.0) as f64, (selected.1 - previous.1) as f64);
            let mu = params.momentum.clamp(0.0, 1.0);
            let limit = params.max_velocity.abs();
            self.velocity = (
                (mu * self.velocity.0 + (1.0 - mu) * step.0).clamp(-limit, limit),
                (mu * self.velocity.1 + (1.0 - mu) * step.1).clamp(-limit, limit),
            );
        }
        self.spotlight = (
            selected.0 as f64 + params.lookahead * self.velocity.0,
            selected.1 as f64 + params.lookahead * self.velocity.1,
        );
        self.last_selected = Some(selected);
    }
}

fn random_velocity(params: &SpotlightParams, rng: &mut Rng) -> (f64, f64) {
    let angle = rng.uniform(0.0, std::f64::consts::TAU);
    let speed = rng.uniform(params.initial_speed.0, params.initial_speed.1);
    (speed * angle.cos(), speed * angle.sin())
}

/// One pass over the eligible towers. Create it with [`Selector::scan`], call
/// [`Scanner::offer`] for each tower, then [`Scanner::finish`].
pub struct Scanner<'a> {
    algorithm: TargetAlgorithm,
    home: (f64, f64),
    spotlight: (f64, f64),
    params: SpotlightParams,
    rng: &'a mut Rng,
    seen: u64,
    // advanced
    total_weight: f64,
    chosen: Option<Candidate>,
    relocation_weight: f64,
    relocation: Option<Candidate>,
    local_count: u64,
    // closest
    nearest: Option<((i64, i64, i64), Candidate)>,
}

impl Scanner<'_> {
    pub fn offer(&mut self, tower: Candidate) {
        self.seen += 1;
        match self.algorithm {
            TargetAlgorithm::Random => {
                // Uniform reservoir: the n-th tower replaces the pick with 1/n.
                if self.rng.unit() * (self.seen as f64) < 1.0 {
                    self.chosen = Some(tower);
                }
            }
            TargetAlgorithm::Closest => {
                let key = (
                    (tower.x as f64 - self.home.0).abs() as i64
                        + (tower.y as f64 - self.home.1).abs() as i64,
                    tower.x,
                    tower.y,
                );
                if self.nearest.is_none_or(|(best, _)| key < best) {
                    self.nearest = Some((key, tower));
                }
            }
            TargetAlgorithm::Advanced => {
                let c = self.params.offset.max(f64::MIN_POSITIVE);
                let (x, y) = (tower.x as f64, tower.y as f64);
                let home = (x - self.home.0).hypot(y - self.home.1);
                let spot = (x - self.spotlight.0).hypot(y - self.spotlight.1);
                if !home.is_finite() || !spot.is_finite() {
                    return;
                }
                let home_term = (home + c).powf(self.params.home_exponent.max(0.0));
                let weight = 1.0 / (home_term * (spot + c));
                self.total_weight += weight;
                if self.rng.unit() * self.total_weight < weight {
                    self.chosen = Some(tower);
                }
                let weight = 1.0 / home_term;
                self.relocation_weight += weight;
                if self.rng.unit() * self.relocation_weight < weight {
                    self.relocation = Some(tower);
                }
                if spot <= self.params.local_radius {
                    self.local_count += 1;
                }
            }
        }
    }

    /// The choice, or `None` when nothing was eligible, with what the pass saw.
    pub fn finish(self) -> (Option<Choice>, ScanStats) {
        let stats = ScanStats { eligible: self.seen, local: self.local_count };
        (self.choose(), stats)
    }

    fn choose(self) -> Option<Choice> {
        match self.algorithm {
            TargetAlgorithm::Random => self.chosen.map(|tower| Choice { tower, relocate: false }),
            TargetAlgorithm::Closest => self.nearest.map(|(_, tower)| Choice { tower, relocate: false }),
            TargetAlgorithm::Advanced if self.local_count == 0 => self
                .relocation
                .or(self.chosen)
                .map(|tower| Choice { tower, relocate: true }),
            TargetAlgorithm::Advanced => self.chosen.map(|tower| Choice { tower, relocate: false }),
        }
    }
}

/// Everything target selection remembers for one stream of picks (one task).
#[derive(Debug, Clone)]
pub struct Selector {
    pub params: SpotlightParams,
    pub state: SelectionState,
    rng: Rng,
}

impl Selector {
    pub fn new(seed: u64) -> Self {
        Self { params: SpotlightParams::default(), state: SelectionState::default(), rng: Rng::seeded(seed) }
    }

    pub fn from_entropy() -> Self {
        Self { params: SpotlightParams::default(), state: SelectionState::default(), rng: Rng::from_entropy() }
    }

    /// Begin a pass for `kingdom_id`, whose main castle is `main_castle`. Entering
    /// a different kingdom resets the geography here, before any tower is seen.
    pub fn scan(
        &mut self,
        algorithm: TargetAlgorithm,
        kingdom_id: i64,
        main_castle: (i64, i64),
    ) -> Scanner<'_> {
        self.state
            .enter_kingdom(kingdom_id, main_castle, &self.params, &mut self.rng);
        Scanner {
            algorithm,
            home: (main_castle.0 as f64, main_castle.1 as f64),
            spotlight: self.state.spotlight,
            params: self.params,
            rng: &mut self.rng,
            seen: 0,
            total_weight: 0.0,
            chosen: None,
            relocation_weight: 0.0,
            relocation: None,
            local_count: 0,
            nearest: None,
        }
    }

    /// The tower was accepted for an attack: move the geography. A failed or
    /// abandoned pick must not call this, so it leaves the state as it was.
    pub fn accept(&mut self, tower: (i64, i64), relocated: bool) {
        if relocated {
            self.state.reset_spotlight(tower, &self.params, &mut self.rng);
        } else {
            self.state.update_spotlight(tower, &self.params);
        }
    }

    /// Convenience for tests and callers holding a slice.
    pub fn pick(
        &mut self,
        algorithm: TargetAlgorithm,
        kingdom_id: i64,
        main_castle: (i64, i64),
        towers: &[Candidate],
    ) -> Option<Choice> {
        let mut scan = self.scan(algorithm, kingdom_id, main_castle);
        for tower in towers {
            scan.offer(*tower);
        }
        scan.finish().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: (i64, i64) = (722, 533);

    fn tower(x: i64, y: i64) -> Candidate {
        Candidate { x, y, level: Some(61), last_attacked_ms: 0 }
    }

    /// A rough circle of towers around `HOME`, nearer ones denser.
    fn ring() -> Vec<Candidate> {
        let mut towers = Vec::new();
        for radius in [4_i64, 9, 15, 22, 30, 40, 50] {
            for step in 0..12 {
                let angle = step as f64 / 12.0 * std::f64::consts::TAU;
                towers.push(tower(
                    HOME.0 + (radius as f64 * angle.cos()) as i64,
                    HOME.1 + (radius as f64 * angle.sin()) as i64,
                ));
            }
        }
        towers
    }

    fn home_distance(tower: &Candidate) -> f64 {
        ((tower.x - HOME.0) as f64).hypot((tower.y - HOME.1) as f64)
    }

    #[test]
    fn selection_prefers_near_towers_but_is_not_a_fixed_order() {
        let towers = ring();
        let mut selector = Selector::new(1);
        let mut distances = Vec::new();
        for _ in 0..2_000 {
            let choice = selector.pick(TargetAlgorithm::Advanced, 1, HOME, &towers).unwrap();
            distances.push(home_distance(&choice.tower));
        }
        let mean = distances.iter().sum::<f64>() / distances.len() as f64;
        let uniform_mean = towers.iter().map(home_distance).sum::<f64>() / towers.len() as f64;
        assert!(mean < uniform_mean * 0.6, "mean {mean:.1} vs uniform {uniform_mean:.1}");
        let distinct = towers
            .iter()
            .filter(|candidate| {
                distances.iter().any(|d| (*d - home_distance(candidate)).abs() < 1e-9)
            })
            .count();
        assert!(distinct > 20, "many different towers get chosen, not one: {distinct}");
        // The far edge is still reachable, just rare.
        assert!(distances.iter().any(|d| *d > 35.0));
    }

    /// The reported failure, in the stochastic setting: an untouched far tower
    /// must not win merely for being untouched.
    #[test]
    fn a_larger_home_exponent_concentrates_picks_nearer_the_castle() {
        let towers = ring();
        let mean_distance = |exponent: f64| {
            let mut selector = Selector::new(21);
            selector.params.home_exponent = exponent;
            let total: f64 = (0..3_000)
                .map(|_| home_distance(&selector.pick(TargetAlgorithm::Advanced, 1, HOME, &towers).unwrap().tower))
                .sum();
            total / 3_000.0
        };
        let (specified, steeper, steepest) = (mean_distance(1.0), mean_distance(2.0), mean_distance(3.0));
        assert!(specified > steeper && steeper > steepest, "{specified:.1} > {steeper:.1} > {steepest:.1}");
    }

    #[test]
    fn untouched_far_towers_do_not_outrank_near_ones() {
        let near = Candidate { last_attacked_ms: 9_000_000, ..tower(725, 536) };
        let far = Candidate { last_attacked_ms: 0, ..tower(770, 580) };
        let mut selector = Selector::new(2);
        let near_wins = (0..500)
            .filter(|_| selector.pick(TargetAlgorithm::Advanced, 1, HOME, &[near, far]).unwrap().tower == near)
            .count();
        assert!(near_wins > 470, "near won {near_wins} of 500");
    }

    #[test]
    fn the_spotlight_pulls_selection_towards_where_it_is() {
        let towers = ring();
        let mut selector = Selector::new(3);
        // Park the spotlight on the east side, at the same range as the west.
        selector.state.initialize_spotlight(1, HOME, &selector.params.clone(), &mut Rng::seeded(1));
        selector.state.spotlight = (HOME.0 as f64 + 22.0, HOME.1 as f64);
        let east = (0..2_000)
            .filter(|_| {
                let choice = selector.pick(TargetAlgorithm::Advanced, 1, HOME, &towers).unwrap();
                choice.tower.x > HOME.0 + 8
            })
            .count();
        selector.state.spotlight = (HOME.0 as f64 - 22.0, HOME.1 as f64);
        let east_when_west = (0..2_000)
            .filter(|_| {
                let choice = selector.pick(TargetAlgorithm::Advanced, 1, HOME, &towers).unwrap();
                choice.tower.x > HOME.0 + 8
            })
            .count();
        assert!(east > east_when_west * 2, "east {east} vs {east_when_west}");
    }

    #[test]
    fn a_used_up_neighbourhood_relocates_towards_home_weighted_towers() {
        // Spotlight far away from every eligible tower: nothing local.
        let towers = vec![tower(735, 540), tower(780, 600), tower(700, 520)];
        let mut selector = Selector::new(4);
        selector.state.initialize_spotlight(1, HOME, &selector.params.clone(), &mut Rng::seeded(1));
        selector.state.spotlight = (HOME.0 as f64 + 300.0, HOME.1 as f64 + 300.0);
        let mut nearest_home = 0;
        for _ in 0..1_000 {
            let choice = selector.pick(TargetAlgorithm::Advanced, 1, HOME, &towers).unwrap();
            assert!(choice.relocate, "nothing is local, so this is a relocation");
            if choice.tower == towers[2] || choice.tower == towers[0] {
                nearest_home += 1;
            }
        }
        assert!(nearest_home > 800, "relocation prefers towers near home: {nearest_home}");
    }

    #[test]
    fn local_towers_keep_the_spotlight_where_it_is() {
        let towers = vec![tower(726, 536), tower(790, 600)];
        let mut selector = Selector::new(5);
        selector.state.initialize_spotlight(1, HOME, &selector.params.clone(), &mut Rng::seeded(1));
        for _ in 0..200 {
            assert!(!selector.pick(TargetAlgorithm::Advanced, 1, HOME, &towers).unwrap().relocate);
        }
    }

    #[test]
    fn momentum_follows_the_direction_of_recent_picks_and_is_clamped() {
        let params = SpotlightParams::default();
        let mut state = SelectionState::default();
        let mut rng = Rng::seeded(6);
        state.initialize_spotlight(1, HOME, &params, &mut rng);
        let initial = state.velocity;
        // First accepted pick: no previous one, so the initial velocity is kept.
        state.update_spotlight((730, 533), &params);
        assert_eq!(state.velocity, initial);
        assert_eq!(state.spotlight, (730.0 + 2.0 * initial.0, 533.0 + 2.0 * initial.1));
        // Marching east: x velocity trends positive, y towards zero.
        for step in 1..=12 {
            state.update_spotlight((730 + step * 6, 533), &params);
        }
        assert!(state.velocity.0 > 3.0, "{:?}", state.velocity);
        assert!(state.velocity.1.abs() < 0.5, "{:?}", state.velocity);
        assert!(state.spotlight.0 > 730.0 + 12.0 * 6.0, "looks ahead of the last pick");
        // A huge jump cannot fling the spotlight: velocity is clamped.
        state.update_spotlight((5_000, 5_000), &params);
        assert!(state.velocity.0.abs() <= params.max_velocity && state.velocity.1.abs() <= params.max_velocity);
    }

    #[test]
    fn changing_kingdom_resets_everything_geographical() {
        let params = SpotlightParams::default();
        let mut rng = Rng::seeded(7);
        let mut selector = Selector::new(8);
        selector.pick(TargetAlgorithm::Advanced, 1, HOME, &[tower(730, 540)]);
        selector.accept((730, 540), false);
        selector.accept((760, 560), false);
        let before = selector.state.clone();
        assert_eq!(before.active_kingdom, Some(1));
        assert!(before.spotlight != (HOME.0 as f64, HOME.1 as f64));
        // Same kingdom keeps its state.
        selector.pick(TargetAlgorithm::Advanced, 1, HOME, &[tower(730, 540)]);
        assert_eq!(selector.state, before);
        // A different kingdom starts at its own castle with a fresh velocity.
        let other_castle = (654, 696);
        selector.pick(TargetAlgorithm::Advanced, 2, other_castle, &[tower(660, 700)]);
        assert_eq!(selector.state.active_kingdom, Some(2));
        assert_eq!(selector.state.spotlight, (654.0, 696.0));
        assert_ne!(selector.state.velocity, before.velocity);
        let speed = selector.state.velocity.0.hypot(selector.state.velocity.1);
        assert!((params.initial_speed.0..=params.initial_speed.1).contains(&speed), "{speed}");
        let _ = &mut rng;
    }

    #[test]
    fn a_failed_pick_leaves_the_geography_alone() {
        let mut selector = Selector::new(9);
        selector.pick(TargetAlgorithm::Advanced, 1, HOME, &[tower(730, 540)]);
        let before = selector.state.clone();
        // Choosing again (the earlier pick was never accepted) changes nothing.
        selector.pick(TargetAlgorithm::Advanced, 1, HOME, &[tower(740, 545), tower(750, 550)]);
        assert_eq!(selector.state, before);
    }

    #[test]
    fn empty_and_degenerate_inputs_are_safe() {
        let mut selector = Selector::new(10);
        assert_eq!(selector.pick(TargetAlgorithm::Advanced, 1, HOME, &[]), None);
        assert_eq!(selector.pick(TargetAlgorithm::Closest, 1, HOME, &[]), None);
        assert_eq!(selector.pick(TargetAlgorithm::Random, 1, HOME, &[]), None);
        // A tower exactly on the castle and on the spotlight: no division by zero.
        let on_top = tower(HOME.0, HOME.1);
        let choice = selector.pick(TargetAlgorithm::Advanced, 1, HOME, &[on_top]).unwrap();
        assert_eq!(choice.tower, on_top);
    }

    #[test]
    fn closest_is_still_plain_manhattan_nearest() {
        let straight = tower(734, 533);
        let diagonal = tower(729, 540);
        let mut selector = Selector::new(11);
        let choice = selector.pick(TargetAlgorithm::Closest, 1, HOME, &[diagonal, straight]).unwrap();
        assert_eq!(choice.tower, straight, "12 beats 14 by Manhattan distance");
    }

    #[test]
    fn random_reaches_every_tower() {
        let towers = [tower(1, 1), tower(2, 2), tower(3, 3), tower(4, 4)];
        let mut selector = Selector::new(12);
        let seen = (0..400)
            .filter_map(|_| selector.pick(TargetAlgorithm::Random, 1, HOME, &towers))
            .map(|choice| choice.tower.x)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn unknown_algorithm_names_fall_back_to_advanced() {
        assert_eq!(TargetAlgorithm::from_name("closest"), TargetAlgorithm::Closest);
        assert_eq!(TargetAlgorithm::from_name(" Random "), TargetAlgorithm::Random);
        assert_eq!(TargetAlgorithm::from_name("advanced"), TargetAlgorithm::Advanced);
        assert_eq!(TargetAlgorithm::from_name("???"), TargetAlgorithm::Advanced);
    }
}
