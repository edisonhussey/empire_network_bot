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
    sands_requested: bool,
}

impl SessionMachine {
    pub fn new(credentials: LoginCredentials, settings: SessionSettings) -> Self {
        Self {
            credentials,
            settings,
            phase: SessionPhase::Disconnected,
            bootstrap_sent: false,
            sands_requested: false,
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
                self.bootstrap_sent = true;
                self.phase = SessionPhase::LoadingCastle;
                self.bootstrap_packets()
            }
            "jaa" if self.bootstrap_sent && !self.sands_requested => {
                self.sands_requested = true;
                self.phase = SessionPhase::LoadingSands;
                let mut packets = vec![encode_client_xt(
                    &self.settings.server_header,
                    "gbl",
                    "1",
                    &json!({}),
                )?];
                packets.extend(self.settings.map.requests(&self.settings.server_header)?);
                Ok(packets)
            }
            "gaa"
                if packet.payload.get("KID").and_then(Value::as_i64)
                    == Some(self.settings.map.kingdom_id) =>
            {
                self.phase = SessionPhase::SandsReady;
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
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
        assert_eq!(session.on_server_text("%xt%gbd%1%0%{}%").unwrap().len(), 9);
        assert_eq!(session.on_server_text("%xt%jaa%1%0%{}%").unwrap().len(), 7);
        session
            .on_server_text("%xt%gaa%1%0%{\"KID\":1,\"AI\":[]}%")
            .unwrap();
        assert_eq!(session.phase(), SessionPhase::SandsReady);
    }
}
