use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use empire_core::{
    account::AccountIdentity,
    licence::{
        LicenceClaims, LicenceError, TrustedPublicKey, embedded_keyring, require_feature,
        verify_token,
    },
    store::{LicenceActivation, Store, StoredLicence},
};
use serde::Serialize;

#[derive(Clone)]
pub struct LicenceGate {
    store: Store,
    keys: Vec<TrustedPublicKey>,
}

/// One installed licence as the window shows it. A licence belongs to exactly one
/// game account: the signed server and main-castle coordinates say which account
/// it may be bound to, and once bound it never moves.
#[derive(Debug, Clone, Serialize)]
pub struct LicenceSummary {
    pub license_id: String,
    pub subject: Option<String>,
    pub valid: bool,
    pub reason: Option<String>,
    /// `activated` once bound to a player, `unactivated` while it still waits for
    /// the account whose coordinates it was issued for.
    pub stage: &'static str,
    pub expires_at: Option<i64>,
    pub remaining_seconds: i64,
    /// The server and main castle this licence was issued for (signed).
    pub issued_server: Option<String>,
    pub issued_main_castle: Option<(i64, i64)>,
    pub bound_server: Option<String>,
    pub bound_player_id: Option<i64>,
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
    /// Every installed licence. The fields above describe the primary one (an
    /// activated, valid licence first) so older window code keeps working.
    pub licences: Vec<LicenceSummary>,
}

struct Checked {
    stored: StoredLicence,
    claims: Result<LicenceClaims, LicenceError>,
}

impl LicenceGate {
    pub fn new(store: Store) -> anyhow::Result<Self> {
        Ok(Self {
            store,
            keys: embedded_keyring().context("embedded licence public key is invalid")?,
        })
    }

    pub async fn status(&self) -> LicenceStatus {
        let checked = match self.checked().await {
            Ok(checked) => checked,
            Err(error) => return invalid_status(error.to_string(), Vec::new()),
        };
        let activations = self.store.licence_activations().await.unwrap_or_default();
        let summaries = checked
            .iter()
            .map(|item| summarise(item, &activations))
            .collect::<Vec<_>>();
        // Primary: an activated valid licence, then any valid one, else the
        // first problem (so the window can say what is wrong).
        let primary = checked
            .iter()
            .filter(|item| item.claims.is_ok())
            .max_by_key(|item| {
                activations
                    .iter()
                    .any(|activation| activation.license_id == item.stored.license_id)
            });
        match primary {
            Some(item) => {
                let claims = item.claims.as_ref().expect("filtered to valid").clone();
                let activation = activations
                    .iter()
                    .find(|activation| activation.license_id == claims.license_id)
                    .cloned();
                status_from_claims(claims, activation, summaries)
            }
            None => {
                let reason = checked
                    .iter()
                    .find_map(|item| item.claims.as_ref().err().map(ToString::to_string))
                    .unwrap_or_else(|| "application token required".to_owned());
                invalid_status(reason, summaries)
            }
        }
    }

    /// Add a licence, or renew an installed one. A new `license_id` is added
    /// alongside the others; it never replaces them.
    pub async fn activate(&self, token: &str) -> anyhow::Result<LicenceStatus> {
        let now = now_epoch();
        let all = self.store.licences().await?;
        let candidate_id = peek_license_id(token, &self.keys, now)?;
        let existing = all.iter().find(|item| item.license_id == candidate_id);
        let claims = verify_token(token, &self.keys, now, existing.map(|item| item.highest_seen_at))?;
        if let Some(current) = existing {
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
            highest_seen_at: existing.map_or(now, |item| item.highest_seen_at).max(now),
        };
        self.store
            .save_licence(&stored, now.saturating_mul(1_000))
            .await?;
        Ok(self.status().await)
    }

    /// Normal protected operations require some licence to have made its one-way
    /// transition to an account binding.
    pub async fn require(&self, feature: &str) -> anyhow::Result<LicenceClaims> {
        let activations = self.store.licence_activations().await?;
        let checked = self.checked().await?;
        let mut first_problem = None;
        for item in &checked {
            let bound = activations
                .iter()
                .any(|activation| activation.license_id == item.stored.license_id);
            match &item.claims {
                Ok(claims) if bound && claims.permits(feature) => return Ok(claims.clone()),
                Ok(claims) if bound => {
                    first_problem.get_or_insert_with(|| {
                        require_feature(claims, feature).unwrap_err().to_string()
                    });
                }
                Ok(_) => {}
                Err(error) => {
                    first_problem.get_or_insert_with(|| error.to_string());
                }
            }
        }
        match first_problem {
            Some(problem) => bail!(problem),
            None if checked.is_empty() => bail!("application token required"),
            None => bail!("licence is awaiting first account initialization"),
        }
    }

    /// The same check for the one licence a live session is running under, so a
    /// second account's licence expiring does not stop the first.
    pub async fn require_licence(
        &self,
        license_id: &str,
        feature: &str,
    ) -> anyhow::Result<LicenceClaims> {
        let checked = self.checked().await?;
        let item = checked
            .into_iter()
            .find(|item| item.stored.license_id == license_id)
            .context("the licence for this game account was removed")?;
        let claims = item.claims?;
        require_feature(&claims, feature)?;
        Ok(claims)
    }

    /// The bootstrap game connection is the sole capability available at
    /// stage 0, because it is how the signed coordinates are verified and the
    /// permanent player id is learned. Any valid licence will do to get that far;
    /// which one applies is decided once the account has authenticated.
    pub async fn require_bootstrap(&self, feature: &str) -> anyhow::Result<LicenceClaims> {
        let checked = self.checked().await?;
        let mut first_problem = None;
        for item in &checked {
            match &item.claims {
                Ok(claims) if claims.permits(feature) => return Ok(claims.clone()),
                Ok(claims) => {
                    first_problem.get_or_insert_with(|| {
                        require_feature(claims, feature).unwrap_err().to_string()
                    });
                }
                Err(error) => {
                    first_problem.get_or_insert_with(|| error.to_string());
                }
            }
        }
        match first_problem {
            Some(problem) => bail!(problem),
            None => bail!("application token required"),
        }
    }

    /// Pick the licence for the account that just authenticated, binding one if
    /// needed, and return its claims.
    ///
    /// 1. A licence already bound to this (server, player) wins.
    /// 2. Otherwise an unbound licence whose signed server and main-castle
    ///    coordinates match this account is bound to it, permanently.
    /// 3. Otherwise there is no licence for this account, and the error says
    ///    exactly which server and coordinates one has to be issued for.
    pub async fn bind_or_validate(
        &self,
        server: &str,
        identity: &AccountIdentity,
    ) -> anyhow::Result<LicenceClaims> {
        let server = server.trim().to_ascii_uppercase();
        let checked = self.checked().await?;
        if let Some(activation) = self
            .store
            .licence_activation_for_player(&server, identity.player_id)
            .await?
        {
            let item = checked
                .into_iter()
                .find(|item| item.stored.license_id == activation.license_id)
                .context("the licence bound to this game account was removed")?;
            let claims = item
                .claims
                .map_err(|error| anyhow::anyhow!("the licence for this game account is unusable: {error}"))?;
            require_feature(&claims, "game_network")?;
            return Ok(claims);
        }
        for item in &checked {
            let Ok(claims) = &item.claims else { continue };
            if !claims.permits("game_network")
                || claims.server.to_ascii_uppercase() != server
                || claims.bootstrap_x != identity.main_castle_x
                || claims.bootstrap_y != identity.main_castle_y
                || self.store.licence_activation(&claims.license_id).await?.is_some()
            {
                continue;
            }
            let activation = LicenceActivation {
                license_id: claims.license_id.clone(),
                server: server.clone(),
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
            return Ok(claims.clone());
        }
        bail!(
            "no licence for this game account: server {server}, player {}, main castle ({}, {}). \
             Add a licence issued for exactly that server and main castle; a licence already \
             bound to another account cannot be reused",
            identity.player_id,
            identity.main_castle_x,
            identity.main_castle_y
        )
    }

    async fn checked(&self) -> anyhow::Result<Vec<Checked>> {
        let stored = self.store.licences().await?;
        let now = now_epoch();
        let checked = stored
            .into_iter()
            .map(|stored| {
                let claims =
                    verify_token(&stored.token, &self.keys, now, Some(stored.highest_seen_at));
                Checked { stored, claims }
            })
            .collect::<Vec<_>>();
        if checked
            .iter()
            .any(|item| now > item.stored.highest_seen_at.saturating_add(60))
        {
            self.store
                .advance_highest_seen(now, now.saturating_mul(1_000))
                .await?;
        }
        Ok(checked)
    }
}

/// The licence id inside a token whose signature verifies, without applying the
/// time checks (those are done once the right history is known).
fn peek_license_id(token: &str, keys: &[TrustedPublicKey], now: i64) -> anyhow::Result<String> {
    Ok(verify_token(token, keys, now, None)?.license_id)
}

fn summarise(item: &Checked, activations: &[LicenceActivation]) -> LicenceSummary {
    let activation = activations
        .iter()
        .find(|activation| activation.license_id == item.stored.license_id);
    let now = now_epoch();
    LicenceSummary {
        license_id: item.stored.license_id.clone(),
        subject: Some(item.stored.subject.clone()),
        valid: item.claims.is_ok(),
        reason: item.claims.as_ref().err().map(ToString::to_string),
        stage: if activation.is_some() { "activated" } else { "unactivated" },
        expires_at: Some(item.stored.expires_at),
        remaining_seconds: item.stored.expires_at.saturating_sub(now).max(0),
        issued_server: item.claims.as_ref().ok().map(|claims| claims.server.clone()),
        issued_main_castle: item
            .claims
            .as_ref()
            .ok()
            .map(|claims| (claims.bootstrap_x, claims.bootstrap_y)),
        bound_server: activation.map(|value| value.server.clone()),
        bound_player_id: activation.map(|value| value.player_id),
    }
}

fn invalid_status(reason: String, licences: Vec<LicenceSummary>) -> LicenceStatus {
    LicenceStatus {
        active: false,
        stage: "invalid",
        reason: Some(reason),
        license_id: None,
        subject: None,
        tier: None,
        revision: None,
        expires_at: None,
        remaining_seconds: 0,
        features: Vec::new(),
        bound_server: None,
        bound_player_id: None,
        licences,
    }
}

fn status_from_claims(
    claims: LicenceClaims,
    activation: Option<LicenceActivation>,
    licences: Vec<LicenceSummary>,
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
        licences,
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
        token_for(signing, now, "ventrilo-stage-zero", "Ventrilo", "US1", 509, 405, 1)
    }

    #[allow(clippy::too_many_arguments)]
    fn token_for(
        signing: &SigningKey,
        now: i64,
        license_id: &str,
        subject: &str,
        server: &str,
        x: i64,
        y: i64,
        revision: u64,
    ) -> String {
        let claims = LicenceClaims {
            schema: CLAIMS_SCHEMA,
            key_id: "test-key".to_owned(),
            license_id: license_id.to_owned(),
            subject: subject.to_owned(),
            server: server.to_owned(),
            bootstrap_x: x,
            bootstrap_y: y,
            issued_at: now,
            not_before: now,
            expires_at: now + 86_400,
            revision,
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

    async fn test_gate() -> (LicenceGate, SigningKey) {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let signing = SigningKey::from_bytes(&[9; 32]);
        let gate = LicenceGate {
            store,
            keys: vec![TrustedPublicKey {
                key_id: "test-key".to_owned(),
                key: signing.verifying_key(),
            }],
        };
        (gate, signing)
    }

    fn identity(player_id: i64, x: i64, y: i64) -> AccountIdentity {
        AccountIdentity { player_id, main_castle_id: 1, main_castle_x: x, main_castle_y: y }
    }

    /// The 8 Oct failure: pingpoko (WORLD2) logged in and was refused with
    /// "licence is bound to a different game account", because the only
    /// installed licence belonged to ventrilo and there was nowhere to add another.
    #[tokio::test]
    async fn each_game_account_gets_its_own_licence() {
        let (gate, signing) = test_gate().await;
        let now = now_epoch();
        gate.activate(&token_for(&signing, now, "ventrilo-stage0", "Ventrilo", "US1", 509, 405, 1))
            .await
            .unwrap();
        let ventrilo = identity(16_862_926, 509, 405);
        let first = gate.bind_or_validate("US1", &ventrilo).await.unwrap();
        assert_eq!(first.license_id, "ventrilo-stage0");

        // Another account has no licence yet; the error names what to issue.
        let pingpoko = identity(27_461_983, 722, 533);
        let error = gate.bind_or_validate("WORLD2", &pingpoko).await.unwrap_err().to_string();
        assert!(error.contains("server WORLD2") && error.contains("main castle (722, 533)"), "{error}");
        assert!(error.contains("player 27461983"), "{error}");

        // Adding its licence leaves the first one in place.
        let status = gate
            .activate(&token_for(&signing, now, "pingpoko-stage0", "Pingpoko", "WORLD2", 722, 533, 1))
            .await
            .unwrap();
        assert_eq!(status.licences.len(), 2);
        let second = gate.bind_or_validate("WORLD2", &pingpoko).await.unwrap();
        assert_eq!(second.license_id, "pingpoko-stage0");
        // Each account keeps resolving to its own, in any order.
        assert_eq!(gate.bind_or_validate("US1", &ventrilo).await.unwrap().license_id, "ventrilo-stage0");
        assert_eq!(gate.bind_or_validate("WORLD2", &pingpoko).await.unwrap().license_id, "pingpoko-stage0");
        let status = gate.status().await;
        assert!(status.licences.iter().all(|licence| licence.stage == "activated"));
        assert!(gate.require("game_network").await.is_ok());
    }

    #[tokio::test]
    async fn a_licence_issued_for_other_coordinates_does_not_bind() {
        let (gate, signing) = test_gate().await;
        let now = now_epoch();
        // Right server, wrong main castle.
        gate.activate(&token_for(&signing, now, "pingpoko-stage0", "Pingpoko", "WORLD2", 700, 500, 1))
            .await
            .unwrap();
        assert!(gate.bind_or_validate("WORLD2", &identity(1, 722, 533)).await.is_err());
        // Right coordinates, wrong server.
        assert!(gate.bind_or_validate("US1", &identity(1, 700, 500)).await.is_err());
        assert_eq!(gate.status().await.licences[0].stage, "unactivated");
        // The unbound licence reports what it is waiting for.
        let waiting = &gate.status().await.licences[0];
        assert_eq!(waiting.issued_server.as_deref(), Some("WORLD2"));
        assert_eq!(waiting.issued_main_castle, Some((700, 500)));
        assert!(gate.bind_or_validate("WORLD2", &identity(1, 700, 500)).await.is_ok());
    }

    #[tokio::test]
    async fn renewing_one_licence_does_not_touch_the_others() {
        let (gate, signing) = test_gate().await;
        let now = now_epoch();
        gate.activate(&token_for(&signing, now, "a", "A", "US1", 1, 1, 1)).await.unwrap();
        gate.activate(&token_for(&signing, now, "b", "B", "WORLD2", 2, 2, 1)).await.unwrap();
        assert!(gate.activate(&token_for(&signing, now, "b", "B", "WORLD2", 2, 2, 2)).await.is_ok());
        assert!(gate.activate(&token_for(&signing, now, "b", "B", "WORLD2", 2, 2, 1)).await.is_err());
        let status = gate.status().await;
        assert_eq!(status.licences.len(), 2);
        assert_eq!(status.licences.iter().find(|item| item.license_id == "a").unwrap().stage, "unactivated");
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
