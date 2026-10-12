use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

/// Kept only for the legacy non-distributable hunt CLI. OpenAuto resolves HBW
/// per source castle from account bootstrap data and never uses this value.
pub const VENTRILO_SANDS_HBW: i64 = 1007;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    Troop,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: i64,
    pub name: &'static str,
    pub kind: CatalogKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KingdomOption {
    pub id: i64,
    pub name: &'static str,
}

pub fn catalog() -> Vec<CatalogItem> {
    let mut values = Vec::with_capacity(empire_game::TROOPS.len() + empire_game::TOOLS.len());
    values.extend(empire_game::TROOPS.iter().map(|item| CatalogItem {
        id: item.id,
        name: item.name,
        kind: CatalogKind::Troop,
    }));
    values.extend(empire_game::TOOLS.iter().map(|item| CatalogItem {
        id: item.id,
        name: item.name,
        kind: CatalogKind::Tool,
    }));
    values.sort_by(|left, right| left.name.cmp(right.name));
    values
}

pub fn kingdoms() -> Vec<KingdomOption> {
    empire_game::KINGDOMS
        .iter()
        .map(|kingdom| KingdomOption {
            id: kingdom.id,
            name: kingdom.name,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotDraft {
    pub item_id: i64,
    pub amount: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideDraft {
    #[serde(default)]
    pub troops: Vec<SlotDraft>,
    #[serde(default)]
    pub tools: Vec<SlotDraft>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveDraft {
    #[serde(default)]
    pub left: SideDraft,
    #[serde(default)]
    pub middle: SideDraft,
    #[serde(default)]
    pub right: SideDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackDraft {
    pub name: String,
    pub waves: Vec<WaveDraft>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coordinate {
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    #[default]
    Coordinate,
    MainCastle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Destination {
    Coordinate(Coordinate),
    RbcLevelRange {
        kingdom_id: i64,
        minimum: i64,
        maximum: i64,
    },
    FortressLevelRange {
        kingdom_id: i64,
        minimum: i64,
        maximum: i64,
    },
    Fortress {
        kingdom_id: i64,
    },
    BerimondCamp {
        kingdom_id: i64,
    },
}

impl Destination {
    /// Whether this destination selects fortresses rather than ordinary
    /// rebels.
    pub const fn is_fortress(&self) -> bool {
        matches!(
            self,
            Self::Fortress { .. } | Self::FortressLevelRange { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetAlgorithm {
    #[default]
    Advanced,
    Closest,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelMode {
    Coin,
    #[serde(rename = "ruby_1")]
    Ruby1,
    #[serde(rename = "ruby_2")]
    Ruby2,
    Feather,
}

impl TravelMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Coin => "coin",
            Self::Ruby1 => "ruby_1",
            Self::Ruby2 => "ruby_2",
            Self::Feather => "feather",
        }
    }

    pub fn from_stored(value: &str) -> Self {
        match value {
            "ruby_1" => Self::Ruby1,
            "ruby_2" => Self::Ruby2,
            "feather" => Self::Feather,
            _ => Self::Coin,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
    ExtraHigh,
}

impl Priority {
    pub const fn scheduler_value(self) -> i64 {
        match self {
            Self::ExtraHigh => 10,
            Self::High => 20,
            Self::Medium => 30,
            Self::Low => 40,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDraft {
    pub name: String,
    pub attack_profile_id: String,
    pub source: Coordinate,
    #[serde(default)]
    pub source_kind: SourceKind,
    pub destination: Destination,
    #[serde(default)]
    pub algorithm: TargetAlgorithm,
    pub travel: TravelMode,
    pub priority: Priority,
    pub commander_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeDraft {
    pub name: String,
    #[serde(default)]
    pub task_ids: Vec<String>,
    #[serde(default)]
    pub allocations: Vec<ModeTaskDraft>,
    /// The support object this bot carries, at most one. It defines where troops go for
    /// every castle; the `support` slots of `allocations` are the commanders kept for it.
    #[serde(default)]
    pub support_bot_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeTaskDraft {
    /// The task, or empty for a reserved-for-support range (see `support`).
    #[serde(default)]
    pub task_id: String,
    pub commander_count: i64,
    /// This slot is commanders set aside for the bot's support object, not a task.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub support: bool,
    /// The commander number (from 1) this task starts at. Without it the task
    /// follows the one before; with it the task sits there, leaving any numbers in
    /// between unused. See [`resolve_commander_ranges`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commander_start: Option<i64>,
}

/// The commanders each task of an attack bot owns, as inclusive 1-based ranges.
///
/// Tasks are taken in order. One with a `start` begins there; one without begins right
/// after the previous task's last commander. Numbers left between ranges stay unused.
/// Two tasks may never share a commander: sections are strict, and a commander on two
/// tasks would be sent by whichever asked first.
pub fn resolve_commander_ranges(
    allocations: &[(i64, Option<i64>)],
) -> Result<Vec<(i64, i64)>, PlanError> {
    let mut ranges: Vec<(i64, i64)> = Vec::with_capacity(allocations.len());
    let mut next = 1;
    for (count, start) in allocations {
        if *count < 1 {
            return Err(PlanError::CommanderCount);
        }
        let first = start.unwrap_or(next);
        if first < 1 {
            return Err(PlanError::CommanderStart);
        }
        let last = first + count - 1;
        if let Some(other) = ranges.iter().find(|(a, b)| first <= *b && *a <= last) {
            return Err(PlanError::CommanderOverlap(*other, (first, last)));
        }
        ranges.push((first, last));
        next = last + 1;
    }
    Ok(ranges)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundledTask {
    pub name: String,
    pub attack: AttackDraft,
    pub source: Coordinate,
    #[serde(default)]
    pub source_kind: SourceKind,
    pub destination: Destination,
    #[serde(default)]
    pub algorithm: TargetAlgorithm,
    pub travel: TravelMode,
    pub priority: Priority,
    pub commander_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commander_start: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeBundle {
    pub schema: u16,
    pub name: String,
    pub tasks: Vec<BundledTask>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlanError {
    #[error("name is required")]
    Name,
    #[error("an attack must contain exactly four waves")]
    WaveCount,
    #[error("{0} has too many {1} slots")]
    SlotCount(&'static str, &'static str),
    #[error("slot amounts must be greater than zero")]
    Amount,
    #[error("item {0} is not a known {1}")]
    UnknownItem(i64, &'static str),
    #[error("commander count must be greater than zero")]
    CommanderCount,
    #[error("commander numbers start at 1")]
    CommanderStart,
    #[error("commanders were reserved for support but no support is chosen")]
    SupportMissing,
    #[error("a slot with no task and no support")]
    EmptySlot,
    #[error("commanders {}–{} and {}–{} overlap: a commander can belong to one task only", .0.0, .0.1, .1.0, .1.1)]
    CommanderOverlap((i64, i64), (i64, i64)),
    #[error("RBC level range is invalid")]
    LevelRange,
    #[error("source and target must be in the same kingdom")]
    CrossKingdom,
    #[error("fortress targets are currently available only in Sands, Ice and Fire")]
    FortressKingdom,
    #[error("a mode needs at least one task")]
    EmptyMode,
}

impl AttackDraft {
    pub fn payload(&self) -> Result<Value, PlanError> {
        if self.name.trim().is_empty() {
            return Err(PlanError::Name);
        }
        if self.waves.len() != 4 {
            return Err(PlanError::WaveCount);
        }
        self.waves
            .iter()
            .map(wave_payload)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array)
    }
}

impl TaskDraft {
    pub fn validate(&self) -> Result<(), PlanError> {
        if self.name.trim().is_empty() {
            return Err(PlanError::Name);
        }
        if self.commander_count < 1 {
            return Err(PlanError::CommanderCount);
        }
        let (kingdom_id, minimum, maximum, fortress) = match self.destination {
            Destination::Coordinate(value) => (value.kingdom_id, None, None, false),
            Destination::RbcLevelRange {
                kingdom_id,
                minimum,
                maximum,
            } => (kingdom_id, Some(minimum), Some(maximum), false),
            Destination::FortressLevelRange {
                kingdom_id,
                minimum,
                maximum,
            } => (kingdom_id, Some(minimum), Some(maximum), true),
            Destination::Fortress { kingdom_id } => (kingdom_id, None, None, true),
            Destination::BerimondCamp { kingdom_id } => (kingdom_id, None, None, false),
        };
        if kingdom_id != self.source.kingdom_id {
            return Err(PlanError::CrossKingdom);
        }
        if minimum
            .zip(maximum)
            .is_some_and(|(min, max)| min < 1 || max < min)
        {
            return Err(PlanError::LevelRange);
        }
        if fortress && !matches!(kingdom_id, 1..=3) {
            return Err(PlanError::FortressKingdom);
        }
        Ok(())
    }
}

impl ModeDraft {
    pub fn validate(&self) -> Result<(), PlanError> {
        if self.name.trim().is_empty() {
            return Err(PlanError::Name);
        }
        if self.task_ids.is_empty() && self.allocations.is_empty() {
            return Err(PlanError::EmptyMode);
        }
        for allocation in &self.allocations {
            if allocation.support && self.support_bot_id.is_none() {
                return Err(PlanError::SupportMissing);
            }
            if !allocation.support && allocation.task_id.trim().is_empty() {
                return Err(PlanError::EmptySlot);
            }
        }
        resolve_commander_ranges(
            &self
                .allocations
                .iter()
                .map(|allocation| (allocation.commander_count, allocation.commander_start))
                .collect::<Vec<_>>(),
        )?;
        Ok(())
    }

    pub fn normalized_allocations(&self) -> Vec<ModeTaskDraft> {
        if self.allocations.is_empty() {
            self.task_ids
                .iter()
                .map(|task_id| ModeTaskDraft {
                    task_id: task_id.clone(),
                    commander_count: 1,
                    support: false,
                    commander_start: None,
                })
                .collect()
        } else {
            self.allocations.clone()
        }
    }
}

fn wave_payload(wave: &WaveDraft) -> Result<Value, PlanError> {
    Ok(json!({
        "L": side_payload("left", &wave.left, 2, 2)?,
        "M": side_payload("middle", &wave.middle, 6, 3)?,
        "R": side_payload("right", &wave.right, 2, 2)?,
    }))
}

fn side_payload(
    name: &'static str,
    side: &SideDraft,
    troop_limit: usize,
    tool_limit: usize,
) -> Result<Value, PlanError> {
    if side.troops.len() > troop_limit {
        return Err(PlanError::SlotCount(name, "troop"));
    }
    if side.tools.len() > tool_limit {
        return Err(PlanError::SlotCount(name, "tool"));
    }
    Ok(json!({
        "T": padded(&side.tools, tool_limit, CatalogKind::Tool)?,
        "U": padded(&side.troops, troop_limit, CatalogKind::Troop)?,
    }))
}

fn padded(slots: &[SlotDraft], limit: usize, kind: CatalogKind) -> Result<Vec<Value>, PlanError> {
    let expected = match kind {
        CatalogKind::Troop => "troop",
        CatalogKind::Tool => "tool",
    };
    let mut values = Vec::with_capacity(limit);
    for slot in slots {
        if slot.amount < 1 {
            return Err(PlanError::Amount);
        }
        let known = match kind {
            CatalogKind::Troop => empire_game::troop_by_id(slot.item_id).is_some(),
            CatalogKind::Tool => empire_game::tool_by_id(slot.item_id).is_some(),
        };
        if !known {
            return Err(PlanError::UnknownItem(slot.item_id, expected));
        }
        values.push(json!([slot.item_id, slot.amount]));
    }
    values.resize_with(limit, || json!([-1, 0]));
    Ok(values)
}

fn flank_wave(troop_id: i64, amount: i64, ladders: bool) -> WaveDraft {
    let side = SideDraft {
        troops: vec![SlotDraft {
            item_id: troop_id,
            amount,
        }],
        tools: ladders
            .then_some(SlotDraft {
                item_id: 614,
                amount: 5,
            })
            .into_iter()
            .collect(),
    };
    WaveDraft {
        left: side.clone(),
        middle: SideDraft::default(),
        right: side,
    }
}

pub fn ventrilo_sands_bundle() -> ModeBundle {
    let source = Coordinate {
        kingdom_id: 1,
        x: 593,
        y: 613,
    };
    ModeBundle {
        schema: 1,
        name: "Ventrilo Sands".to_owned(),
        tasks: vec![
            BundledTask {
                name: "Sands level 61 crossbow".to_owned(),
                attack: AttackDraft {
                    name: "Level 61 crossbow".to_owned(),
                    waves: vec![
                        WaveDraft {
                            left: SideDraft {
                                troops: vec![SlotDraft {
                                    item_id: 607,
                                    amount: 50,
                                }],
                                tools: Vec::new(),
                            },
                            ..WaveDraft::default()
                        },
                        WaveDraft::default(),
                        WaveDraft::default(),
                        WaveDraft::default(),
                    ],
                },
                source,
                source_kind: SourceKind::Coordinate,
                destination: Destination::RbcLevelRange {
                    kingdom_id: 1,
                    minimum: 61,
                    maximum: 61,
                },
                algorithm: TargetAlgorithm::Advanced,
                travel: TravelMode::Coin,
                priority: Priority::High,
                commander_count: 17,
                commander_start: None,
            },
            BundledTask {
                name: "Sands level 35–60 kunai".to_owned(),
                attack: AttackDraft {
                    name: "Kunai flank clear".to_owned(),
                    waves: (0..4).map(|_| flank_wave(35, 30, true)).collect(),
                },
                source,
                source_kind: SourceKind::Coordinate,
                destination: Destination::RbcLevelRange {
                    kingdom_id: 1,
                    minimum: 35,
                    maximum: 60,
                },
                algorithm: TargetAlgorithm::Advanced,
                travel: TravelMode::Coin,
                priority: Priority::ExtraHigh,
                commander_count: 18,
                commander_start: None,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commander_ranges_follow_each_other_unless_a_task_is_placed() {
        assert_eq!(resolve_commander_ranges(&[(10, None), (10, None), (10, None)]), Ok(vec![(1, 10), (11, 20), (21, 30)]));
        assert_eq!(resolve_commander_ranges(&[(10, None), (10, Some(21)), (5, None)]), Ok(vec![(1, 10), (21, 30), (31, 35)]), "a gap, and the next task follows the placed one");
        assert_eq!(resolve_commander_ranges(&[(3, Some(5)), (2, Some(1))]), Ok(vec![(5, 7), (1, 2)]), "order on the board need not be numeric order");
        assert_eq!(resolve_commander_ranges(&[]), Ok(vec![]));
    }

    #[test]
    fn a_commander_can_belong_to_one_task_only() {
        assert_eq!(
            resolve_commander_ranges(&[(10, None), (5, Some(8))]),
            Err(PlanError::CommanderOverlap((1, 10), (8, 12)))
        );
        assert!(matches!(resolve_commander_ranges(&[(1, Some(3)), (1, Some(3))]), Err(PlanError::CommanderOverlap(..))));
        assert_eq!(resolve_commander_ranges(&[(0, None)]), Err(PlanError::CommanderCount));
        assert_eq!(resolve_commander_ranges(&[(1, Some(0))]), Err(PlanError::CommanderStart));
        let message = PlanError::CommanderOverlap((1, 10), (8, 12)).to_string();
        assert!(message.contains("1–10") && message.contains("8–12"), "{message}");
    }

    #[test]
    fn a_mode_draft_with_overlapping_tasks_is_refused_before_it_is_saved() {
        let draft = |allocations: Vec<ModeTaskDraft>| ModeDraft { name: "m".into(), task_ids: vec![], allocations, support_bot_id: None };
        let task = |count, start| ModeTaskDraft { task_id: "t".into(), commander_count: count, support: false, commander_start: start };
        assert!(draft(vec![task(10, None), task(10, Some(11))]).validate().is_ok());
        assert!(draft(vec![task(10, None), task(10, Some(5))]).validate().is_err());
        // Old JSON without the field still reads.
        let old: ModeTaskDraft = serde_json::from_str(r#"{"task_id":"a","commander_count":3}"#).unwrap();
        assert_eq!(old.commander_start, None);
    }

    #[test]
    fn ventrilo_bundle_compiles_to_exact_wire_slot_shapes() {
        let bundle = ventrilo_sands_bundle();
        assert_eq!(bundle.tasks.len(), 2);
        for task in bundle.tasks {
            let payload = task.attack.payload().unwrap();
            assert_eq!(payload.as_array().unwrap().len(), 4);
            assert_eq!(payload[0]["L"]["T"].as_array().unwrap().len(), 2);
            assert_eq!(payload[0]["M"]["U"].as_array().unwrap().len(), 6);
        }
    }

    #[test]
    fn travel_modes_have_stable_human_facing_wire_names() {
        assert_eq!(serde_json::to_value(TravelMode::Coin).unwrap(), "coin");
        assert_eq!(serde_json::to_value(TravelMode::Ruby1).unwrap(), "ruby_1");
        assert_eq!(serde_json::to_value(TravelMode::Ruby2).unwrap(), "ruby_2");
        assert_eq!(
            serde_json::to_value(TravelMode::Feather).unwrap(),
            "feather"
        );
        assert_eq!(TravelMode::from_stored("unknown"), TravelMode::Coin);
    }

    #[test]
    fn catalog_preserves_the_complete_python_generated_data() {
        let items = catalog();
        assert_eq!(items.len(), 551);
        assert!(
            items
                .iter()
                .any(|item| item.id == 35 && item.kind == CatalogKind::Troop)
        );
        assert!(
            items
                .iter()
                .any(|item| item.id == 614 && item.kind == CatalogKind::Tool)
        );
        assert_eq!(
            kingdoms()[0],
            KingdomOption {
                id: 0,
                name: "green_kingdom"
            }
        );
        assert!(
            kingdoms()
                .iter()
                .any(|kingdom| kingdom.id == 3 && kingdom.name == "fire_kingdom")
        );
    }

    #[test]
    fn dynamic_targets_stay_in_the_source_kingdom() {
        let task = TaskDraft {
            name: "farm".to_owned(),
            attack_profile_id: "attack".to_owned(),
            source: Coordinate {
                kingdom_id: 1,
                x: 0,
                y: 0,
            },
            source_kind: SourceKind::MainCastle,
            destination: Destination::RbcLevelRange {
                kingdom_id: 2,
                minimum: 10,
                maximum: 20,
            },
            algorithm: TargetAlgorithm::Advanced,
            travel: TravelMode::Coin,
            priority: Priority::Medium,
            commander_count: 1,
        };
        assert_eq!(task.validate(), Err(PlanError::CrossKingdom));
    }

    #[test]
    fn fortress_is_restricted_to_the_three_outer_kingdoms() {
        let task = TaskDraft {
            name: "fortress".to_owned(),
            attack_profile_id: "attack".to_owned(),
            source: Coordinate {
                kingdom_id: 0,
                x: 0,
                y: 0,
            },
            source_kind: SourceKind::MainCastle,
            destination: Destination::Fortress { kingdom_id: 0 },
            algorithm: TargetAlgorithm::Advanced,
            travel: TravelMode::Coin,
            priority: Priority::Medium,
            commander_count: 1,
        };
        assert_eq!(task.validate(), Err(PlanError::FortressKingdom));
    }
}
