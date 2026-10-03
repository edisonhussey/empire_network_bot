use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const MAX_PACKET_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XtPacket {
    pub server_header: Option<String>,
    pub command: String,
    pub request_id: String,
    pub status: Option<String>,
    pub payload: Value,
    pub extra_fields: Vec<String>,
    pub raw: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum PacketError {
    #[error("packet exceeds the {MAX_PACKET_BYTES} byte safety limit")]
    TooLarge,
    #[error("not an XT packet")]
    InvalidEnvelope,
    #[error("packet command is empty")]
    MissingCommand,
    #[error("packet payload is not valid JSON: {0}")]
    InvalidJson(String),
}

pub fn parse_xt_packet(raw: &str) -> Result<XtPacket, PacketError> {
    if raw.len() > MAX_PACKET_BYTES {
        return Err(PacketError::TooLarge);
    }
    let trimmed = raw.trim();
    if !trimmed.starts_with("%xt%") || !trimmed.ends_with('%') {
        return Err(PacketError::InvalidEnvelope);
    }
    let body = &trimmed[4..trimmed.len() - 1];
    let fields: Vec<&str> = body.split('%').collect();
    let has_server_header = fields
        .first()
        .is_some_and(|field| field.starts_with("EmpireEx_"));
    let offset = usize::from(has_server_header);
    let command = fields.get(offset).copied().unwrap_or_default();
    let request_id = fields
        .get(offset + 1)
        .copied()
        .ok_or(PacketError::InvalidEnvelope)?;
    if command.is_empty() {
        return Err(PacketError::MissingCommand);
    }
    let three_field_request = fields.len() == 3
        && fields
            .get(2)
            .is_some_and(|field| field.starts_with('{') || field.starts_with('['));
    let (status, payload_index) = if has_server_header || three_field_request {
        (None, offset + 2)
    } else {
        (
            Some(
                fields
                    .get(offset + 2)
                    .copied()
                    .ok_or(PacketError::InvalidEnvelope)?
                    .to_owned(),
            ),
            offset + 3,
        )
    };
    let payload_text = fields.get(payload_index).copied();
    let payload = payload_text
        .map(|text| serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
        .unwrap_or(Value::Null);
    Ok(XtPacket {
        server_header: has_server_header.then(|| fields[0].to_owned()),
        command: command.to_owned(),
        request_id: request_id.to_owned(),
        status,
        payload,
        extra_fields: fields
            .get(payload_index + 1..)
            .unwrap_or_default()
            .iter()
            .map(|field| (*field).to_owned())
            .collect(),
        raw: trimmed.to_owned(),
    })
}

pub fn encode_xt_request(
    command: &str,
    request_id: &str,
    payload: &Value,
) -> Result<String, PacketError> {
    if command.is_empty() {
        return Err(PacketError::MissingCommand);
    }
    let json = serde_json::to_string(payload)
        .map_err(|error| PacketError::InvalidJson(error.to_string()))?;
    Ok(format!("%xt%{command}%{request_id}%{json}%"))
}

pub fn encode_client_xt(
    server_header: &str,
    command: &str,
    request_id: &str,
    payload: &Value,
) -> Result<String, PacketError> {
    if server_header.is_empty() || command.is_empty() {
        return Err(PacketError::MissingCommand);
    }
    let json = serde_json::to_string(payload)
        .map_err(|error| PacketError::InvalidJson(error.to_string()))?;
    Ok(format!(
        "%xt%{server_header}%{command}%{request_id}%{json}%"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_request_and_response() {
        let request = parse_xt_packet("%xt%gbl%1%{}%").unwrap();
        assert_eq!(request.command, "gbl");
        assert_eq!(request.status, None);
        let response = parse_xt_packet("%xt%bup%1%0%{\"ok\":true}%").unwrap();
        assert_eq!(response.status.as_deref(), Some("0"));
        assert_eq!(response.payload, json!({"ok": true}));
    }

    #[test]
    fn encodes_request() {
        assert_eq!(
            encode_xt_request("gbl", "1", &json!({})).unwrap(),
            "%xt%gbl%1%{}%"
        );
    }

    #[test]
    fn parses_client_header_and_non_json_fields() {
        let packet =
            parse_xt_packet("%xt%EmpireEx_21%vck%1%1169011%web-html5%<RoundHouseKick>%6.1e307%")
                .unwrap();
        assert_eq!(packet.server_header.as_deref(), Some("EmpireEx_21"));
        assert_eq!(packet.command, "vck");
        assert_eq!(packet.status, None);
        assert_eq!(packet.payload, serde_json::json!(1169011));
        assert_eq!(packet.extra_fields.len(), 3);
    }

    #[test]
    fn parses_payloadless_login_acknowledgement() {
        let packet = parse_xt_packet("%xt%lli%1%0%").unwrap();
        assert_eq!(packet.command, "lli");
        assert_eq!(packet.status.as_deref(), Some("0"));
        assert_eq!(packet.payload, Value::Null);
    }
}
