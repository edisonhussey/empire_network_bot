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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeTaskDraft {
    pub task_id: String,
    pub commander_count: i64,
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
        if self
            .allocations
            .iter()
            .any(|allocation| allocation.commander_count < 1)
        {
            return Err(PlanError::CommanderCount);
        }
        Ok(())
    }

    pub fn normalized_allocations(&self) -> Vec<ModeTaskDraft> {
        if self.allocations.is_empty() {
            self.task_ids
                .iter()
                .map(|task_id| ModeTaskDraft {
                    task_id: task_id.clone(),
                    commander_count: 1,
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
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
