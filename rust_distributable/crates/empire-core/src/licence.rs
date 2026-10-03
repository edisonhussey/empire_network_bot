use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const TOKEN_PREFIX: &str = "OA1";
pub const CLAIMS_SCHEMA: u16 = 1;
pub const PRIMARY_KEY_ID: &str = "oa-main-2026";
pub const CLOCK_ROLLBACK_TOLERANCE_SECONDS: i64 = 300;
const PRIMARY_PUBLIC_KEY_B64: &str = include_str!("../assets/licence_public_key.b64");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LicenceClaims {
    pub schema: u16,
    pub key_id: String,
    pub license_id: String,
    pub subject: String,
    pub issued_at: i64,
    pub not_before: i64,
    pub expires_at: i64,
    pub revision: u64,
    pub tier: String,
    #[serde(default)]
    pub features: Vec<String>,
}

impl LicenceClaims {
    pub fn permits(&self, feature: &str) -> bool {
        self.features
            .iter()
            .any(|item| item == "*" || item == feature)
    }
}

#[derive(Clone)]
pub struct TrustedPublicKey {
    pub key_id: String,
    pub key: VerifyingKey,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LicenceError {
    #[error("licence token format is invalid")]
    Format,
    #[error("licence encoding is invalid")]
    Encoding,
    #[error("licence signature is invalid or its issuer is not trusted")]
    Signature,
    #[error("licence claims are invalid: {0}")]
    Claims(String),
    #[error("licence schema is unsupported")]
    Schema,
    #[error("licence key identifier does not match its signer")]
    KeyId,
    #[error("licence is not active yet")]
    NotActive,
    #[error("licence has expired")]
    Expired,
    #[error("system clock moved backwards")]
    ClockRollback,
    #[error("licence does not permit feature {0}")]
    Feature(String),
}

pub fn embedded_keyring() -> Result<Vec<TrustedPublicKey>, LicenceError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(PRIMARY_PUBLIC_KEY_B64.trim())
        .map_err(|_| LicenceError::Encoding)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| LicenceError::Encoding)?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|_| LicenceError::Encoding)?;
    Ok(vec![TrustedPublicKey {
        key_id: PRIMARY_KEY_ID.to_owned(),
        key,
    }])
}

pub fn verify_token(
    token: &str,
    keyring: &[TrustedPublicKey],
    now_epoch_seconds: i64,
    highest_seen_epoch_seconds: Option<i64>,
) -> Result<LicenceClaims, LicenceError> {
    let mut parts = token.trim().split('.');
    if parts.next() != Some(TOKEN_PREFIX) {
        return Err(LicenceError::Format);
    }
    let claims_text = parts.next().ok_or(LicenceError::Format)?;
    let signature_text = parts.next().ok_or(LicenceError::Format)?;
    if parts.next().is_some() || claims_text.is_empty() || signature_text.is_empty() {
        return Err(LicenceError::Format);
    }
    let claims_bytes = URL_SAFE_NO_PAD
        .decode(claims_text)
        .map_err(|_| LicenceError::Encoding)?;
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(signature_text)
        .map_err(|_| LicenceError::Encoding)?;
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| LicenceError::Signature)?;

    // Verify opaque bytes before deserializing or trusting any claim. Trying
    // the small embedded keyring avoids an unsigned key selector in the token.
    let signer = keyring
        .iter()
        .find(|trusted| trusted.key.verify(&claims_bytes, &signature).is_ok())
        .ok_or(LicenceError::Signature)?;
    let claims: LicenceClaims = serde_json::from_slice(&claims_bytes)
        .map_err(|error| LicenceError::Claims(error.to_string()))?;
    if claims.schema != CLAIMS_SCHEMA {
        return Err(LicenceError::Schema);
    }
    if claims.key_id != signer.key_id {
        return Err(LicenceError::KeyId);
    }
    if claims.license_id.trim().is_empty()
        || claims.subject.trim().is_empty()
        || claims.tier.trim().is_empty()
        || claims.issued_at > claims.not_before
        || claims.not_before >= claims.expires_at
        || claims.revision == 0
    {
        return Err(LicenceError::Claims(
            "inconsistent required fields".to_owned(),
        ));
    }
    if highest_seen_epoch_seconds.is_some_and(|highest| {
        now_epoch_seconds.saturating_add(CLOCK_ROLLBACK_TOLERANCE_SECONDS) < highest
    }) {
        return Err(LicenceError::ClockRollback);
    }
    if now_epoch_seconds < claims.not_before {
        return Err(LicenceError::NotActive);
    }
    if now_epoch_seconds >= claims.expires_at {
        return Err(LicenceError::Expired);
    }
    Ok(claims)
}

pub fn require_feature(claims: &LicenceClaims, feature: &str) -> Result<(), LicenceError> {
    if claims.permits(feature) {
        Ok(())
    } else {
        Err(LicenceError::Feature(feature.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn claims() -> LicenceClaims {
        LicenceClaims {
            schema: CLAIMS_SCHEMA,
            key_id: "test-key".to_owned(),
            license_id: "lic-test".to_owned(),
            subject: "developer".to_owned(),
            issued_at: 100,
            not_before: 100,
            expires_at: 200,
            revision: 1,
            tier: "pro".to_owned(),
            features: vec!["game_network".to_owned()],
        }
    }

    fn signed_token(claims: &LicenceClaims) -> (String, Vec<TrustedPublicKey>) {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let claims_bytes = serde_json::to_vec(claims).unwrap();
        let signature = signing_key.sign(&claims_bytes);
        (
            format!(
                "{TOKEN_PREFIX}.{}.{}",
                URL_SAFE_NO_PAD.encode(claims_bytes),
                URL_SAFE_NO_PAD.encode(signature.to_bytes())
            ),
            vec![TrustedPublicKey {
                key_id: claims.key_id.clone(),
                key: signing_key.verifying_key(),
            }],
        )
    }

    #[test]
    fn verifies_signature_before_time_and_features() {
        let claims = claims();
        let (token, keys) = signed_token(&claims);
        assert_eq!(verify_token(&token, &keys, 150, Some(149)).unwrap(), claims);
        assert!(require_feature(&claims, "game_network").is_ok());
        assert!(matches!(
            require_feature(&claims, "admin"),
            Err(LicenceError::Feature(_))
        ));
    }

    #[test]
    fn rejects_tampering_expiry_and_clock_rollback() {
        let claims = claims();
        let (token, keys) = signed_token(&claims);
        let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
        let mut payload: LicenceClaims =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&parts[1]).unwrap()).unwrap();
        payload.expires_at = 9_999_999_999;
        parts[1] = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        assert_eq!(
            verify_token(&parts.join("."), &keys, 150, None),
            Err(LicenceError::Signature)
        );
        assert_eq!(
            verify_token(&token, &keys, 100, Some(1_000)),
            Err(LicenceError::ClockRollback)
        );
    }

    #[test]
    fn rejects_expired_and_unknown_key_tokens() {
        let claims = claims();
        let (token, keys) = signed_token(&claims);
        assert_eq!(
            verify_token(&token, &keys, 200, None),
            Err(LicenceError::Expired)
        );
        assert_eq!(
            verify_token(&token, &[], 150, None),
            Err(LicenceError::Signature)
        );
    }
}
