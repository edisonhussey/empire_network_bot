use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use empire_core::{
    account::AccountIdentity,
    licence::{LicenceClaims, TrustedPublicKey, embedded_keyring, require_feature, verify_token},
    store::{LicenceActivation, Store, StoredLicence},
};
use serde::Serialize;

#[derive(Clone)]
pub struct LicenceGate {
    store: Store,
    keys: Vec<TrustedPublicKey>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LicenceStatus {
    pub active: bool,
    pub stage: &'static str,
    pub reason: Option<String>,
    pub license_id: Option<String>,
    pub subject: Option<String>,
    pub tier: Option<String>,
    pub revision: Option<u64>,
    pub expires_at: Option<i64>,
    pub remaining_seconds: i64,
    pub features: Vec<String>,
    pub bound_server: Option<String>,
    pub bound_player_id: Option<i64>,
}

impl LicenceGate {
    pub fn new(store: Store) -> anyhow::Result<Self> {
        Ok(Self {
            store,
            keys: embedded_keyring().context("embedded licence public key is invalid")?,
        })
    }

    pub async fn status(&self) -> LicenceStatus {
        match self.current_claims().await {
            Ok(claims) => {
                let activation = self
                    .store
                    .licence_activation(&claims.license_id)
                    .await
                    .ok()
                    .flatten();
                status_from_claims(claims, activation)
            }
            Err(error) => LicenceStatus {
                active: false,
                stage: "invalid",
                reason: Some(error.to_string()),
                license_id: None,
                subject: None,
                tier: None,
                revision: None,
                expires_at: None,
                remaining_seconds: 0,
                features: Vec::new(),
                bound_server: None,
                bound_player_id: None,
            },
        }
    }

    pub async fn activate(&self, token: &str) -> anyhow::Result<LicenceStatus> {
        let now = now_epoch();
        let existing = self.store.licence().await?;
        let highest = existing.as_ref().map(|item| item.highest_seen_at);
        let claims = verify_token(token, &self.keys, now, highest)?;
        if let Some(current) = existing.as_ref()
            && current.license_id == claims.license_id
        {
            let revision =
                i64::try_from(claims.revision).context("licence revision is too large")?;
            if revision < current.revision {
                bail!("licence revision is older than the installed revision")
            }
            if revision == current.revision && token.trim() != current.token {
                bail!("a different token already uses this licence revision")
            }
            if claims.expires_at < current.expires_at {
                bail!("renewal cannot shorten the installed licence")
            }
        }
        let stored = StoredLicence {
            token: token.trim().to_owned(),
            license_id: claims.license_id.clone(),
            subject: claims.subject.clone(),
            revision: i64::try_from(claims.revision).context("licence revision is too large")?,
            expires_at: claims.expires_at,
            highest_seen_at: highest.unwrap_or(now).max(now),
        };
        self.store
            .save_licence(&stored, now.saturating_mul(1_000))
            .await?;
        Ok(self.status().await)
    }

    /// Normal protected operations require the stage-0 licence to have already
    /// made its one-way transition to an account binding.
    pub async fn require(&self, feature: &str) -> anyhow::Result<LicenceClaims> {
        let claims = self.current_claims().await?;
        require_feature(&claims, feature)?;
        self.store
            .licence_activation(&claims.license_id)
            .await?
            .context("licence is awaiting first account initialization")?;
        Ok(claims)
    }

    /// The bootstrap game connection is the sole capability available at
    /// stage 0, because it is how the signed coordinates are verified and the
    /// permanent player id is learned.
    pub async fn require_bootstrap(&self, feature: &str) -> anyhow::Result<LicenceClaims> {
        let claims = self.current_claims().await?;
        require_feature(&claims, feature)?;
        Ok(claims)
    }

    pub async fn bind_or_validate(
        &self,
        server: &str,
        identity: &AccountIdentity,
    ) -> anyhow::Result<()> {
        let claims = self.require_bootstrap("game_network").await?;
        let server = server.trim().to_ascii_uppercase();
        if let Some(activation) = self.store.licence_activation(&claims.license_id).await? {
            if activation.server != server || activation.player_id != identity.player_id {
                bail!("licence is bound to a different game account")
            }
            return Ok(());
        }
        if claims.server.to_ascii_uppercase() != server
            || claims.bootstrap_x != identity.main_castle_x
            || claims.bootstrap_y != identity.main_castle_y
        {
            bail!("stage-0 activation does not match authenticated server/main castle")
        }
        let activation = LicenceActivation {
            license_id: claims.license_id.clone(),
            server,
            player_id: identity.player_id,
            activated_at: now_epoch(),
        };
        self.store
            .bind_licence(&activation, now_epoch().saturating_mul(1_000))
            .await?;
        let stored = self
            .store
            .licence_activation(&claims.license_id)
            .await?
            .context("failed to persist licence activation")?;
        if stored.server != activation.server || stored.player_id != activation.player_id {
            bail!("licence activation is already bound to another account")
        }
        Ok(())
    }

    async fn current_claims(&self) -> anyhow::Result<LicenceClaims> {
        let stored = self
            .store
            .licence()
            .await?
            .context("application token required")?;
        let now = now_epoch();
        let claims = verify_token(&stored.token, &self.keys, now, Some(stored.highest_seen_at))?;
        if now > stored.highest_seen_at.saturating_add(60) {
            self.store
                .advance_highest_seen(now, now.saturating_mul(1_000))
                .await?;
        }
        Ok(claims)
    }
}

fn status_from_claims(
    claims: LicenceClaims,
    activation: Option<LicenceActivation>,
) -> LicenceStatus {
    let stage = if activation.is_some() {
        "activated"
    } else {
        "unactivated"
    };
    LicenceStatus {
        active: true,
        stage,
        reason: None,
        license_id: Some(claims.license_id),
        subject: Some(claims.subject),
        tier: Some(claims.tier),
        revision: Some(claims.revision),
        expires_at: Some(claims.expires_at),
        remaining_seconds: claims.expires_at.saturating_sub(now_epoch()),
        features: claims.features,
        bound_server: activation.as_ref().map(|value| value.server.clone()),
        bound_player_id: activation.map(|value| value.player_id),
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use ed25519_dalek::{Signer, SigningKey};
    use empire_core::licence::{CLAIMS_SCHEMA, TOKEN_PREFIX};

    fn stage_zero_token(signing: &SigningKey, now: i64) -> String {
        let claims = LicenceClaims {
            schema: CLAIMS_SCHEMA,
            key_id: "test-key".to_owned(),
            license_id: "ventrilo-stage-zero".to_owned(),
            subject: "Ventrilo".to_owned(),
            server: "US1".to_owned(),
            bootstrap_x: 509,
            bootstrap_y: 405,
            issued_at: now,
            not_before: now,
            expires_at: now + 86_400,
            revision: 1,
            tier: "pro".to_owned(),
            features: vec!["game_network".to_owned()],
        };
        let payload = serde_json::to_vec(&claims).unwrap();
        let signature = signing.sign(&payload);
        format!(
            "{TOKEN_PREFIX}.{}.{}",
            URL_SAFE_NO_PAD.encode(payload),
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }

    #[tokio::test]
    async fn stage_zero_learns_player_once_then_ignores_relocation() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let signing = SigningKey::from_bytes(&[9; 32]);
        let gate = LicenceGate {
            store: store.clone(),
            keys: vec![TrustedPublicKey {
                key_id: "test-key".to_owned(),
                key: signing.verifying_key(),
            }],
        };
        gate.activate(&stage_zero_token(&signing, now_epoch()))
            .await
            .unwrap();
        assert_eq!(gate.status().await.stage, "unactivated");
        assert!(gate.require("game_network").await.is_err());

        let identity = AccountIdentity {
            player_id: 16_862_926,
            main_castle_id: 16_011_862,
            main_castle_x: 509,
            main_castle_y: 405,
        };
        gate.bind_or_validate("US1", &identity).await.unwrap();
        assert_eq!(gate.status().await.stage, "activated");
        assert!(gate.require("game_network").await.is_ok());

        let relocated = AccountIdentity {
            main_castle_x: 733,
            main_castle_y: 191,
            ..identity.clone()
        };
        gate.bind_or_validate("US1", &relocated).await.unwrap();
        let different_player = AccountIdentity {
            player_id: 27_461_983,
            ..relocated
        };
        assert!(
            gate.bind_or_validate("US1", &different_player)
                .await
                .is_err()
        );
    }
}
