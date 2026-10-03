use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SANDS_KINGDOM_ID: i64 = 1;
pub const RBC_AREA_TYPE: i64 = 2;

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
}
