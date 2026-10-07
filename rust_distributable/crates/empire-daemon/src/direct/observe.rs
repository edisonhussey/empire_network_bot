use super::*;

pub(super) async fn observe_account_packet(
    store: &Store,
    account_id: &str,
    raw: &str,
    rbc_scan_origins: &HashMap<i64, (i64, i64)>,
    settings: &SessionSettings,
    learn_rbc: bool,
) {
    let Ok(packet) = parse_xt_packet(raw) else {
        return;
    };
    if packet.status.as_deref().is_some_and(|status| status != "0") {
        return;
    }
    if let Err(error) = persist_navigation_packet(
        store,
        account_id,
        &packet.command,
        &packet.payload,
        now_ms(),
    )
    .await
    {
        warn!(%error, %account_id, command = %packet.command, "navigation persistence failed");
    }
    let result = match packet.command.as_str() {
        "gbd" => {
            let castles = owned_castles(&packet.payload);
            let commanders = commander_lids(&packet.payload);
            let travel_options = castle_travel_options(&packet.payload);
            if castles.is_empty() {
                return;
            }
            if let Err(error) = store
                .replace_account_bootstrap(account_id, &castles, &commanders, now_ms())
                .await
            {
                Err(error)
            } else {
                store
                    .upsert_castle_travel_options(account_id, &travel_options, now_ms())
                    .await
            }
        }
        "gaa" => {
            // Every map response says how long each tower it shows is on
            // cooldown. Keep that current for towers we already know, whether or
            // not this response is part of a scan: it is what stops the bot
            // finding out by being refused.
            let seen = rbc_targets(&packet.payload);
            let mut targets = if learn_rbc { seen.clone() } else { Vec::new() };
            // The radius is per kingdom, and a kingdom the operator did not ask
            // for is not being scanned at all, so nothing from it is kept.
            targets.retain(|target| {
                let radius = i64::from(settings.kingdom_radius(target.kingdom_id));
                radius > 0
                    && rbc_scan_origins.get(&target.kingdom_id).is_some_and(
                        |(origin_x, origin_y)| {
                            (target.x - origin_x).abs() <= radius
                                && (target.y - origin_y).abs() <= radius
                        },
                    )
            });
            let fortresses = fortress_targets(&packet.payload);
            let observed_at_ms = now_ms();
            async {
                if !targets.is_empty() {
                    store
                        .upsert_rbc_targets(account_id, &targets, observed_at_ms)
                        .await?;
                } else if !seen.is_empty() {
                    store
                        .refresh_rbc_cooldowns(account_id, &seen, observed_at_ms)
                        .await?;
                }
                if !fortresses.is_empty() {
                    store
                        .upsert_fortress_targets(account_id, &fortresses, observed_at_ms)
                        .await?;
                }
                Ok::<(), sqlx::Error>(())
            }
            .await
        }
        _ => return,
    };
    if let Err(error) = result {
        warn!(%error, %account_id, command = %packet.command, "account discovery persistence failed");
    }
}

pub(super) fn permanent_scan_origins(
    account_id: &str,
    castles: &[empire_core::store::OwnedCastleRecord],
) -> HashMap<i64, (i64, i64)> {
    castles
        .iter()
        .filter(|castle| castle.account_id.eq_ignore_ascii_case(account_id))
        .filter(|castle| {
            (castle.kingdom_id == 0 && castle.area_type == 1)
                || (castle.kingdom_id != 0 && castle.area_type == 12)
        })
        .map(|castle| (castle.kingdom_id, (castle.x, castle.y)))
        .collect()
}

pub(super) async fn persist_navigation_packet(
    store: &Store,
    account_id: &str,
    command: &str,
    payload: &Value,
    observed_at_ms: i64,
) -> Result<(), sqlx::Error> {
    let previous = store.navigation(account_id).await?;
    let state = match command {
        "jaa" => {
            let row = payload.pointer("/gca/A").and_then(Value::as_array);
            let Some(castle_id) = row.and_then(|values| values.get(3)).and_then(Value::as_i64)
            else {
                return Ok(());
            };
            let kingdom_id = payload
                .get("KID")
                .and_then(Value::as_i64)
                .or_else(|| row.and_then(|values| values.get(9)).and_then(Value::as_i64));
            NavigationState {
                account_id: account_id.to_owned(),
                current_kingdom_id: kingdom_id,
                current_castle_id: Some(castle_id),
                map_mode: false,
                recruit_page: false,
                last_castle_switch_at_ms: observed_at_ms,
            }
        }
        "gaa" => {
            let Some(kingdom_id) = payload.get("KID").and_then(Value::as_i64) else {
                return Ok(());
            };
            NavigationState {
                account_id: account_id.to_owned(),
                current_kingdom_id: Some(kingdom_id),
                current_castle_id: None,
                map_mode: true,
                recruit_page: false,
                last_castle_switch_at_ms: previous
                    .as_ref()
                    .map_or(0, |state| state.last_castle_switch_at_ms),
            }
        }
        _ => return Ok(()),
    };
    store.set_navigation(&state, observed_at_ms).await
}

pub(super) async fn record_safe_outbound(store: &Store, _account_id: &str, raw: &str) {
    if let Ok(packet) = parse_xt_packet(raw)
        && packet.command == "lli"
    {
        return;
    }
    record_text(store, Direction::ClientToServer, raw).await;
}

pub(super) fn map_request(raw: &str) -> Option<(i64, i64, i64, i64, i64)> {
    let packet = parse_xt_packet(raw).ok()?;
    if packet.command != "gaa" {
        return None;
    }
    Some((
        packet.payload.get("KID")?.as_i64()?,
        packet.payload.get("AX1")?.as_i64()?,
        packet.payload.get("AY1")?.as_i64()?,
        packet.payload.get("AX2")?.as_i64()?,
        packet.payload.get("AY2")?.as_i64()?,
    ))
}

pub(super) async fn record_text(store: &Store, direction: Direction, raw: &str) {
    let parsed = parse_xt_packet(raw).ok();
    let command = parsed.as_ref().map(|packet| packet.command.clone());
    let payload = parsed.map_or_else(
        || json!({"system_frame": true}),
        |packet| {
            if packet.status.as_deref().is_some_and(|status| status != "0") {
                json!({"status": packet.status, "payload": packet.payload})
            } else {
                compact_diagnostic_payload(&packet.command, packet.payload)
            }
        },
    );
    if let Err(error) = store
        .record_message(now_ms(), direction, command.as_deref(), &payload)
        .await
    {
        warn!(%error, "failed to persist direct-session event");
    }
}

/// Keep the event console useful without serialising and inserting megabytes
/// of map objects on the socket task. Durable target tables already retain the
/// RBC/fortress facts; the diagnostic stream only needs a truthful summary.
pub(super) fn compact_diagnostic_payload(command: &str, payload: Value) -> Value {
    if command != "gaa" {
        return payload;
    }
    let Some(items) = payload.get("AI").and_then(Value::as_array) else {
        return payload;
    };
    if items.len() < 500 {
        return payload;
    }
    let mut rbcs = 0_u64;
    let mut fortresses = 0_u64;
    for item in items {
        match item
            .as_array()
            .and_then(|fields| fields.first())
            .and_then(Value::as_i64)
        {
            Some(2) => rbcs += 1,
            Some(11) => fortresses += 1,
            _ => {}
        }
    }
    json!({
        "KID": payload.get("KID").cloned().unwrap_or(Value::Null),
        "map_summary": {
            "objects": items.len(),
            "rbcs": rbcs,
            "fortresses": fortresses
        }
    })
}
