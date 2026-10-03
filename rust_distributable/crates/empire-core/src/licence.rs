use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicenceClaims {
    pub subject: String,
    pub not_before: i64,
    pub expires_at: i64,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedLicence {
    pub claims: String,
    pub signature: String,
}

#[derive(Debug, Error)]
pub enum LicenceError {
    #[error("licence encoding is invalid")]
    Encoding,
    #[error("licence signature is invalid")]
    Signature,
    #[error("licence claims are invalid: {0}")]
    Claims(String),
    #[error("licence is not active yet")]
    NotActive,
    #[error("licence has expired")]
    Expired,
}

pub fn verify(
    signed: &SignedLicence,
    public_key: &VerifyingKey,
    now_epoch_seconds: i64,
) -> Result<LicenceClaims, LicenceError> {
    let claims_bytes = URL_SAFE_NO_PAD
        .decode(&signed.claims)
        .map_err(|_| LicenceError::Encoding)?;
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(&signed.signature)
        .map_err(|_| LicenceError::Encoding)?;
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| LicenceError::Signature)?;
    public_key
        .verify(&claims_bytes, &signature)
        .map_err(|_| LicenceError::Signature)?;
    let claims: LicenceClaims = serde_json::from_slice(&claims_bytes)
        .map_err(|error| LicenceError::Claims(error.to_string()))?;
    if now_epoch_seconds < claims.not_before {
        return Err(LicenceError::NotActive);
    }
    if now_epoch_seconds >= claims.expires_at {
        return Err(LicenceError::Expired);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_licence(claims: &LicenceClaims) -> (SignedLicence, VerifyingKey) {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let claims_bytes = serde_json::to_vec(claims).unwrap();
        let signature = signing_key.sign(&claims_bytes);
        (
            SignedLicence {
                claims: URL_SAFE_NO_PAD.encode(claims_bytes),
                signature: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
            },
            signing_key.verifying_key(),
        )
    }

    #[test]
    fn verifies_signature_and_time_window() {
        let claims = LicenceClaims {
            subject: "developer".to_owned(),
            not_before: 100,
            expires_at: 200,
            features: vec!["proxy".to_owned()],
        };
        let (signed, key) = signed_licence(&claims);
        assert_eq!(verify(&signed, &key, 150).unwrap(), claims);
        assert!(matches!(
            verify(&signed, &key, 99),
            Err(LicenceError::NotActive)
        ));
        assert!(matches!(
            verify(&signed, &key, 200),
            Err(LicenceError::Expired)
        ));
    }
}
