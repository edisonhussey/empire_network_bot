use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::{PacketError, encode_client_xt, parse_xt_packet};

pub const DEFAULT_SERVER_HEADER: &str = "EmpireEx_21";

#[derive(Clone, Deserialize)]
pub struct LoginCredentials {
    pub player_name: String,
    pub portal_account_id: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub login_token: Option<String>,
    #[serde(default)]
    pub registration_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSettings {
    pub server_header: String,
    pub client_version: String,
    pub language: String,
    pub platform_id: i64,
    pub connection_time: i64,
    pub round_trip_time: i64,
    pub map: MapViewport,
    /// Map-coordinate distance from the main castle to cover in every direction.
    #[serde(default)]
    pub map_scan_radius: u16,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            server_header: DEFAULT_SERVER_HEADER.to_owned(),
            client_version: "1169011".to_owned(),
            language: "en".to_owned(),
            platform_id: 1,
            connection_time: 676,
            round_trip_time: 118,
            map: MapViewport::sands_default(),
            map_scan_radius: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MapViewport {
    pub kingdom_id: i64,
    pub left: i64,
    pub top: i64,
    pub columns: u8,
    pub rows: u8,
}

impl MapViewport {
    pub const fn sands_default() -> Self {
        Self {
            kingdom_id: 1,
            left: 572,
            top: 598,
            columns: 3,
            rows: 2,
        }
    }

    pub fn requests(self, server_header: &str) -> Result<Vec<String>, PacketError> {
        let mut packets = Vec::with_capacity(usize::from(self.columns) * usize::from(self.rows));
        for row in 0..self.rows {
            for column in 0..self.columns {
                let ax1 = self.left + i64::from(column) * 13;
                let ay1 = self.top + i64::from(row) * 13;
                packets.push(encode_client_xt(
                    server_header,
                    "gaa",
                    "1",
                    &json!({
                        "KID": self.kingdom_id,
                        "AX1": ax1,
                        "AY1": ay1,
                        "AX2": ax1 + 12,
                        "AY2": ay1 + 12,
                    }),
                )?);
            }
        }
        Ok(packets)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Disconnected,
    SocketHandshake,
    AwaitingRoom,
    AwaitingVersion,
    Authenticating,
    Authenticated,
    LoadingAccount,
    LoadingCastle,
    LoadingSands,
    SandsReady,
    Failed,
}

pub struct SessionMachine {
    credentials: LoginCredentials,
    settings: SessionSettings,
    phase: SessionPhase,
    bootstrap_sent: bool,
    kingdom_maps: Vec<MapViewport>,
    current_map_index: Option<usize>,
}

impl SessionMachine {
    pub fn new(credentials: LoginCredentials, settings: SessionSettings) -> Self {
        Self {
            credentials,
            settings,
            phase: SessionPhase::Disconnected,
            bootstrap_sent: false,
            kingdom_maps: Vec::new(),
            current_map_index: None,
        }
    }

    pub fn phase(&self) -> SessionPhase {
        self.phase
    }

    pub fn on_connected(&mut self) -> Vec<String> {
        self.phase = SessionPhase::SocketHandshake;
        vec!["<msg t='sys'><body action='verChk' r='0'><ver v='166' /></body></msg>".to_owned()]
    }

    pub fn on_server_text(&mut self, text: &str) -> Result<Vec<String>, PacketError> {
        if text.contains("action='apiOK'") || text.contains("action=\"apiOK\"") {
            self.phase = SessionPhase::AwaitingRoom;
            return Ok(vec![format!(
                "<msg t='sys'><body action='login' r='0'><login z='{}'><nick><![CDATA[]]></nick><pword><![CDATA[{}%{}%0]]></pword></login></body></msg>",
                self.settings.server_header, self.settings.client_version, self.settings.language
            )]);
        }
        if text.contains("action='joinOK'") || text.contains("action=\"joinOK\"") {
            self.phase = SessionPhase::AwaitingVersion;
            return Ok(vec![
                "<msg t='sys'><body action='roundTrip' r='1'></body></msg>".to_owned(),
                format!(
                    "%xt%{}%vck%1%{}%web-html5%<RoundHouseKick>%6.154856171990956e+307%",
                    self.settings.server_header, self.settings.client_version
                ),
            ]);
        }

        let Ok(packet) = parse_xt_packet(text) else {
            return Ok(Vec::new());
        };
        match packet.command.as_str() {
            "rlu" if self.phase == SessionPhase::AwaitingRoom => Ok(vec![
                "<msg t='sys'><body action='autoJoin' r='-1'></body></msg>".to_owned(),
            ]),
            "vck" if self.phase == SessionPhase::AwaitingVersion => {
                self.phase = SessionPhase::Authenticating;
                Ok(vec![self.login_packet()?])
            }
            "lli" if self.phase == SessionPhase::Authenticating => {
                self.phase = if packet.status.as_deref() == Some("0") {
                    SessionPhase::Authenticated
                } else {
                    SessionPhase::Failed
                };
                Ok(Vec::new())
            }
            "gbd" if !self.bootstrap_sent => {
                self.kingdom_maps = [0, 2, 1, 3]
                    .into_iter()
                    .filter_map(|kingdom_id| {
                        if self.settings.map_scan_radius == 0 {
                            if kingdom_id == 1 {
                                sands_viewport_from_bootstrap(&packet.payload)
                            } else {
                                viewport_from_bootstrap(&packet.payload, kingdom_id)
                            }
                        } else {
                            radius_viewport_from_bootstrap(
                                &packet.payload,
                                kingdom_id,
                                self.settings.map_scan_radius,
                            )
                        }
                    })
                    .collect();
                self.bootstrap_sent = true;
                self.phase = SessionPhase::LoadingCastle;
                self.bootstrap_packets()
            }
            "jaa" if self.bootstrap_sent && self.current_map_index.is_none() => {
                if self.kingdom_maps.is_empty() {
                    self.phase = SessionPhase::Failed;
                    return Ok(Vec::new());
                }
                self.current_map_index = Some(0);
                self.phase = SessionPhase::LoadingSands;
                self.map_transition_packets(self.kingdom_maps[0])
            }
            "gaa" if self.current_map_index.is_some() => {
                let index = self.current_map_index.expect("checked above");
                if packet.payload.get("KID").and_then(Value::as_i64)
                    != Some(self.kingdom_maps[index].kingdom_id)
                {
                    return Ok(Vec::new());
                }
                let next = index + 1;
                if next < self.kingdom_maps.len() {
                    self.current_map_index = Some(next);
                    self.map_transition_packets(self.kingdom_maps[next])
                } else {
                    self.phase = SessionPhase::SandsReady;
                    Ok(Vec::new())
                }
            }
            _ => Ok(Vec::new()),
        }
    }

    fn map_transition_packets(&self, map: MapViewport) -> Result<Vec<String>, PacketError> {
        let mut packets = vec![
            encode_client_xt(&self.settings.server_header, "gbl", "1", &json!({}))?,
            encode_client_xt(&self.settings.server_header, "upt", "1", &json!({}))?,
        ];
        packets.extend(map.requests(&self.settings.server_header)?);
        Ok(packets)
    }

    fn login_packet(&self) -> Result<String, PacketError> {
        let mut payload = json!({
            "CONM": self.settings.connection_time,
            "RTM": self.settings.round_trip_time,
            "ID": 0,
            "PL": self.settings.platform_id,
            "NOM": self.credentials.player_name,
            "LANG": self.settings.language,
            "DID": "0",
            "AID": self.credentials.portal_account_id,
            "KID": "",
            "REF": "https://empire.goodgamestudios.com",
            "GCI": "",
            "SID": 9,
            "PLFID": self.settings.platform_id,
        });
        let fields = payload.as_object_mut().expect("login payload is an object");
        if let Some(password) = self
            .credentials
            .password
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            fields.insert("PW".to_owned(), Value::String(password.to_owned()));
            fields.insert("LT".to_owned(), Value::Null);
        } else if let Some(token) = self
            .credentials
            .login_token
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            fields.insert("PW".to_owned(), Value::Null);
            fields.insert("LT".to_owned(), Value::String(token.to_owned()));
        }
        if let Some(token) = self
            .credentials
            .registration_token
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            fields.insert("RCT".to_owned(), Value::String(token.to_owned()));
        }
        encode_client_xt(&self.settings.server_header, "lli", "1", &payload)
    }

    fn bootstrap_packets(&self) -> Result<Vec<String>, PacketError> {
        let requests = [
            ("gbl", json!({})),
            ("jca", json!({"CID": -1, "KID": 0})),
            ("alb", json!({})),
            ("sli", json!({})),
            ("gie", json!({})),
            ("asc", json!({})),
            ("sie", json!({})),
            ("ffi", json!({"FIDS": [1]})),
            ("kli", json!({})),
        ];
        requests
            .into_iter()
            .map(|(command, payload)| {
                encode_client_xt(&self.settings.server_header, command, "1", &payload)
            })
            .collect()
    }
}

/// Area type that identifies a kingdom's main castle inside the `gbd` payload.
///
/// A home castle is type 1; an outer-kingdom castle (Sands) is type 12.
fn main_castle_area_types(kingdom_id: i64) -> &'static [i64] {
    match kingdom_id {
        0 => &[1],
        1 => &[12],
        _ => &[1, 12],
    }
}

/// Offset from a kingdom's main castle to the 3x2 viewport origin.
///
/// The tile grid always steps 13 tiles, but the offset is **kingdom-specific**.
/// Both entries are confirmed against real data, not fitted
/// (`docs/network_requests.md` §5):
///
/// - Green, main castle `507,403` -> tiles requested from `494,390` (live capture).
/// - Sands, main castle `593,613` -> tiles requested from `572,598` (live capture;
///   the castle coordinate was confirmed by the account owner).
///
/// A single constant cannot satisfy both. Do not collapse them.
fn viewport_offset(kingdom_id: i64) -> (i64, i64) {
    match kingdom_id {
        1 => (21, 15),
        _ => (13, 13),
    }
}

fn sands_viewport_from_bootstrap(payload: &Value) -> Option<MapViewport> {
    // Literal rather than `crate::account::SANDS_KINGDOM_ID`: `session` is
    // available without the `application` feature, which `account` is not.
    viewport_from_bootstrap(payload, 1)
}

/// Viewport origin for any kingdom, derived from its main castle in `gbd`.
fn viewport_from_bootstrap(payload: &Value, kingdom_id: i64) -> Option<MapViewport> {
    let (dx, dy) = viewport_offset(kingdom_id);
    let (x, y) = main_castle_coordinate(payload, kingdom_id)?;
    Some(MapViewport {
        kingdom_id,
        left: x.saturating_sub(dx),
        top: y.saturating_sub(dy),
        columns: 3,
        rows: 2,
    })
}

/// A square grid covering `radius` map coordinates around the main castle.
/// Radius 6 is exactly one 13×13 `gaa`; radius 50 is an 8×8 grid.
fn radius_viewport_from_bootstrap(
    payload: &Value,
    kingdom_id: i64,
    radius: u16,
) -> Option<MapViewport> {
    let (x, y) = main_castle_coordinate(payload, kingdom_id)?;
    let diameter = u32::from(radius).saturating_mul(2).saturating_add(1);
    let side = u8::try_from(diameter.div_ceil(13)).ok()?;
    Some(MapViewport {
        kingdom_id,
        left: x.saturating_sub(i64::from(radius)),
        top: y.saturating_sub(i64::from(radius)),
        columns: side,
        rows: side,
    })
}

fn main_castle_coordinate(payload: &Value, kingdom_id: i64) -> Option<(i64, i64)> {
    let area_types = main_castle_area_types(kingdom_id);
    let kingdoms = payload.pointer("/gcl/C")?.as_array()?;
    for kingdom in kingdoms {
        if kingdom.get("KID").and_then(Value::as_i64) != Some(kingdom_id) {
            continue;
        }
        for area in kingdom.get("AI")?.as_array()? {
            let row = area.get("AI")?.as_array()?;
            let Some(area_type) = row.first().and_then(Value::as_i64) else {
                continue;
            };
            if !area_types.contains(&area_type) {
                continue;
            }
            let x = row.get(1)?.as_i64()?;
            let y = row.get(2)?.as_i64()?;
            return Some((x, y));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine() -> SessionMachine {
        SessionMachine::new(
            LoginCredentials {
                player_name: "Player".to_owned(),
                portal_account_id: "portal-id".to_owned(),
                password: Some("password-secret".to_owned()),
                login_token: None,
                registration_token: None,
            },
            SessionSettings::default(),
        )
    }

    #[test]
    fn follows_observed_login_to_sands_sequence() {
        let mut session = machine();
        assert!(session.on_connected()[0].contains("verChk"));
        assert!(session.on_server_text("<body action='apiOK'>").unwrap()[0].contains("login"));
        assert!(
            session
                .on_server_text("%xt%rlu%-1%1%0%100000%2%Lobby%")
                .unwrap()[0]
                .contains("autoJoin")
        );
        assert_eq!(
            session
                .on_server_text("<body action='joinOK'>")
                .unwrap()
                .len(),
            2
        );
        let login = session
            .on_server_text("%xt%vck%1%0%6.1167.7%41.8.0%")
            .unwrap();
        assert!(login[0].contains("%lli%"));
        session.on_server_text("%xt%lli%1%0%").unwrap();
        assert_eq!(session.phase(), SessionPhase::Authenticated);
        let gbd = json!({
            "gcl": {"C": [
                {"KID": 0, "AI": [{"AI": [1, 509, 405, 16011862]}]},
                {"KID": 1, "AI": [{"AI": [12, 593, 613, 16366514]}]}
            ]}
        });
        let gbd_frame = format!("%xt%gbd%1%0%{}%", gbd);
        assert_eq!(session.on_server_text(&gbd_frame).unwrap().len(), 9);
        let green = session.on_server_text("%xt%jaa%1%0%{}%").unwrap();
        assert_eq!(green.len(), 8);
        assert!(green[2..].iter().all(|packet| packet.contains("\"KID\":0")));
        assert_ne!(session.phase(), SessionPhase::SandsReady);
        let sands = session
            .on_server_text("%xt%gaa%1%0%{\"KID\":0,\"AI\":[]}%")
            .unwrap();
        assert_eq!(sands.len(), 8);
        assert!(sands[2..].iter().all(|packet| packet.contains("\"KID\":1")));
        assert_eq!(session.phase(), SessionPhase::LoadingSands);
        session
            .on_server_text("%xt%gaa%1%0%{\"KID\":1,\"AI\":[]}%")
            .unwrap();
        assert_eq!(session.phase(), SessionPhase::SandsReady);
    }

    /// Sands main castle is `593,613`, and the live capture shows the client
    /// requesting tiles from `572,598` — an offset of 21/15, not the 13/13 green
    /// uses. Both numbers are real, which is why the offset is per-kingdom data.
    #[test]
    fn derives_sands_viewport_from_the_accounts_castle() {
        let payload = json!({
            "gcl": {"C": [{"KID": 1, "AI": [{"AI": [12, 593, 613, 16366514]}]}]}
        });
        let viewport = sands_viewport_from_bootstrap(&payload).unwrap();
        assert_eq!((viewport.left, viewport.top), (572, 598));
    }

    #[test]
    fn scan_radius_is_measured_in_map_coordinates() {
        let payload = json!({
            "gcl": {"C": [{"KID": 1, "AI": [{"AI": [12, 593, 613, 16366514]}]}]}
        });
        let one = radius_viewport_from_bootstrap(&payload, 1, 6).unwrap();
        assert_eq!((one.left, one.top, one.columns, one.rows), (587, 607, 1, 1));
        assert_eq!(one.requests(DEFAULT_SERVER_HEADER).unwrap().len(), 1);

        let fifty = radius_viewport_from_bootstrap(&payload, 1, 50).unwrap();
        assert_eq!(
            (fifty.left, fifty.top, fifty.columns, fifty.rows),
            (543, 563, 8, 8)
        );
        assert_eq!(fifty.requests(DEFAULT_SERVER_HEADER).unwrap().len(), 64);
    }

    #[test]
    fn initialization_walks_every_owned_permanent_kingdom_at_the_shared_radius() {
        let settings = SessionSettings {
            map_scan_radius: 50,
            ..SessionSettings::default()
        };
        let mut session = SessionMachine::new(machine().credentials, settings);
        session.bootstrap_sent = false;
        let payload = json!({
            "gcl": {"C": [
                {"KID": 0, "AI": [{"AI": [1, 500, 400, 10]}]},
                {"KID": 1, "AI": [{"AI": [12, 600, 600, 11]}]},
                {"KID": 2, "AI": [{"AI": [12, 700, 700, 12]}]},
                {"KID": 3, "AI": [{"AI": [12, 800, 800, 13]}]}
            ]}
        });
        session
            .on_server_text(&format!("%xt%gbd%1%0%{}%", payload))
            .unwrap();

        let green = session.on_server_text("%xt%jaa%1%0%{}%").unwrap();
        assert_eq!(green.len(), 66);
        assert!(green[2..].iter().all(|packet| packet.contains("\"KID\":0")));

        for (current, next) in [(0, 2), (2, 1), (1, 3)] {
            let frames = session
                .on_server_text(&format!("%xt%gaa%1%0%{{\"KID\":{current},\"AI\":[]}}%"))
                .unwrap();
            assert_eq!(frames.len(), 66);
            assert!(
                frames[2..]
                    .iter()
                    .all(|packet| packet.contains(&format!("\"KID\":{next}")))
            );
        }
        session
            .on_server_text("%xt%gaa%1%0%{\"KID\":3,\"AI\":[]}%")
            .unwrap();
        assert_eq!(session.phase(), SessionPhase::SandsReady);
    }

    /// The green origin is not a guess: the same live capture that reported the
    /// castle at 507,403 shows the client requesting tiles from 494,390, so the
    /// offset there is 13 on both axes rather than the 21/15 Sands uses.
    #[test]
    fn green_viewport_uses_its_own_offset_not_the_sands_one() {
        let payload = json!({
            "gcl": {"C": [{"KID": 0, "AI": [{"AI": [1, 507, 403, 6771615]}]}]}
        });
        let viewport = viewport_from_bootstrap(&payload, 0).unwrap();
        assert_eq!((viewport.left, viewport.top), (494, 390));
        assert_eq!(viewport.kingdom_id, 0);
        // A single fused constant could not satisfy both this and the Sands case.
        assert_ne!(viewport_offset(0), viewport_offset(1));
    }

    #[test]
    fn a_kingdom_without_a_matching_castle_yields_no_viewport() {
        let payload = json!({"gcl": {"C": [{"KID": 0, "AI": []}]}});
        assert!(viewport_from_bootstrap(&payload, 0).is_none());
        assert!(viewport_from_bootstrap(&payload, 3).is_none());
    }

    #[test]
    fn cached_map_data_cannot_replace_live_navigation_proof() {
        let mut session = machine();
        session.phase = SessionPhase::LoadingSands;
        assert_ne!(session.phase(), SessionPhase::SandsReady);
    }
}
