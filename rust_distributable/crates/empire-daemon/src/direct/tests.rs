use super::*;

fn awaiting_fortress_info() -> Automation {
        let mut automation = Automation::new();
        automation.phase = AutomationPhase::AwaitAdi {
            task: ActiveModeTask {
                mode_id: 1,
                task_id: "fortress".into(),
                name: "Sands fortress".into(),
                profile_id: "attack".into(),
                payload: json!([]),
                kingdom_id: 1,
                level_min: None,
                level_max: None,
                source_x: 593,
                source_y: 613,
                travel_mode: empire_core::planning::TravelMode::Coin,
                algorithm: "closest".into(),
                target_kind: "fortress".into(),
                commander_lids: vec![7],
            },
            target: ReservedTarget {
                kingdom_id: 1,
                x: 516,
                y: 984,
                level: Some(45),
            },
            deadline_ms: now_ms() + REQUEST_TIMEOUT_MS,
        };
        automation
}

fn awaiting_rbc_info() -> Automation {
    let mut automation = Automation::new();
    automation.phase = AutomationPhase::AwaitAdi {
        task: ActiveModeTask {
            mode_id: 1,
            task_id: "rbc".into(),
            name: "Sands towers".into(),
            profile_id: "attack".into(),
            payload: json!([]),
            kingdom_id: 1,
            level_min: Some(35),
            level_max: Some(60),
            source_x: 593,
            source_y: 613,
            travel_mode: empire_core::planning::TravelMode::Coin,
            algorithm: "advanced".into(),
            target_kind: "rbc".into(),
            commander_lids: vec![0, 2, 3],
        },
        target: ReservedTarget {
            kingdom_id: 1,
            x: 636,
            y: 574,
            level: Some(43),
        },
        deadline_ms: now_ms() + REQUEST_TIMEOUT_MS,
    };
    automation
}

#[tokio::test]
async fn fortress_abi_reply_advances_and_unrelated_packets_preserve_the_waiter() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let mut automation = awaiting_fortress_info();
        for raw in [
            "%xt%adi%1%6%null%",
            "%xt%adi%1%0%{}%",
            "%xt%gaa%1%0%{\"KID\":1,\"AI\":[]} %",
            "%xt%cra%1%0%{}%",
        ] {
            automation.observe(&store, "ventrilo", raw).await;
            assert!(
                matches!(automation.phase, AutomationPhase::AwaitAdi { .. }),
                "{raw}"
            );
            assert!(automation.operational_errors.is_empty());
        }
        automation
            .observe(
                &store,
                "ventrilo",
                "%xt%abi%1%0%{\"KID\":1,\"SCID\":16366514,\"gli\":{\"C\":[{\"ID\":7}]}}%",
            )
            .await;
        assert!(matches!(
            automation.phase,
            AutomationPhase::ReadyCra { lord_id: 7, .. }
        ));
}

#[tokio::test]
async fn refused_or_empty_abi_never_commits_an_attack() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        for raw in ["%xt%abi%1%6%null%", "%xt%abi%1%0%null%"] {
            let mut automation = awaiting_fortress_info();
            automation.observe(&store, "ventrilo", raw).await;
            assert!(matches!(automation.phase, AutomationPhase::Idle { .. }));
            assert!(automation.operational_errors.is_empty());
            assert_eq!(automation.last_cra_ms, None);
        }
}

#[tokio::test]
async fn expected_target_rejections_never_trip_the_global_safety_pause() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let mut automation = Automation::new();

    // Live Sands runs regularly receive status 95 when a tower changed after
    // the map scan. The old release treated two of these as operational errors
    // and froze every otherwise-free commander for five minutes.
    for _ in 0..6 {
        automation.phase = awaiting_rbc_info().phase;
        let observed_at = now_ms();
        automation
            .observe(&store, "ventrilo", "%xt%adi%1%95%null%")
            .await;
        // A refused tower is not abandoned: its tile is re-read to learn the
        // real cooldown, after the normal short acknowledgement delay.
        let AutomationPhase::RefreshRbc { due_ms, .. } = automation.phase else {
            panic!("expected rejection to queue a cooldown re-read");
        };
        assert!(due_ms >= observed_at + 750);
        // The wait is low-heavy with a long tail, so only its ceiling is fixed.
        assert!(due_ms <= observed_at + 125_000);
        assert!(automation.operational_errors.is_empty());
        assert_eq!(automation.safety_pause_until_ms, 0);
    }
}

async fn tower_store(cooldown_remaining_s: i64) -> Store {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .upsert_account_profile("ventrilo", "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1)
        .await
        .unwrap();
    store
        .upsert_rbc_targets(
            "ventrilo",
            &[empire_core::account::RbcTarget {
                kingdom_id: 1,
                x: 636,
                y: 574,
                level: Some(43),
                cooldown_remaining_s,
            }],
            now_ms(),
        )
        .await
        .unwrap();
    store
}

fn tower() -> ReservedTarget {
    ReservedTarget {
        kingdom_id: 1,
        x: 636,
        y: 574,
        level: Some(43),
    }
}

#[tokio::test]
async fn a_timeout_on_a_silent_link_does_not_quarantine_the_tower_or_count_an_error() {
    // The Wi-Fi dropped mid-handshake: nothing arrived, so nothing was refused.
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    automation.phase = AutomationPhase::AwaitAdi {
        task: match awaiting_rbc_info().phase {
            AutomationPhase::AwaitAdi { task, .. } => task,
            _ => unreachable!(),
        },
        target: tower(),
        deadline_ms: now_ms() - 1,
    };
    automation.last_inbound_ms = now_ms() - 60_000;
    automation
        .next_packet(&store, "ventrilo", US1_SERVER_HEADER)
        .await
        .unwrap();
    assert!(matches!(automation.phase, AutomationPhase::Idle { .. }));
    assert!(automation.operational_errors.is_empty());
    let ready = store
        .next_rbc_ready_ms("ventrilo", 1, None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(ready <= now_ms(), "the tower went straight back to the pool");

    // The same timeout on a live link still quarantines and counts.
    automation.phase = AutomationPhase::AwaitAdi {
        task: match awaiting_rbc_info().phase {
            AutomationPhase::AwaitAdi { task, .. } => task,
            _ => unreachable!(),
        },
        target: tower(),
        deadline_ms: now_ms() - 1,
    };
    automation.last_inbound_ms = now_ms();
    automation
        .next_packet(&store, "ventrilo", US1_SERVER_HEADER)
        .await
        .unwrap();
    assert_eq!(automation.operational_errors.len(), 1);
}

#[test]
fn only_lost_connections_are_retried() {
    use tokio_tungstenite::tungstenite::{Error as WsError, error::ProtocolError};
    let reset: anyhow::Error =
        WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake).into();
    assert!(connection_was_lost(&reset));
    assert!(connection_was_lost(&ConnectionLost("silent".into()).into()));
    assert!(!connection_was_lost(&anyhow::anyhow!("licence revoked")));
}

#[test]
fn there_are_exactly_two_retries_about_one_and_five_minutes_apart() {
    let mut rng = Rng::seeded(3);
    for _ in 0..1_000 {
        let first = reconnect_delay(1, &mut rng).unwrap().as_secs_f64();
        let second = reconnect_delay(2, &mut rng).unwrap().as_secs_f64();
        assert!((48.0..=72.0).contains(&first), "first retry {first}");
        assert!((240.0..=360.0).contains(&second), "second retry {second}");
    }
    assert!(reconnect_delay(3, &mut rng).is_none(), "then give up");
    assert!(reconnect_delay(0, &mut rng).is_none());
    // Jitter means two losses do not retry on the same beat.
    let delays = (0..20).map(|_| reconnect_delay(1, &mut rng).unwrap().as_millis()).collect::<std::collections::HashSet<_>>();
    assert!(delays.len() > 10);
}

#[test]
fn a_refused_login_is_final_and_says_when_the_lock_ends() {
    let locked = LoginRejected {
        status: "27".to_owned(),
        locked_until_ms: Some(now_ms() + (23 * 3_600 + 30 * 60) * 1_000 + 5_000),
    };
    let text = locked.to_string();
    assert!(text.contains("status 27") && text.contains("23h 30m"), "{text}");
    assert!(text.contains("Not retrying"));
    assert!(!connection_was_lost(&locked.into()), "never retried");
}

#[tokio::test]
async fn an_army_the_castle_cannot_supply_is_never_sent() {
    // 8 Oct: 50 crossbows asked for, 12 at home. The old code sent it anyway,
    // three times, and the server locked the account.
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    let AutomationPhase::AwaitAdi { mut task, target, deadline_ms } = awaiting_rbc_info().phase
    else {
        unreachable!()
    };
    task.payload = json!([{"L": {"T": [[-1, 0]], "U": [[607, 50], [-1, 0]]}}]);
    automation.phase = AutomationPhase::AwaitAdi { task, target, deadline_ms };
    let reply = json!({"gli": {"C": [{"ID": 0}, {"ID": 2}, {"ID": 3}]}, "gui": {"I": [[607, 12]]}});
    automation
        .observe(&store, "ventrilo", &format!("%xt%adi%1%0%{reply}%"))
        .await;
    assert!(
        matches!(automation.phase, AutomationPhase::Idle { .. }),
        "no CRA may be queued: {}",
        automation.status().0
    );
    assert!(automation.detail.contains("unit 607 need 50 have 12"), "{}", automation.detail);
    assert_eq!(automation.last_cra_ms, None);

    // With the troops home the same reply goes ahead.
    let AutomationPhase::AwaitAdi { mut task, target, deadline_ms } = awaiting_rbc_info().phase
    else {
        unreachable!()
    };
    task.payload = json!([{"L": {"T": [[-1, 0]], "U": [[607, 50], [-1, 0]]}}]);
    automation.phase = AutomationPhase::AwaitAdi { task, target, deadline_ms };
    let stocked = json!({"gli": {"C": [{"ID": 0}]}, "gui": {"I": [[607, 50]]}});
    automation
        .observe(&store, "ventrilo", &format!("%xt%adi%1%0%{stocked}%"))
        .await;
    assert!(matches!(automation.phase, AutomationPhase::ReadyCra { lord_id: 0, .. }));
}

#[tokio::test]
async fn every_attack_details_reply_leaves_a_durable_stock_record() {
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    let AutomationPhase::AwaitAdi { mut task, target, deadline_ms } = awaiting_rbc_info().phase
    else {
        unreachable!()
    };
    task.payload = json!([{"L": {"T": [[-1, 0]], "U": [[607, 50], [-1, 0]]}}]);
    automation.phase = AutomationPhase::AwaitAdi { task, target, deadline_ms };
    let reply = json!({"SCID": 16366514, "gli": {"C": [{"ID": 0}]},
        "gui": {"I": [[607, 21459], [5, 25000]], "TU": [[607, 853]]}});
    automation
        .observe(&store, "Ventrilo", &format!("%xt%adi%1%0%{reply}%"))
        .await;
    // Only the unit the attack uses is kept, under the canonical account id.
    let samples = store.recent_stock_samples("ventrilo", 10).await.unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(
        (samples[0].castle_id, samples[0].unit_id, samples[0].home, samples[0].out),
        (16366514, 607, 21459, 853)
    );
    assert!(automation.status().1.contains("unit 607: 21,459 home, 853 out"), "{}", automation.status().1);
}

#[tokio::test]
async fn an_unexplained_cra_refusal_stops_the_mode_instead_of_trying_the_next_tower() {
    let (store, tasks) = two_section_store().await;
    assert!(store.account_mode_running("ventrilo").await.unwrap());
    let mut automation = Automation::new();
    for status in ["101", "313", "5"] {
        store
            .subscribe_account_mode("ventrilo", tasks[0].mode_id, true, now_ms())
            .await
            .unwrap();
        automation.phase = AutomationPhase::AwaitCra {
            task: tasks[0].clone(),
            target: tower(),
            lord_id: tasks[0].commander_lids[0],
            deadline_ms: now_ms() + REQUEST_TIMEOUT_MS,
        };
        automation
            .observe(&store, "ventrilo", &format!("%xt%cra%1%{status}%null%"))
            .await;
        assert!(!store.account_mode_running("ventrilo").await.unwrap(), "status {status}");
        assert!(automation.detail.starts_with("STOPPED"), "{}", automation.detail);
        let events = store.recent_events("ventrilo", 5).await.unwrap();
        assert_eq!(events[0].kind, "attack.stopped", "status {status}");
        // The reason survives the mode being off.
        automation.phase = AutomationPhase::Idle { due_ms: 0 };
        automation
            .next_packet(&store, "ventrilo", US1_SERVER_HEADER)
            .await
            .unwrap();
        assert!(automation.detail.starts_with("STOPPED"), "{}", automation.detail);
    }
    // Refusals about the tower or the commander are still routine.
    for status in ["95", "93", "256"] {
        store
            .subscribe_account_mode("ventrilo", tasks[0].mode_id, true, now_ms())
            .await
            .unwrap();
        automation.phase = AutomationPhase::AwaitCra {
            task: tasks[0].clone(),
            target: tower(),
            lord_id: tasks[0].commander_lids[0],
            deadline_ms: now_ms() + REQUEST_TIMEOUT_MS,
        };
        automation
            .observe(&store, "ventrilo", &format!("%xt%cra%1%{status}%null%"))
            .await;
        assert!(store.account_mode_running("ventrilo").await.unwrap(), "status {status}");
    }
}

#[tokio::test]
async fn a_refused_tower_is_parked_for_a_minute_not_an_hour() {
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    automation.phase = awaiting_rbc_info().phase;
    let before = now_ms();
    automation
        .observe(&store, "ventrilo", "%xt%adi%1%95%null%")
        .await;
    let ready = store
        .next_rbc_ready_ms("ventrilo", 1, Some(35), Some(60))
        .await
        .unwrap()
        .unwrap();
    assert!(ready >= before + 59_000 && ready <= now_ms() + 61_000, "{ready}");
}

#[tokio::test]
async fn the_cooldown_reread_sends_the_towers_tile_and_learns_the_server_cooldown() {
    // The tile response is what carries the cooldown; observe_account_packet
    // stores it before the automation sees the packet, so seed it the same way.
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    automation.phase = AutomationPhase::RefreshRbc {
        target: tower(),
        due_ms: 0,
    };
    let packet = automation
        .next_packet(&store, "ventrilo", US1_SERVER_HEADER)
        .await
        .unwrap()
        .expect("the tile read goes out");
    assert!(packet.contains("%gaa%") && packet.contains("\"AX1\":630"), "{packet}");
    assert!(matches!(
        automation.phase,
        AutomationPhase::AwaitRbcRefresh { .. }
    ));

    let map = json!({"KID": 1, "AI": [[2, 636, 574, -1, 42, 5400, 1]]});
    let raw = format!("%xt%gaa%1%0%{map}%");
    observe_account_packet(
        &store,
        "ventrilo",
        &raw,
        &HashMap::new(),
        &SessionSettings::default(),
        false,
    )
    .await;
    automation.observe(&store, "ventrilo", &raw).await;
    assert!(matches!(automation.phase, AutomationPhase::Idle { .. }));
    assert!(automation.detail.contains("on cooldown"), "{}", automation.detail);
    let free_at = store.rbc_server_free_at("ventrilo", &tower()).await.unwrap().unwrap();
    assert!(free_at > now_ms() + 5_000_000, "{free_at}");
    assert!(automation.operational_errors.is_empty());
}

#[tokio::test]
async fn a_refusal_the_map_cannot_explain_parks_the_tower_for_half_an_hour() {
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    automation.phase = AutomationPhase::AwaitRbcRefresh {
        target: tower(),
        deadline_ms: now_ms() + REQUEST_TIMEOUT_MS,
    };
    automation
        .observe(&store, "ventrilo", "%xt%gaa%1%0%{\"KID\":1,\"AI\":[]}%")
        .await;
    assert!(matches!(automation.phase, AutomationPhase::Idle { .. }));
    let ready = store
        .next_rbc_ready_ms("ventrilo", 1, None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(ready >= now_ms() + UNEXPLAINED_REFUSAL_HOLD_MS - 1_000, "{ready}");
}

#[tokio::test]
async fn a_missing_tile_read_times_out_without_counting_as_an_operational_error() {
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    automation.phase = AutomationPhase::AwaitRbcRefresh {
        target: tower(),
        deadline_ms: now_ms() - 1,
    };
    automation
        .next_packet(&store, "ventrilo", US1_SERVER_HEADER)
        .await
        .unwrap();
    assert!(matches!(automation.phase, AutomationPhase::Idle { .. }));
    assert!(automation.operational_errors.is_empty());
}

/// Two tasks, 17 + 18 commanders, one tower ready for the first task only.
async fn two_section_store() -> (Store, Vec<ActiveModeTask>) {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .upsert_account_profile("ventrilo", "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1)
        .await
        .unwrap();
    store
        .replace_account_bootstrap(
            "ventrilo",
            &[empire_core::account::OwnedCastle {
                kingdom_id: 1,
                castle_id: 100,
                area_type: 12,
                x: 593,
                y: 613,
                name: "Sands".to_owned(),
            }],
            hunt::USABLE_COMMANDER_LIDS,
            now_ms(),
        )
        .await
        .unwrap();
    let mode_id = store
        .import_mode_bundle(&empire_core::planning::ventrilo_sands_bundle(), now_ms())
        .await
        .unwrap();
    store
        .subscribe_account_mode("ventrilo", mode_id, true, now_ms())
        .await
        .unwrap();
    store
        .upsert_rbc_targets(
            "ventrilo",
            &[empire_core::account::RbcTarget {
                kingdom_id: 1,
                x: 600,
                y: 610,
                level: Some(61),
                cooldown_remaining_s: 0,
            }],
            now_ms(),
        )
        .await
        .unwrap();
    let tasks = store.active_mode_tasks("ventrilo").await.unwrap();
    assert_eq!(tasks.len(), 2);
    (store, tasks)
}

async fn send_everyone_out(store: &Store, lids: &[i64]) {
    for lord_id in lids {
        store
            .set_commander_state(
                &CommanderState {
                    account_id: "ventrilo".to_owned(),
                    lord_id: *lord_id,
                    status: COMMANDER_OUTBOUND.to_owned(),
                    available_after_ms: now_ms() + 600_000,
                    march_id: None,
                    target_key: None,
                },
                now_ms(),
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn sections_map_to_the_scheduler_commander_table_exactly() {
    // The visible commander numbers are 1-based positions in this table; the
    // server LIDs skip 1, 4, 5, 12-15 and 19, so the two never line up.
    let (_, tasks) = two_section_store().await;
    let human = |lid: i64| {
        hunt::USABLE_COMMANDER_LIDS
            .iter()
            .position(|usable| *usable == lid)
            .map(|index| index + 1)
            .unwrap()
    };
    let crossbow = tasks[0].commander_lids.iter().map(|lid| human(*lid)).collect::<Vec<_>>();
    assert_eq!(crossbow, (1..=17).collect::<Vec<_>>());
    assert_eq!(tasks[0].commander_lids[16], 24, "human 17 is LID 24");
    // Human 29 is LID 36: it belongs to the second section, never the first.
    assert_eq!(hunt::USABLE_COMMANDER_LIDS[28], 36);
    assert!(tasks[1].commander_lids.contains(&36));
    assert!(!tasks[0].commander_lids.contains(&36));
}

#[tokio::test]
async fn an_idle_section_is_never_lent_to_a_busy_one() {
    // Every crossbow commander is out and a level-61 tower is ready; the other
    // section's commanders are all home. The tower must wait: sending it under
    // a commander reserved for the other section is exactly what was wrong.
    let (store, tasks) = two_section_store().await;
    send_everyone_out(&store, &tasks[0].commander_lids).await;
    let mut automation = Automation::new();
    let packet = automation
        .next_packet(&store, "ventrilo", US1_SERVER_HEADER)
        .await
        .unwrap();
    assert!(packet.is_none(), "nothing may be sent");
    assert!(
        matches!(automation.phase, AutomationPhase::Idle { .. }),
        "{}",
        automation.status().0
    );
    assert!(automation.detail.contains("tower ready, next commander in"), "{}", automation.detail);
}

#[tokio::test]
async fn a_task_only_ever_selects_its_own_commanders() {
    let (store, tasks) = two_section_store().await;
    let mut automation = Automation::new();
    automation
        .next_packet(&store, "ventrilo", US1_SERVER_HEADER)
        .await
        .unwrap();
    let AutomationPhase::AwaitMap { task, .. } = &automation.phase else {
        panic!("expected the sand tower to be taken: {}", automation.detail);
    };
    assert_eq!(task.task_id, tasks[0].task_id);
    assert_eq!(task.commander_lids, tasks[0].commander_lids);
}

#[tokio::test]
async fn when_nothing_can_run_the_scheduler_sleeps_until_the_first_blocker_clears() {
    let (sections, tasks) = two_section_store().await;
    // Make the sand tower cool for twenty minutes and send everything out for
    // ten: the commanders are the first thing to come back.
    sections
        .refresh_rbc_cooldowns(
            "ventrilo",
            &[empire_core::account::RbcTarget {
                kingdom_id: 1,
                x: 600,
                y: 610,
                level: Some(61),
                cooldown_remaining_s: 1_200,
            }],
            now_ms(),
        )
        .await
        .unwrap();
    let mut everyone = tasks[0].commander_lids.clone();
    everyone.extend(&tasks[1].commander_lids);
    send_everyone_out(&sections, &everyone).await;
    let mut automation = Automation::new();
    let before = now_ms();
    let packet = automation
        .next_packet(&sections, "ventrilo", US1_SERVER_HEADER)
        .await
        .unwrap();
    assert!(packet.is_none());
    let AutomationPhase::Idle { due_ms } = automation.phase else {
        panic!("expected to wait");
    };
    // The blocker clears in minutes, so the wake-up is capped at the ceiling
    // rather than the old fixed fifteen-second poll *or* a minutes-long sleep
    // that would ignore a stop request.
    assert!(due_ms >= before + 1_500 && due_ms <= before + 32_000, "{}", due_ms - before);
    assert!(automation.detail.contains("next tower in"), "{}", automation.detail);
}

    /// One request has to answer eighteen slots, so the window is the smallest
    /// rectangle spanning three slots on each residue family.
    #[test]
    fn a_fortress_probe_asks_for_a_whole_block() {
        let packet =
            fortress_gaa_packet("EmpireEx_21", 1, FortressBlock { x: 633, y: 633 }).unwrap();
        assert!(packet.contains("\"AX1\":632"), "{packet}");
        assert!(packet.contains("\"AX2\":732"), "{packet}");
        assert!(packet.contains("\"AY1\":632"), "{packet}");
        assert!(packet.contains("\"AY2\":732"), "{packet}");
    }

    #[test]
    fn the_discovery_walk_measures_a_rectangle_without_a_map_size() {
        // The shape the live arms produced: a 7x8 block band, no map size given
        // to the walk anywhere. It has to find the extent on its own.
        let origins_x = [243_i64, 360, 477, 594, 711, 828, 945];
        let origins_y = [126_i64, 243, 360, 477, 594, 711, 828, 945];
        let band = origins_x
            .iter()
            .flat_map(|x| origins_y.iter().map(move |y| (*x, *y)))
            .collect::<Vec<_>>();

        let base = block_origin((593, 613));
        let mut walk = FortressDiscovery::new(7, base);
        let mut probes = 0_u32;
        while let Some(block) = walk.next() {
            probes += 1;
            assert!(probes < 200, "the walk never terminated");
            let holds = band
                .iter()
                .any(|(x, y)| block_origin((*x, *y)) == (block.x, block.y));
            walk.observe(holds);
        }

        let bounds = walk.bounds().expect("a band with fortresses in it");
        assert_eq!(
            (bounds.left, bounds.top, bounds.right, bounds.bottom),
            (243, 126, 1_043, 1_043)
        );
        // Every fortress the band holds is inside the measured rectangle, and
        // measuring plus filling beats the flat 121-cell sweep.
        let missed = band
            .iter()
            .filter(|(x, y)| {
                *x < bounds.left || *x > bounds.right || *y < bounds.top || *y > bounds.bottom
            })
            .count();
        assert_eq!(missed, 0);
        let fill = empire_core::fortress::align_bounds(bounds, SWEEP_ALIGNMENT).block_count();
        assert!((probes as i64) + fill < 121, "{} probes", probes);
    }

    /// A refused window arrives as an empty payload, so the walk cannot tell it
    /// apart from ground with nothing on it. Treating it as empty ends the arm
    /// early: that under-covers, which shows up as a missed fortress. Assuming
    /// it held something would extend the rectangle over ground never seen,
    /// which is worse. This pins the conservative direction.
    #[test]
    fn a_refused_window_ends_an_arm_rather_than_extending_it() {
        let base = (477_i64, 594_i64);
        let mut walk = FortressDiscovery::new(7, base);
        // The base answers, then every arm window is refused.
        let mut probes = 0_u32;
        while let Some(_block) = walk.next() {
            probes += 1;
            assert!(probes < 20, "refusals must end the arms, not grow them");
            walk.observe(false);
        }
        // One base plus two refusals per arm, and nothing was ever reached.
        assert_eq!(probes, 1 + 2 * ARM_STEPS.len() as u32);
        assert!(walk.bounds().is_none(), "a refused walk measures nothing");
    }

    #[test]
    fn fortress_probe_pacing_matches_the_scan_budget() {
        let mut rng = Rng::seeded(42);
        let samples = (0..1_000)
            .map(|_| fortress_probe_delay_seconds(&mut rng))
            .collect::<Vec<_>>();
        assert!(samples.iter().all(|delay| (0.70..=1.70).contains(delay)));
        // The delay alone is about 145 s for a full 121-cell fallback sweep; a measured
        // rectangle is smaller, and the server's own time is added on top and is
        // not measurable from here.
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let seconds = 121.0 * mean;
        assert!(
            (130.0..=160.0).contains(&seconds),
            "{seconds} s per kingdom"
        );
    }

#[tokio::test]
async fn the_second_error_inside_five_minutes_trips_the_safety_pause() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let mut automation = Automation::new();
    automation
        .record_operational_error(&store, "ventrilo", 1_000_000, "first")
        .await;
    assert_eq!(automation.safety_pause_until_ms, 0);
    automation
        .record_operational_error(&store, "ventrilo", 1_299_999, "second")
        .await;
    assert_eq!(automation.safety_pause_until_ms, 1_300_000);
    assert!(automation.detail.contains("Safety pause"));
}

#[tokio::test]
async fn old_errors_age_out_of_the_rolling_window() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let mut automation = Automation::new();
    automation
        .record_operational_error(&store, "ventrilo", 1_000_000, "old")
        .await;
    automation
        .record_operational_error(&store, "ventrilo", 1_300_000, "new")
        .await;
    assert_eq!(automation.operational_errors.len(), 1);
    assert_eq!(automation.safety_pause_until_ms, 0);
}

    #[test]
    fn large_map_diagnostics_are_compacted_without_losing_counts() {
        let mut objects = (0..500).map(|x| json!([1, x, 1, -1])).collect::<Vec<_>>();
        objects.push(json!([2, 600, 610, -1, 50]));
        objects.push(json!([11, 594, 594, -1, 45, 0, 7, 1]));
        let compacted = compact_diagnostic_payload("gaa", json!({"KID": 1, "AI": objects}));
        assert_eq!(compacted.pointer("/map_summary/objects"), Some(&json!(502)));
        assert_eq!(compacted.pointer("/map_summary/rbcs"), Some(&json!(1)));
        assert_eq!(
            compacted.pointer("/map_summary/fortresses"),
            Some(&json!(1))
        );
        assert!(compacted.get("AI").is_none());
    }

    #[test]
    fn the_outward_scan_is_wide_rather_than_uniform() {
        let mut rng = Rng::seeded(11);
        let samples = (0..2_000)
            .map(|_| base_scan_delay_seconds(&mut rng))
            .collect::<Vec<_>>();
        assert!(samples.iter().all(|delay| (0.7..=1.7).contains(delay)));
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        assert!((1.15..=1.25).contains(&mean), "mean {mean}");
        let low = samples.iter().copied().fold(f64::MAX, f64::min);
        let high = samples.iter().copied().fold(f64::MIN, f64::max);
        assert!(high - low > 0.95, "spread {}", high - low);
    }

    #[test]
    fn a_kingdom_switch_pauses_longer_than_a_probe() {
        let mut rng = Rng::seeded(7);
        let switches = (0..1_000)
            .map(|_| fortress_kingdom_switch_delay_seconds(&mut rng))
            .collect::<Vec<_>>();
        assert!(switches.iter().all(|delay| (3.4..=4.8).contains(delay)));
        assert!(switches[0] > fortress_probe_delay_seconds(&mut Rng::seeded(7)));
    }

    #[test]
    fn world_profiles_keep_socket_and_portal_identity_together() {
        assert_eq!(
            GameServer::Us1.connection(),
            (US1_ENDPOINT, US1_SERVER_HEADER, VENTRILO_PORTAL_ACCOUNT_ID)
        );
        assert_eq!(
            GameServer::World2.connection(),
            (
                WORLD2_ENDPOINT,
                WORLD2_SERVER_HEADER,
                PINGPOKO_PORTAL_ACCOUNT_ID
            )
        );
        assert_ne!(
            GameServer::Us1.connection().2,
            GameServer::World2.connection().2
        );
        assert_ne!(
            GameServer::Us1.connection().1,
            GameServer::World2.connection().1
        );
        assert_eq!(
            trusted_server_name(US1_ENDPOINT, US1_SERVER_HEADER),
            Some("US1")
        );
        assert_eq!(
            trusted_server_name(WORLD2_ENDPOINT, WORLD2_SERVER_HEADER),
            Some("WORLD2")
        );
        assert_eq!(
            trusted_server_name("wss://attacker.invalid/", US1_SERVER_HEADER),
            None
        );
    }

    #[tokio::test]
    async fn jaa_and_gaa_are_authoritative_castle_and_map_context() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store
            .upsert_account_profile("ventrilo", "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1)
            .await
            .unwrap();
        persist_navigation_packet(
            &store,
            "ventrilo",
            "jaa",
            &json!({
                "KID": 0,
                "gca": {"A": [1, 509, 405, 16011862, 16862926, 7, 7, 7, 3, 0]}
            }),
            10,
        )
        .await
        .unwrap();
        let castle = store.navigation("ventrilo").await.unwrap().unwrap();
        assert!(!castle.map_mode);
        assert_eq!(castle.current_kingdom_id, Some(0));
        assert_eq!(castle.current_castle_id, Some(16011862));

        persist_navigation_packet(&store, "ventrilo", "gaa", &json!({"KID": 1, "AI": []}), 20)
            .await
            .unwrap();
        let map = store.navigation("ventrilo").await.unwrap().unwrap();
        assert!(map.map_mode);
        assert_eq!(map.current_kingdom_id, Some(1));
        assert_eq!(map.current_castle_id, None);
        assert_eq!(map.last_castle_switch_at_ms, 10);
    }

    #[tokio::test]
    async fn cooldown_reads_keep_fortresses_but_do_not_expand_the_rbc_catalogue() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        store
            .upsert_account_profile("ventrilo", "Ventrilo", US1_ENDPOINT, US1_SERVER_HEADER, 1)
            .await
            .unwrap();
        let settings = SessionSettings {
            kingdom_scans: vec![KingdomScan {
                kingdom_id: 1,
                enabled: true,
                radius: 50,
            }],
            ..SessionSettings::default()
        };
        let bootstrap = json!({
            "gcl": {"C": [{"KID": 1, "AI": [{"AI": [12, 593, 613, 16366514, 1, 5, 5, 3, 4, 1, "Castle Ventrilo"]}]}]},
            "gli": {"C": [{"ID": 1}]}
        });
        observe_account_packet(
            &store,
            "ventrilo",
            &format!("%xt%gbd%1%0%{bootstrap}%"),
            &HashMap::new(),
            &settings,
            true,
        )
        .await;
        let origins = permanent_scan_origins("ventrilo", &store.owned_castles().await.unwrap());
        let map = json!({"KID": 1, "AI": [
            [2, 600, 610, -1, 50],
            [11, 594, 594, -1, 45, 38421, 17185267, 1]
        ]});
        let raw = format!("%xt%gaa%1%0%{map}%");

        observe_account_packet(&store, "ventrilo", &raw, &origins, &settings, false).await;
        assert!(!store.account_has_targets("ventrilo", 1).await.unwrap());
        assert!(store.account_has_fortresses("ventrilo", 1).await.unwrap());

        observe_account_packet(&store, "ventrilo", &raw, &origins, &settings, true).await;
        assert!(store.account_has_targets("ventrilo", 1).await.unwrap());
    }

// ---------------------------------------------------------------------------
// stochastic scheduling, spotlight selection and response health
// ---------------------------------------------------------------------------

fn rbc_task(task_id: &str) -> ActiveModeTask {
    match awaiting_rbc_info().phase {
        AutomationPhase::AwaitAdi { mut task, .. } => {
            task.task_id = task_id.into();
            task
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn the_spotlight_moves_only_when_an_attack_is_acknowledged() {
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    let task = rbc_task("rbc");
    let mut selector = Selector::new(1);
    selector
        .state
        .initialize_spotlight(1, (593, 613), &selector.params.clone(), &mut Rng::seeded(1));
    let before = selector.state.clone();
    automation.selectors.insert("rbc".into(), selector);
    automation.pending_pick = Some(PendingPick { task_id: "rbc".into(), tower: (636, 574), relocate: false });

    // A refused attack leaves the geography exactly as it was.
    automation.phase = AutomationPhase::AwaitCra {
        task: task.clone(),
        target: tower(),
        lord_id: 0,
        deadline_ms: now_ms() + REQUEST_TIMEOUT_MS,
    };
    automation.observe(&store, "ventrilo", "%xt%cra%1%95%null%").await;
    assert_eq!(automation.selectors["rbc"].state, before);
    assert_eq!(automation.selected, None);

    // An acknowledged one moves it, with momentum.
    automation.pending_pick = Some(PendingPick { task_id: "rbc".into(), tower: (636, 574), relocate: false });
    automation.phase = AutomationPhase::AwaitCra {
        task,
        target: tower(),
        lord_id: 0,
        deadline_ms: now_ms() + REQUEST_TIMEOUT_MS,
    };
    automation
        .observe(&store, "ventrilo", "%xt%cra%1%0%{\"AAM\":{\"M\":{\"MID\":42,\"TT\":300}}}%")
        .await;
    let after = &automation.selectors["rbc"].state;
    assert_ne!(after.spotlight, before.spotlight);
    assert_eq!(automation.selected, Some((636, 574)));
    assert_eq!(automation.last_completed.as_ref().map(|done| done.march_id), Some(42));
    assert!(automation.pending_pick.is_none());
}

#[tokio::test]
async fn the_stochastic_wait_never_beats_the_global_cra_spacing() {
    let store = tower_store(0).await;
    for _ in 0..40 {
        let mut automation = Automation::new();
        let AutomationPhase::AwaitAdi { mut task, target, deadline_ms } = awaiting_rbc_info().phase
        else {
            unreachable!()
        };
        task.payload = json!([]);
        automation.phase = AutomationPhase::AwaitAdi { task, target, deadline_ms };
        // The last attack and the reference instant are the same moment.
        let before = now_ms();
        automation.last_cra_ms = Some(before);
        automation
            .observe(&store, "ventrilo", "%xt%adi%1%0%{\"gli\":{\"C\":[{\"ID\":0}]}}%")
            .await;
        let AutomationPhase::ReadyCra { due_ms, .. } = automation.phase else {
            panic!("expected a queued attack: {}", automation.detail);
        };
        assert!(due_ms >= before + 4_000, "due in {} ms", due_ms - before);
        let last = automation.scheduler.last().expect("the wait is recorded");
        assert!(last.waited_s >= 2.0, "its own minimum applies");
        // The limit is named whenever it decided; if the stochastic wait was
        // already longer than the spacing, no limit was in play.
        assert!(
            last.restriction == Some("global cra spacing") || last.waited_s >= 4.0,
            "{last:?}"
        );
        assert!(last.applied_s >= last.waited_s);
    }
}

#[tokio::test]
async fn a_long_idle_gap_does_not_queue_a_burst() {
    // Waking after a long sleep produces one fresh wait from "now", not a run of
    // overdue actions: the machine is single-flight, so only one handshake exists.
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    automation.last_cra_ms = Some(now_ms() - 3_600_000);
    let AutomationPhase::AwaitAdi { mut task, target, deadline_ms } = awaiting_rbc_info().phase
    else {
        unreachable!()
    };
    task.payload = json!([]);
    automation.phase = AutomationPhase::AwaitAdi { task, target, deadline_ms };
    let before = now_ms();
    automation
        .observe(&store, "ventrilo", "%xt%adi%1%0%{\"gli\":{\"C\":[{\"ID\":0}]}}%")
        .await;
    let AutomationPhase::ReadyCra { due_ms, .. } = automation.phase else {
        panic!("expected a queued attack");
    };
    assert!(due_ms >= before + 2_000 && due_ms <= before + 8_000, "{}", due_ms - before);
    assert_eq!(automation.scheduler.last().unwrap().restriction, None, "no limit was in play");
}

#[tokio::test]
async fn the_third_unexpected_timeout_in_an_hour_stops_new_actions_until_the_mode_is_restarted() {
    let (store, tasks) = two_section_store().await;
    let mut automation = Automation::new();
    for round in 1..=3 {
        automation.last_inbound_ms = now_ms(); // the link is alive: this is not a connection loss
        automation.phase = AutomationPhase::AwaitAdi {
            task: tasks[0].clone(),
            target: tower(),
            deadline_ms: now_ms() - 1,
        };
        automation.safety_pause_until_ms = 0;
        automation.operational_errors.clear();
        automation.next_packet(&store, "ventrilo", US1_SERVER_HEADER).await.unwrap();
        assert_eq!(automation.health.count(now_ms()), round);
    }
    assert!(automation.health.paused());
    assert_eq!(automation.status().0, "paused_on_errors");
    assert!(automation.detail.starts_with("PAUSED"), "{}", automation.detail);
    let kinds = store.recent_events("ventrilo", 10).await.unwrap();
    assert!(kinds.iter().any(|event| event.kind == "health.paused"));

    // Nothing new starts, however long it waits.
    automation.phase = AutomationPhase::Idle { due_ms: 0 };
    automation.safety_pause_until_ms = 0;
    for _ in 0..3 {
        assert!(automation.next_packet(&store, "ventrilo", US1_SERVER_HEADER).await.unwrap().is_none());
        assert!(matches!(automation.phase, AutomationPhase::Idle { .. }));
        automation.phase = AutomationPhase::Idle { due_ms: 0 };
    }
    assert!(automation.health.paused(), "it does not resume by itself");

    // Stopping the mode is the operator action that clears it.
    store.stop_account_mode("ventrilo", now_ms()).await.unwrap();
    automation.next_packet(&store, "ventrilo", US1_SERVER_HEADER).await.unwrap();
    assert!(!automation.health.paused());
}

#[tokio::test]
async fn timeouts_on_a_silent_link_and_ordinary_refusals_do_not_count_as_null_responses() {
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    // Link down: connection loss, recorded but not counted.
    automation.last_inbound_ms = now_ms() - 60_000;
    automation.phase = AutomationPhase::AwaitAdi {
        task: rbc_task("rbc"),
        target: tower(),
        deadline_ms: now_ms() - 1,
    };
    automation.next_packet(&store, "ventrilo", US1_SERVER_HEADER).await.unwrap();
    // An explicit refusal is an answer.
    automation.phase = awaiting_rbc_info().phase;
    automation.observe(&store, "ventrilo", "%xt%adi%1%95%null%").await;
    // A valid empty tile read.
    automation.phase = AutomationPhase::AwaitRbcRefresh { target: tower(), deadline_ms: now_ms() + REQUEST_TIMEOUT_MS };
    automation.observe(&store, "ventrilo", "%xt%gaa%1%0%{\"KID\":1,\"AI\":[]}%").await;
    assert_eq!(automation.health.count(now_ms()), 0);
    assert!(!automation.health.paused());
    let outcomes = automation.health.recent().map(|incident| incident.outcome).collect::<Vec<_>>();
    assert!(outcomes.contains(&Outcome::ConnectionLoss) && outcomes.contains(&Outcome::ExpectedEmpty));
    // A status-0 reply with no payload is a qualifying null.
    automation.phase = awaiting_rbc_info().phase;
    automation.observe(&store, "ventrilo", "%xt%adi%1%0%null%").await;
    assert_eq!(automation.health.count(now_ms()), 1);
}

#[tokio::test]
async fn the_null_tolerance_can_be_configured() {
    let store = tower_store(0).await;
    store
        .set_app_state("automation.null_tolerance", &json!(5), now_ms())
        .await
        .unwrap();
    let mut automation = Automation::new();
    automation.configure(&store).await;
    assert_eq!(automation.health.tolerance(), 5);
}

#[tokio::test]
async fn taking_a_snapshot_for_the_development_tab_changes_nothing_in_the_bot() {
    let store = tower_store(0).await;
    let mut automation = Automation::new();
    automation.phase = AutomationPhase::AwaitRbcRefresh { target: tower(), deadline_ms: now_ms() + REQUEST_TIMEOUT_MS };
    automation.last_cra_ms = Some(123);
    let detail = automation.detail.clone();
    let first = automation.dev_snapshot(now_ms());
    let second = automation.dev_snapshot(now_ms());
    assert!(matches!(automation.phase, AutomationPhase::AwaitRbcRefresh { .. }));
    assert_eq!((automation.last_cra_ms, &automation.detail), (Some(123), &detail));
    // The model keeps moving with real time and the snapshot carries what the tab
    // needs to draw it, without any credentials or packets.
    assert_eq!(first.timing.waves.len(), 10);
    assert!(first.timing.waves.iter().all(|wave| wave.omega > 0.0 && wave.phase.is_finite()));
    assert!((0.5..1.0).contains(&first.timing.floor_s));
    assert_eq!(first.timing.projection.len(), 30);
    assert_eq!(first.timing.projection[0].index, second.timing.projection[0].index, "taking a snapshot uses up no values");
    assert_eq!(second.scheduler.state, "refreshing_tower");
    assert_eq!(first.health.tolerance, 2);
    let text = serde_json::to_string(&first).unwrap();
    for forbidden in ["password", "token", "lli"] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    drop(store);
}
