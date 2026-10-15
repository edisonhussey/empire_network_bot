//! Kingdom grouping: stay in the kingdom the bot is already in, instead of alternating.
//!
//! Every attack in a kingdom other than the current one costs a map change (an extra `gaa`
//! to enter it), and tasks taken in strict rotation alternate kingdoms: fire, green, fire,
//! green. This module decides only the *order* tasks are tried in and whether to wait a
//! little for the current kingdom instead of leaving it. It does no I/O and never refuses
//! to attack: with grouping off, or when the current kingdom has nothing to offer for too
//! long, the order is the plain rotation.
//!
//! All the tunables are in [`KingdomGrouping`], read from the `automation.kingdom_grouping`
//! app-state value so they can be changed without a release:
//!
//! ```json
//! {"enabled": true, "max_streak": 12, "patience_s": 120}
//! ```

use serde_json::Value;

/// App-state key the settings are read from.
pub const SETTINGS_KEY: &str = "automation.kingdom_grouping";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KingdomGrouping {
    /// Off means the plain rotation, exactly as before grouping existed.
    pub enabled: bool,
    /// Attacks in a row from one kingdom before another kingdom is tried first. `0` means no
    /// cap: leave only when the kingdom has nothing ready for longer than `patience_ms`.
    pub max_streak: u32,
    /// How long to wait for the current kingdom (a commander coming home, a tower coming off
    /// cooldown) before leaving it for another kingdom that can attack now. `0` never waits.
    pub patience_ms: i64,
}

impl Default for KingdomGrouping {
    fn default() -> Self {
        Self { enabled: true, max_streak: 12, patience_ms: 120_000 }
    }
}

impl KingdomGrouping {
    /// Read the settings, keeping the default for anything missing or out of range.
    pub fn from_json(value: &Value) -> Self {
        let default = Self::default();
        Self {
            enabled: value.get("enabled").and_then(Value::as_bool).unwrap_or(default.enabled),
            max_streak: value
                .get("max_streak")
                .and_then(Value::as_u64)
                .map_or(default.max_streak, |streak| streak.min(10_000) as u32),
            patience_ms: value
                .get("patience_s")
                .and_then(Value::as_f64)
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                .map_or(default.patience_ms, |seconds| (seconds.min(3_600.0) * 1_000.0) as i64),
        }
    }

    fn capped(&self, streak: u32) -> bool {
        self.max_streak > 0 && streak >= self.max_streak
    }

    /// The order to try tasks in. `rotated` is the plain round-robin order; `kingdom_of` says
    /// which kingdom a task index belongs to. The result is a permutation of `rotated`, stable
    /// within each group, so the rotation among a kingdom's own tasks is kept.
    ///
    /// * Not grouping, or no current kingdom: unchanged.
    /// * Current kingdom first, others after (the usual case).
    /// * Streak at the cap: others first, the current kingdom last, to let them have a turn.
    pub fn order(
        &self,
        rotated: &[usize],
        kingdom_of: impl Fn(usize) -> i64,
        current: Option<i64>,
        streak: u32,
    ) -> Vec<usize> {
        let Some(current) = current.filter(|_| self.enabled) else {
            return rotated.to_vec();
        };
        let (here, elsewhere): (Vec<usize>, Vec<usize>) =
            rotated.iter().copied().partition(|index| kingdom_of(*index) == current);
        if self.capped(streak) {
            elsewhere.into_iter().chain(here).collect()
        } else {
            here.into_iter().chain(elsewhere).collect()
        }
    }

    /// Whether to wait for the current kingdom rather than switch now. `ready_in_ms` is how long
    /// until its first task could attack (`Some(0)` is already; `None` is nothing to wait for).
    /// Only ever true when the kingdom is not at its streak cap and the wait is within patience.
    pub fn should_wait(&self, ready_in_ms: Option<i64>, streak: u32) -> bool {
        self.enabled
            && !self.capped(streak)
            && ready_in_ms.is_some_and(|ms| ms <= self.patience_ms)
    }
}

/// How many attacks in a row have gone to one kingdom. In memory only: after a restart the
/// first attack simply starts a new run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Streak {
    kingdom: Option<i64>,
    count: u32,
}

impl Streak {
    /// An attack was acknowledged in `kingdom`.
    pub fn note_attack(&mut self, kingdom: i64) {
        if self.kingdom == Some(kingdom) {
            self.count = self.count.saturating_add(1);
        } else {
            *self = Self { kingdom: Some(kingdom), count: 1 };
        }
    }

    /// The run length for `kingdom`; zero if the last attacks went elsewhere.
    pub fn length_in(&self, kingdom: Option<i64>) -> u32 {
        if self.kingdom.is_some() && self.kingdom == kingdom { self.count } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Tasks 0..5 in kingdoms fire(3), green(0), fire, green, ice(2), fire.
    const KINGDOMS: [i64; 6] = [3, 0, 3, 0, 2, 3];
    fn kingdom_of(index: usize) -> i64 {
        KINGDOMS[index]
    }
    fn rotation(start: usize) -> Vec<usize> {
        (0..6).map(|offset| (start + offset) % 6).collect()
    }

    #[test]
    fn plain_rotation_alternates_kingdoms_which_is_the_problem_being_solved() {
        let rotated = rotation(0);
        let hops = rotated.windows(2).filter(|w| kingdom_of(w[0]) != kingdom_of(w[1])).count();
        assert_eq!(hops, 5);
    }

    #[test]
    fn the_current_kingdom_goes_first_and_the_rotation_inside_each_group_is_kept() {
        let grouping = KingdomGrouping::default();
        let order = grouping.order(&rotation(1), kingdom_of, Some(3), 1);
        assert_eq!(order, vec![2, 5, 0, 1, 3, 4], "fire tasks (rotated from 1) then the rest");
        let order = grouping.order(&rotation(1), kingdom_of, Some(0), 1);
        assert_eq!(order, vec![1, 3, 2, 4, 5, 0]);
    }

    /// Thirty attacks with every task always able to go: the kingdom changes made.
    fn changes(grouping: KingdomGrouping) -> usize {
        let (mut current, mut streak, mut hops, mut start) = (None, Streak::default(), 0, 0);
        for _ in 0..30 {
            let order = grouping.order(&rotation(start), kingdom_of, current, streak.length_in(current));
            let picked = order[0];
            start = (picked + 1) % 6;
            let kingdom = kingdom_of(picked);
            if current.is_some_and(|c| c != kingdom) {
                hops += 1;
            }
            streak.note_attack(kingdom);
            current = Some(kingdom);
        }
        hops
    }

    #[test]
    fn grouping_makes_far_fewer_kingdom_changes_than_the_rotation() {
        let off = changes(KingdomGrouping { enabled: false, ..Default::default() });
        let uncapped = changes(KingdomGrouping { max_streak: 0, ..Default::default() });
        let capped = changes(KingdomGrouping { max_streak: 12, ..Default::default() });
        assert!(off >= 20, "the plain rotation changes kingdom almost every attack: {off}");
        assert_eq!(uncapped, 0);
        assert!((1..=3).contains(&capped), "a cap of 12 over 30 attacks hands over about twice: {capped}");
    }

    #[test]
    fn the_streak_cap_hands_the_turn_to_another_kingdom() {
        let grouping = KingdomGrouping { max_streak: 3, ..Default::default() };
        assert_eq!(grouping.order(&rotation(0), kingdom_of, Some(3), 2)[0], 0, "under the cap: stay");
        let order = grouping.order(&rotation(0), kingdom_of, Some(3), 3);
        assert_eq!(order, vec![1, 3, 4, 0, 2, 5], "at the cap: the others first, fire last");
    }

    #[test]
    fn off_or_unknown_current_kingdom_leaves_the_rotation_alone() {
        let off = KingdomGrouping { enabled: false, ..Default::default() };
        assert_eq!(off.order(&rotation(2), kingdom_of, Some(3), 0), rotation(2));
        assert_eq!(KingdomGrouping::default().order(&rotation(2), kingdom_of, None, 0), rotation(2));
        assert!(KingdomGrouping::default().order(&[], kingdom_of, Some(3), 0).is_empty());
    }

    #[test]
    fn waiting_is_only_for_a_short_while_and_never_at_the_cap() {
        let grouping = KingdomGrouping { max_streak: 5, patience_ms: 90_000, ..Default::default() };
        assert!(grouping.should_wait(Some(30_000), 1));
        assert!(grouping.should_wait(Some(90_000), 1), "exactly the patience still waits");
        assert!(!grouping.should_wait(Some(90_001), 1), "too long: leave");
        assert!(!grouping.should_wait(None, 1), "nothing to wait for");
        assert!(!grouping.should_wait(Some(1_000), 5), "at the cap another kingdom must get a turn");
        assert!(!KingdomGrouping { patience_ms: 0, ..grouping }.should_wait(Some(1), 0), "zero patience never waits");
        assert!(KingdomGrouping { patience_ms: 0, ..grouping }.should_wait(Some(0), 0), "but ready now is ready");
        assert!(!KingdomGrouping { enabled: false, ..grouping }.should_wait(Some(0), 0));
    }

    #[test]
    fn the_streak_counts_a_run_and_restarts_on_a_change() {
        let mut streak = Streak::default();
        assert_eq!(streak.length_in(Some(3)), 0);
        streak.note_attack(3);
        streak.note_attack(3);
        assert_eq!((streak.length_in(Some(3)), streak.length_in(Some(0)), streak.length_in(None)), (2, 0, 0));
        streak.note_attack(0);
        assert_eq!((streak.length_in(Some(3)), streak.length_in(Some(0))), (0, 1));
    }

    #[test]
    fn settings_come_from_json_with_defaults_and_limits() {
        assert_eq!(KingdomGrouping::from_json(&json!({})), KingdomGrouping::default());
        let read = KingdomGrouping::from_json(&json!({"enabled": false, "max_streak": 4, "patience_s": 30}));
        assert_eq!(read, KingdomGrouping { enabled: false, max_streak: 4, patience_ms: 30_000 });
        let wild = KingdomGrouping::from_json(&json!({"max_streak": -1, "patience_s": -5}));
        assert_eq!(wild, KingdomGrouping::default(), "nonsense keeps the default");
        assert_eq!(KingdomGrouping::from_json(&json!({"patience_s": 99999})).patience_ms, 3_600_000);
    }
}
