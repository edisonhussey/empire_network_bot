use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use empire_core::{
    licence::{LicenceClaims, TrustedPublicKey, embedded_keyring, require_feature, verify_token},
    store::{Store, StoredLicence},
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
    pub reason: Option<String>,
    pub license_id: Option<String>,
    pub subject: Option<String>,
    pub tier: Option<String>,
    pub revision: Option<u64>,
    pub expires_at: Option<i64>,
    pub remaining_seconds: i64,
    pub features: Vec<String>,
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
            Ok(claims) => status_from_claims(claims),
            Err(error) => LicenceStatus {
                active: false,
                reason: Some(error.to_string()),
                license_id: None,
                subject: None,
                tier: None,
                revision: None,
                expires_at: None,
                remaining_seconds: 0,
                features: Vec::new(),
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
        Ok(status_from_claims(claims))
    }

    pub async fn require(&self, feature: &str) -> anyhow::Result<LicenceClaims> {
        let claims = self.current_claims().await?;
        require_feature(&claims, feature)?;
        Ok(claims)
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

fn status_from_claims(claims: LicenceClaims) -> LicenceStatus {
    LicenceStatus {
        active: true,
        reason: None,
        license_id: Some(claims.license_id),
        subject: Some(claims.subject),
        tier: Some(claims.tier),
        revision: Some(claims.revision),
        expires_at: Some(claims.expires_at),
        remaining_seconds: claims.expires_at.saturating_sub(now_epoch()),
        features: claims.features,
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
