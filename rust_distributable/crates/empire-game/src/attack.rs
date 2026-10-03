//! Attack payload construction.
//!
//! Mirrors `bot/game_data/attack.py`. An attack is up to four waves; each wave
//! has three flanks; each flank carries tools and units in a fixed number of
//! slots. Unused slots are padded with `[-1, 0]` so the wire shape is exactly
//! what the game server expects.
//!
//! ```text
//! to_payload() -> [                       // one entry per wave actually present
//!   {
//!     "L": { "T": [[id, qty], ..2], "U": [[id, qty], ..2] },
//!     "M": { "T": [[id, qty], ..3], "U": [[id, qty], ..6] },
//!     "R": { "T": [[id, qty], ..2], "U": [[id, qty], ..2] },
//!   },
//!   ...
//! ]
//! ```

use serde_json::{Map, Value};

/// The filler the game uses for an unused tool or unit slot.
pub const EMPTY_SLOT: [i64; 2] = [-1, 0];

/// How many slots a flank provides. Matches `SIDE_SLOTS` in the Python source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlankSlots {
    pub tools: usize,
    pub units: usize,
}

pub const LEFT_SLOTS: FlankSlots = FlankSlots { tools: 2, units: 2 };
pub const MIDDLE_SLOTS: FlankSlots = FlankSlots { tools: 3, units: 6 };
pub const RIGHT_SLOTS: FlankSlots = FlankSlots { tools: 2, units: 2 };

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AttackError {
    #[error("amount must be greater than 0, got {0}")]
    NonPositiveAmount(i64),
    #[error("too many items for {total} slots ({provided} provided)")]
    TooManyItems { total: usize, provided: usize },
    #[error("attack payload could not be serialised")]
    Serialize,
}

/// One `[item_id, amount]` pair on a flank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub item_id: i64,
    pub amount: i64,
}

impl Slot {
    /// Build a slot, rejecting the non-positive amounts the server rejects too.
    pub fn new(item_id: i64, amount: i64) -> Result<Self, AttackError> {
        if amount <= 0 {
            return Err(AttackError::NonPositiveAmount(amount));
        }
        Ok(Self { item_id, amount })
    }

    fn to_value(self) -> Value {
        Value::Array(vec![Value::from(self.item_id), Value::from(self.amount)])
    }
}

/// One flank of a wave.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Side {
    pub tools: Vec<Slot>,
    pub units: Vec<Slot>,
}

impl Side {
    pub fn new(tools: Vec<Slot>, units: Vec<Slot>) -> Self {
        Self { tools, units }
    }

    /// A flank that brings no tools.
    pub fn units_only(units: Vec<Slot>) -> Self {
        Self {
            tools: Vec::new(),
            units,
        }
    }
}

/// One wave: up to three flanks, any of which may be absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wave {
    pub left: Option<Side>,
    pub middle: Option<Side>,
    pub right: Option<Side>,
}

impl Wave {
    pub fn new(left: Option<Side>, middle: Option<Side>, right: Option<Side>) -> Self {
        Self {
            left,
            middle,
            right,
        }
    }
}

/// An attack of one to four waves.
///
/// `wave1` is always present, matching the Python source, where an attack with
/// only `wave1` supplied still serialises exactly one wave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attack {
    pub wave1: Wave,
    pub wave2: Option<Wave>,
    pub wave3: Option<Wave>,
    pub wave4: Option<Wave>,
}

impl Attack {
    pub fn new(wave1: Wave, wave2: Option<Wave>, wave3: Option<Wave>, wave4: Option<Wave>) -> Self {
        Self {
            wave1,
            wave2,
            wave3,
            wave4,
        }
    }

    pub fn single_wave(wave1: Wave) -> Self {
        Self::new(wave1, None, None, None)
    }

    /// The `A` list the server expects.
    pub fn to_payload(&self) -> Result<Vec<Value>, AttackError> {
        let mut payload = Vec::new();
        payload.push(wave_payload(&self.wave1)?);
        for extra in [&self.wave2, &self.wave3, &self.wave4]
            .into_iter()
            .flatten()
        {
            payload.push(wave_payload(extra)?);
        }
        Ok(payload)
    }

    /// Compact JSON, as the Python `to_json()` produces it.
    pub fn to_json(&self) -> Result<String, AttackError> {
        serde_json::to_string(&self.to_payload()?).map_err(|_| AttackError::Serialize)
    }

    /// `{"A": [...]}` — the fragment embedded in a `cra` packet.
    pub fn to_attack_part(&self) -> Result<Value, AttackError> {
        let mut part = Map::new();
        part.insert("A".to_owned(), Value::Array(self.to_payload()?));
        Ok(Value::Object(part))
    }
}

fn wave_payload(wave: &Wave) -> Result<Value, AttackError> {
    let mut payload = Map::new();
    let flanks = [
        ("L", LEFT_SLOTS, wave.left.as_ref()),
        ("M", MIDDLE_SLOTS, wave.middle.as_ref()),
        ("R", RIGHT_SLOTS, wave.right.as_ref()),
    ];
    for (key, slots, side) in flanks {
        let (tools, units): (&[Slot], &[Slot]) = match side {
            Some(side) => (side.tools.as_slice(), side.units.as_slice()),
            None => (&[], &[]),
        };
        let mut flank = Map::new();
        flank.insert("T".to_owned(), padded(tools, slots.tools)?);
        flank.insert("U".to_owned(), padded(units, slots.units)?);
        payload.insert(key.to_owned(), Value::Object(flank));
    }
    Ok(Value::Object(payload))
}

fn padded(items: &[Slot], total: usize) -> Result<Value, AttackError> {
    if items.len() > total {
        return Err(AttackError::TooManyItems {
            total,
            provided: items.len(),
        });
    }
    let mut slots: Vec<Value> = items.iter().map(|slot| slot.to_value()).collect();
    slots.resize(
        total,
        Value::Array(vec![
            Value::from(EMPTY_SLOT[0]),
            Value::from(EMPTY_SLOT[1]),
        ]),
    );
    Ok(Value::Array(slots))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn slot(item_id: i64, amount: i64) -> Slot {
        Slot::new(item_id, amount).unwrap()
    }

    #[test]
    fn absent_flanks_are_padded_to_the_full_side_shape() {
        let attack = Attack::single_wave(Wave::new(
            Some(Side::units_only(vec![slot(607, 50)])),
            None,
            None,
        ));
        assert_eq!(
            serde_json::to_value(attack.to_payload().unwrap()).unwrap(),
            json!([{
                "L": {"T": [[-1, 0], [-1, 0]], "U": [[607, 50], [-1, 0]]},
                "M": {"T": [[-1, 0], [-1, 0], [-1, 0]],
                      "U": [[-1, 0], [-1, 0], [-1, 0], [-1, 0], [-1, 0], [-1, 0]]},
                "R": {"T": [[-1, 0], [-1, 0]], "U": [[-1, 0], [-1, 0]]}
            }])
        );
    }

    #[test]
    fn only_the_waves_that_are_present_are_serialised() {
        let wave = || Wave::new(Some(Side::units_only(vec![slot(1, 1)])), None, None);
        assert_eq!(Attack::single_wave(wave()).to_payload().unwrap().len(), 1);
        assert_eq!(
            Attack::new(wave(), Some(wave()), None, None)
                .to_payload()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn an_empty_wave_still_serialises_one_wave() {
        let payload = Attack::single_wave(Wave::default()).to_payload().unwrap();
        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0]["M"]["U"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn middle_flank_carries_more_slots_than_the_outer_flanks() {
        let middle = Some(Side::new(
            vec![slot(1, 1), slot(2, 2), slot(3, 3)],
            vec![
                slot(4, 1),
                slot(5, 1),
                slot(6, 1),
                slot(7, 1),
                slot(8, 1),
                slot(9, 1),
            ],
        ));
        let outer = || Some(Side::new(vec![slot(1, 1), slot(2, 2)], vec![slot(4, 1), slot(5, 1)]));
        let attack = Attack::single_wave(Wave::new(outer(), middle, outer()));
        let payload = attack.to_payload().unwrap();
        assert_eq!(payload[0]["M"]["T"].as_array().unwrap().len(), 3);
        assert_eq!(payload[0]["M"]["U"].as_array().unwrap().len(), 6);
        for flank in ["L", "R"] {
            assert_eq!(payload[0][flank]["T"].as_array().unwrap().len(), 2);
            assert_eq!(payload[0][flank]["U"].as_array().unwrap().len(), 2);
        }
    }

    #[test]
    fn too_many_items_is_rejected_rather_than_truncated() {
        let error = Attack::single_wave(Wave::new(
            Some(Side::units_only(vec![slot(1, 1), slot(2, 1), slot(3, 1)])),
            None,
            None,
        ))
        .to_payload()
        .unwrap_err();
        assert_eq!(
            error,
            AttackError::TooManyItems {
                total: 2,
                provided: 3
            }
        );
    }

    #[test]
    fn non_positive_amounts_are_rejected() {
        assert_eq!(
            Slot::new(5, 0).unwrap_err(),
            AttackError::NonPositiveAmount(0)
        );
        assert_eq!(
            Slot::new(5, -3).unwrap_err(),
            AttackError::NonPositiveAmount(-3)
        );
    }

    #[test]
    fn attack_part_wraps_the_payload_under_a() {
        let part = Attack::single_wave(Wave::default()).to_attack_part().unwrap();
        assert!(part.get("A").and_then(Value::as_array).is_some());
    }
}
