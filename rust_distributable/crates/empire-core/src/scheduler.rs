use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDefinition {
    pub id: String,
    pub event: TaskEvent,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskEvent {
    Recruit {
        kingdom_id: i64,
        unit_id: i64,
        batch_size: i64,
    },
    Attack {
        kingdom_id: i64,
        profile_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSubscription {
    pub task_id: String,
    pub castle_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCatalog {
    tasks: HashMap<String, TaskDefinition>,
    subscriptions: HashMap<String, Vec<i64>>,
}

impl TaskCatalog {
    pub fn new(
        tasks: impl IntoIterator<Item = TaskDefinition>,
        subscriptions: impl IntoIterator<Item = TaskSubscription>,
    ) -> Self {
        Self {
            tasks: tasks
                .into_iter()
                .map(|task| (task.id.clone(), task))
                .collect(),
            subscriptions: subscriptions
                .into_iter()
                .map(|subscription| (subscription.task_id, subscription.castle_ids))
                .collect(),
        }
    }

    pub fn subscribed_castles(&self, task_id: &str) -> &[i64] {
        self.subscriptions
            .get(task_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn task(&self, task_id: &str) -> Option<&TaskDefinition> {
        self.tasks.get(task_id).filter(|task| task.enabled)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PacketClass {
    AttackCra,
    CastleMutation,
    BackgroundRead,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimingPolicy {
    pub cra_floor_ms: i64,
    pub castle_switch_floor_ms: i64,
    pub background_floor_ms: i64,
    pub background_variance_ms: i64,
}

impl Default for TimingPolicy {
    fn default() -> Self {
        Self {
            cra_floor_ms: 4_000,
            castle_switch_floor_ms: 3_000,
            background_floor_ms: 250,
            background_variance_ms: 650,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionReservation {
    pub due_at_ms: i64,
    pub packet_class: PacketClass,
    pub castle_id: Option<i64>,
}

#[derive(Debug, Default)]
pub struct TimingGate {
    policy: TimingPolicy,
    last_cra_at_ms: Option<i64>,
    last_packet_at_ms: Option<i64>,
    last_castle: Option<(i64, i64)>,
}

impl TimingGate {
    pub fn new(policy: TimingPolicy) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }

    pub fn reserve(
        &mut self,
        requested_at_ms: i64,
        packet_class: PacketClass,
        castle_id: Option<i64>,
        variance_seed: u64,
    ) -> ActionReservation {
        let variance = if self.policy.background_variance_ms <= 0 {
            0
        } else {
            (variance_seed % (self.policy.background_variance_ms as u64 + 1)) as i64
        };
        let mut due_at_ms = requested_at_ms;
        if let Some(last_packet) = self.last_packet_at_ms {
            due_at_ms = due_at_ms.max(
                last_packet
                    .saturating_add(self.policy.background_floor_ms)
                    .saturating_add(variance),
            );
        }
        if packet_class == PacketClass::AttackCra
            && let Some(last_cra) = self.last_cra_at_ms
        {
            due_at_ms = due_at_ms.max(last_cra.saturating_add(self.policy.cra_floor_ms));
        }
        if let (Some(next_castle), Some((previous_castle, previous_at))) =
            (castle_id, self.last_castle)
            && next_castle != previous_castle
        {
            due_at_ms =
                due_at_ms.max(previous_at.saturating_add(self.policy.castle_switch_floor_ms));
        }

        self.last_packet_at_ms = Some(due_at_ms);
        if packet_class == PacketClass::AttackCra {
            self.last_cra_at_ms = Some(due_at_ms);
        }
        if let Some(castle_id) = castle_id {
            self.last_castle = Some((castle_id, due_at_ms));
        }
        ActionReservation {
            due_at_ms,
            packet_class,
            castle_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_subscriptions_remain_data_driven() {
        let catalog = TaskCatalog::new(
            [TaskDefinition {
                id: "sand_crossbow".to_owned(),
                event: TaskEvent::Recruit {
                    kingdom_id: 1,
                    unit_id: 17,
                    batch_size: 5,
                },
                enabled: true,
            }],
            [TaskSubscription {
                task_id: "sand_crossbow".to_owned(),
                castle_ids: vec![16366514, 16366513],
            }],
        );
        assert!(catalog.task("sand_crossbow").is_some());
        assert_eq!(
            catalog.subscribed_castles("sand_crossbow"),
            [16366514, 16366513]
        );
    }

    #[test]
    fn enforces_cra_and_castle_switch_floors() {
        let mut gate = TimingGate::new(TimingPolicy::default());
        let first = gate.reserve(1_000, PacketClass::AttackCra, Some(10), 0);
        let second = gate.reserve(1_100, PacketClass::AttackCra, Some(10), 1);
        let switched = gate.reserve(5_100, PacketClass::CastleMutation, Some(11), 2);
        assert!(second.due_at_ms - first.due_at_ms >= 4_000);
        assert!(switched.due_at_ms - second.due_at_ms >= 3_000);
    }

    #[test]
    fn background_reads_have_bounded_variance() {
        let policy = TimingPolicy::default();
        let mut gate = TimingGate::new(policy);
        let first = gate.reserve(0, PacketClass::BackgroundRead, None, 0);
        let second = gate.reserve(0, PacketClass::BackgroundRead, None, 417);
        let gap = second.due_at_ms - first.due_at_ms;
        assert!(gap >= policy.background_floor_ms);
        assert!(gap <= policy.background_floor_ms + policy.background_variance_ms);
    }
}
