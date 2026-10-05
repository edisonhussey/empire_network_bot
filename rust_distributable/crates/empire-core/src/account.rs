use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::planning::TravelMode;

pub const SANDS_KINGDOM_ID: i64 = 1;
pub const RBC_AREA_TYPE: i64 = 2;
pub const FORTRESS_AREA_TYPE: i64 = 11;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnedCastle {
    pub kingdom_id: i64,
    pub castle_id: i64,
    pub area_type: i64,
    pub x: i64,
    pub y: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RbcTarget {
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
    pub level: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FortressTarget {
    pub kingdom_id: i64,
    pub x: i64,
    pub y: i64,
    /// NPC level carried by the map row: 45 in Sands and 55 in Fire captures.
    pub level: i64,
    /// Remaining server cooldown at observation time. Captures show this field
    /// decreasing in lockstep with wall time.
    pub cooldown_remaining_s: i64,
    /// Raw player/occupier id from the wire row.
    pub occupier_player_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountIdentity {
    pub player_id: i64,
    pub main_castle_id: i64,
    pub main_castle_x: i64,
    pub main_castle_y: i64,
}

pub fn account_identity(payload: &Value) -> Option<AccountIdentity> {
    let player_id = payload.pointer("/gpi/PID").and_then(Value::as_i64)?;
    let main = owned_castles(payload)
        .into_iter()
        .find(|castle| castle.kingdom_id == 0 && castle.area_type == 1)?;
    Some(AccountIdentity {
        player_id,
        main_castle_id: main.castle_id,
        main_castle_x: main.x,
        main_castle_y: main.y,
    })
}

/// Travel options unlocked by one owned castle. `gpc.A[].UH` is the server's
/// authoritative list and differs with that castle's stable level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CastleTravelOptions {
    pub castle_id: i64,
    pub kingdom_id: i64,
    pub unlocked_hbw: Vec<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttackTravel {
    pub hbw: i64,
    pub ptt: i64,
}

impl CastleTravelOptions {
    /// Within every observed stable family, the coin horse is the lowest HBW
    /// id (1001, 1004 or 1007). The wire array is not ordered by price.
    pub fn coin_hbw(&self) -> Option<i64> {
        self.unlocked_hbw.iter().copied().filter(|id| *id > 0).min()
    }

    /// Resolve a human travel choice against this castle's stable family.
    /// UH is sorted during parsing because the wire order is not meaningful.
    pub fn resolve(&self, mode: TravelMode) -> Option<AttackTravel> {
        let hbw = match mode {
            TravelMode::Coin => *self.unlocked_hbw.first()?,
            TravelMode::Ruby1 => *self.unlocked_hbw.get(1)?,
            TravelMode::Ruby2 => *self.unlocked_hbw.get(2)?,
            TravelMode::Feather => -1,
        };
        Some(AttackTravel {
            hbw,
            ptt: i64::from(mode == TravelMode::Feather),
        })
    }
}

pub fn owned_castles(payload: &Value) -> Vec<OwnedCastle> {
    let mut result = Vec::new();
    let Some(kingdoms) = payload.pointer("/gcl/C").and_then(Value::as_array) else {
        return result;
    };
    for kingdom in kingdoms {
        let Some(kingdom_id) = kingdom.get("KID").and_then(Value::as_i64) else {
            continue;
        };
        let Some(areas) = kingdom.get("AI").and_then(Value::as_array) else {
            continue;
        };
        for area in areas {
            let Some(row) = area.get("AI").and_then(Value::as_array) else {
                continue;
            };
            let Some(castle) = castle_row(kingdom_id, row) else {
                continue;
            };
            // Owned castles in observed GBD use home/outpost type 1/4 and
            // outer-kingdom type 12. Ignore unrelated area rows defensively.
            if matches!(castle.area_type, 1 | 4 | 12) {
                result.push(castle);
            }
        }
    }
    result
}

pub fn commander_lids(payload: &Value) -> Vec<i64> {
    payload
        .pointer("/gli/C")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("ID").and_then(Value::as_i64))
        .collect()
}

pub fn castle_travel_options(payload: &Value) -> Vec<CastleTravelOptions> {
    payload
        .pointer("/gpc/A")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let castle_id = row.get("AID").and_then(Value::as_i64)?;
            let kingdom_id = row.get("KID").and_then(Value::as_i64)?;
            let mut unlocked_hbw = row
                .get("UH")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .filter(|id| *id > 0)
                .collect::<Vec<_>>();
            unlocked_hbw.sort_unstable();
            unlocked_hbw.dedup();
            (!unlocked_hbw.is_empty()).then_some(CastleTravelOptions {
                castle_id,
                kingdom_id,
                unlocked_hbw,
            })
        })
        .collect()
}

pub fn rbc_targets(payload: &Value) -> Vec<RbcTarget> {
    let Some(kingdom_id) = payload.get("KID").and_then(Value::as_i64) else {
        return Vec::new();
    };
    payload
        .get("AI")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let row = value.as_array()?;
            if row.first()?.as_i64()? != RBC_AREA_TYPE {
                return None;
            }
            let raw_level = row.get(4)?.as_i64()?;
            Some(RbcTarget {
                kingdom_id,
                x: row.get(1)?.as_i64()?,
                y: row.get(2)?.as_i64()?,
                level: (kingdom_id == SANDS_KINGDOM_ID).then(|| sands_level(raw_level)),
            })
        })
        .collect()
}

/// Captured fortress rows have the stable shape
/// `[11, x, y, -1, level, remaining_seconds, player_id, kingdom_id]`.
/// Requiring both type 11 and the trailing kingdom marker prevents them from
/// being confused with ordinary type-2 RBC towers or another map object.
pub fn fortress_targets(payload: &Value) -> Vec<FortressTarget> {
    let Some(kingdom_id) = payload.get("KID").and_then(Value::as_i64) else {
        return Vec::new();
    };
    payload
        .get("AI")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let row = value.as_array()?;
            if row.first()?.as_i64()? != FORTRESS_AREA_TYPE || row.get(7)?.as_i64()? != kingdom_id {
                return None;
            }
            Some(FortressTarget {
                kingdom_id,
                x: row.get(1)?.as_i64()?,
                y: row.get(2)?.as_i64()?,
                level: row.get(4)?.as_i64()?,
                cooldown_remaining_s: row.get(5)?.as_i64()?.max(0),
                occupier_player_id: row.get(6)?.as_i64()?,
            })
        })
        .collect()
}

fn castle_row(kingdom_id: i64, row: &[Value]) -> Option<OwnedCastle> {
    Some(OwnedCastle {
        kingdom_id,
        area_type: row.first()?.as_i64()?,
        x: row.get(1)?.as_i64()?,
        y: row.get(2)?.as_i64()?,
        castle_id: row.get(3)?.as_i64()?,
        name: row.get(10).and_then(Value::as_str).unwrap_or("").to_owned(),
    })
}

fn sands_level(value: i64) -> i64 {
    (1.9 * (value.max(0) as f64).powf(0.555)).floor() as i64 + 35
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn learns_castles_commanders_and_only_rbc_rows() {
        let bootstrap = json!({
            "gcl":{"C":[{"KID":1,"AI":[{"AI":[12,593,613,16366514,1,5,5,3,4,1,"Castle Ventrilo"]}]}]},
            "gli":{"C":[{"ID":0},{"ID":2},{"ID":17}]}
        });
        assert_eq!(owned_castles(&bootstrap)[0].castle_id, 16_366_514);
        assert_eq!(commander_lids(&bootstrap), vec![0, 2, 17]);
        let map = json!({"KID":1,"AI":[[2,600,610,-1,50],[1,601,611,-1,50]]});
        let targets = rbc_targets(&map);
        assert_eq!(targets.len(), 1);
        assert_eq!((targets[0].x, targets[0].y), (600, 610));
    }

    #[test]
    fn gaa_distinguishes_fortress_rows_from_rbc_rows() {
        let map = json!({"KID":1,"AI":[
            [2,593,593,-1,112,-1800,1],
            [11,594,594,-1,45,38421,17185267,1],
            [11,614,614,-1,45,9100,15185330,2]
        ]});
        assert_eq!(rbc_targets(&map).len(), 1);
        assert_eq!(
            fortress_targets(&map),
            vec![FortressTarget {
                kingdom_id: 1,
                x: 594,
                y: 594,
                level: 45,
                cooldown_remaining_s: 38_421,
                occupier_player_id: 17_185_267,
            }]
        );
    }

    #[test]
    fn fire_gaa_rows_are_level_55_fortresses_with_server_cooldowns() {
        let map = serde_json::json!({"KID": 3, "AI": [
            [11,692,575,-1,55,47500,16859207,3],
            [11,672,594,-1,55,54673,17381830,3],
            [11,711,594,-1,55,54301,15185330,3],
            [11,731,575,-1,55,21252,15185330,3],
            [2,732,581,-1,0,-1,3]
        ]});
        let targets = fortress_targets(&map);
        assert_eq!(targets.len(), 4);
        assert!(targets.iter().all(|target| {
            target.kingdom_id == 3 && target.level == 55 && target.cooldown_remaining_s > 0
        }));
        assert_eq!((targets[0].x, targets[0].y), (692, 575));
        assert_eq!(targets[0].cooldown_remaining_s, 47_500);
        assert!(rbc_targets(&map).iter().all(|target| target.x != 692));
    }

    #[test]
    fn learns_permanent_identity_and_green_main_castle_from_gbd() {
        let bootstrap = json!({
            "gpi":{"PID":16862926},
            "gcl":{"C":[{"KID":0,"AI":[
                {"AI":[4,505,407,16632819,16862926,5,5,1,0,1,"outpost"]},
                {"AI":[1,509,405,16011862,16862926,7,7,7,3,1,"main"]}
            ]}]}
        });
        assert_eq!(
            account_identity(&bootstrap),
            Some(AccountIdentity {
                player_id: 16_862_926,
                main_castle_id: 16_011_862,
                main_castle_x: 509,
                main_castle_y: 405,
            })
        );
    }

    #[test]
    fn learns_per_castle_stable_family_and_coin_option() {
        let bootstrap = json!({"gpc":{"A":[
            {"AID":70499,"KID":0,"UH":[1008,1009,1007]},
            {"AID":341842,"KID":1,"UH":[1004,1005,1006]},
            {"AID":327523,"KID":2,"UH":[1001,1002,1003]}
        ]}});
        let options = castle_travel_options(&bootstrap);
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].unlocked_hbw, vec![1007, 1008, 1009]);
        assert_eq!(options[0].coin_hbw(), Some(1007));
        assert_eq!(options[1].coin_hbw(), Some(1004));
        assert_eq!(options[2].coin_hbw(), Some(1001));
        assert_eq!(
            options[1].resolve(TravelMode::Ruby1),
            Some(AttackTravel { hbw: 1005, ptt: 0 })
        );
        assert_eq!(
            options[1].resolve(TravelMode::Ruby2),
            Some(AttackTravel { hbw: 1006, ptt: 0 })
        );
        assert_eq!(
            options[1].resolve(TravelMode::Feather),
            Some(AttackTravel { hbw: -1, ptt: 1 })
        );
    }
}
