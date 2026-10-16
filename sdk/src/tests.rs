use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
struct Fixture {
    root: PathBuf,
    p: Profile,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("universal-ap-test-{}", nonce()));
        let p = Profile {
            bridge_dir: root.join("bridge"),
            journal_path: root.join("journal.json"),
            endpoint: "ws://127.0.0.1:1".into(),
            slot_name: "Test".into(),
            game: "Synthetic".into(),
            save: "disposable".into(),
            contract: "fixture-v1".into(),
            items: BTreeMap::from([(
                1,
                ItemMapping {
                    kind: EffectKind::Repeatable,
                    payload: serde_json::json!({"gold":5}),
                },
            )]),
            locations: BTreeSet::from([10]),
            ack_timeout_ms: 1000,
            stale_after_ms: 500,
        };
        Self { root, p }
    }
    fn identity(&self) -> SessionIdentity {
        SessionIdentity {
            game: self.p.game.clone(),
            seed: "seed".into(),
            team: 0,
            slot: 1,
            save: self.p.save.clone(),
        }
    }
    fn setup(&self) -> (FileAdapter, GameEndpoint) {
        let mut a = FileAdapter::open(self.p.clone()).unwrap();
        a.bind(self.identity()).unwrap();
        let mut g = GameEndpoint::open(self.p.clone(), serde_json::json!(0)).unwrap();
        g.tick(true, grant).unwrap();
        assert!(a.current().is_err());
        g.tick(true, grant).unwrap();
        assert!(a.current().is_ok());
        (a, g)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let (Ok(root), Ok(temp)) = (
            fs::canonicalize(&self.root),
            fs::canonicalize(std::env::temp_dir()),
        ) {
            assert_eq!(root.parent(), Some(temp.as_path()));
            assert!(
                root.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("universal-ap-test-")
            );
            let _ = fs::remove_dir_all(root);
        }
    }
}
fn grant(v: &Value, _: &Effect<Value>) -> Result<Value> {
    Ok(serde_json::json!(v.as_u64().unwrap() + 1))
}
fn effect() -> Effect<Value> {
    Effect {
        id: EffectId::received_item(0),
        kind: EffectKind::Repeatable,
        payload: serde_json::json!({"gold":5}),
    }
}
#[test]
fn transient_publication_errors_are_narrowly_classified() {
    for code in [5, 32, 33, 1450] {
        let error = format!("atomic replacement failed: localized text (os error {code})");
        assert_eq!(is_transient_replace(&error), cfg!(windows));
    }
    for error in [
        "missing directory (os error 3)",
        "disk full (os error 112)",
        "invalid parameter (os error 87)",
        "other error (os error 14500)",
        "unspecified resource failure",
    ] {
        assert!(!is_transient_replace(error), "unexpected retry: {error}");
    }
}
fn game_thread(mut game: GameEndpoint) -> (Arc<AtomicBool>, std::thread::JoinHandle<GameEndpoint>) {
    let stop = Arc::new(AtomicBool::new(false));
    let other = stop.clone();
    let t = std::thread::spawn(move || {
        while !other.load(Ordering::SeqCst) {
            game.tick(true, grant).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        game
    });
    (stop, t)
}
#[test]
fn exclusive_role_locks() {
    let f = Fixture::new();
    let (_a, _g) = f.setup();
    assert!(FileAdapter::open(f.p.clone()).is_err());
    assert!(GameEndpoint::open(f.p.clone(), Value::Null).is_err());
}
#[test]
fn stale_snapshot_and_restarted_boot_require_progress() {
    let f = Fixture::new();
    let (mut a, g) = f.setup();
    std::thread::sleep(Duration::from_millis(550));
    assert!(a.current().is_err());
    drop(g);
    let mut g = GameEndpoint::open(f.p.clone(), Value::Null).unwrap();
    g.tick(true, grant).unwrap();
    assert!(a.current().is_err());
    g.tick(true, grant).unwrap();
    assert!(a.current().is_ok());
}
#[test]
fn reconnect_challenge_invalidates_old_snapshot() {
    let f = Fixture::new();
    let (mut a, mut g) = f.setup();
    a.bind(f.identity()).unwrap();
    assert!(a.current().is_err());
    g.tick(true, grant).unwrap();
    assert!(a.current().is_err());
    g.tick(true, grant).unwrap();
    assert!(a.current().is_ok());
}
#[test]
fn malformed_version_and_oversize_fail_closed() {
    let f = Fixture::new();
    let (mut a, _g) = f.setup();
    let path = f.p.bridge_dir.join("snapshot.json");
    let mut s: Snapshot = read_json(&path).unwrap();
    s.version = 999;
    write_json(&path, &s).unwrap();
    assert!(a.current().unwrap_err().contains("version"));
    fs::write(&path, b"not json").unwrap();
    assert!(a.current().is_err());
    fs::write(&path, vec![b' '; MAX_FRAME + 1]).unwrap();
    assert!(a.current().unwrap_err().contains("1 MiB"));
}
#[test]
fn wrong_identity_and_capabilities_fail_closed() {
    let f = Fixture::new();
    let (mut a, _g) = f.setup();
    let path = f.p.bridge_dir.join("snapshot.json");
    let original: Snapshot = read_json(&path).unwrap();
    let mut s = original.clone();
    s.identity.slot = 2;
    write_json(&path, &s).unwrap();
    assert!(a.current().is_err());
    s = original;
    s.capabilities.repeatable_effects = false;
    write_json(&path, &s).unwrap();
    assert!(a.current().is_err());
}
#[test]
fn not_ready_does_not_queue() {
    let f = Fixture::new();
    let (mut a, mut g) = f.setup();
    g.tick(false, grant).unwrap();
    assert!(matches!(
        a.apply(&f.identity(), &effect()),
        EffectOutcome::NotReady(_)
    ));
    assert!(!f.p.bridge_dir.join("command.json").exists());
}
#[test]
fn not_ready_ack_is_final_for_that_attempt() {
    let f = Fixture::new();
    let (a, mut g) = f.setup();
    let s: Snapshot = read_json(&f.p.bridge_dir.join("snapshot.json")).unwrap();
    let c = Command {
        version: VERSION,
        challenge: s.challenge,
        boot: s.boot,
        attempt: nonce(),
        identity: f.identity(),
        effect: effect(),
    };
    write_json(&f.p.bridge_dir.join("command.json"), &c).unwrap();
    g.tick(false, grant).unwrap();
    let ack: Ack = read_json(&f.p.bridge_dir.join("ack.json")).unwrap();
    assert!(matches!(ack.outcome, EffectOutcome::NotReady(_)));
    g.tick(true, grant).unwrap();
    assert_eq!(g.save().unwrap().state, serde_json::json!(0));
    drop(a);
}
#[test]
fn durable_ack_completes_host_and_replay_does_not_duplicate() {
    let f = Fixture::new();
    let (mut a, g) = f.setup();
    let (stop, t) = game_thread(g);
    let mut host = Host::new(f.identity());
    host.enqueue(effect()).unwrap();
    let mut states = Vec::new();
    host.step(&mut a, &mut |_, r| {
        states.push(r.state.clone());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        host.records().next().unwrap().state,
        DeliveryState::Completed
    );
    assert!(matches!(states[0], DeliveryState::Uncertain(_)));
    assert_eq!(
        a.apply(&f.identity(), &effect()),
        EffectOutcome::AlreadySatisfied
    );
    stop.store(true, Ordering::SeqCst);
    let g = t.join().unwrap();
    assert_eq!(g.save().unwrap().state, serde_json::json!(1));
    drop(g);
    let mut g = GameEndpoint::open(f.p.clone(), serde_json::json!(999)).unwrap();
    a.bind(f.identity()).unwrap();
    g.tick(true, grant).unwrap();
    assert!(a.current().is_err());
    g.tick(true, grant).unwrap();
    assert!(a.current().is_ok());
    let (stop, t) = game_thread(g);
    assert_eq!(
        a.apply(&f.identity(), &effect()),
        EffectOutcome::AlreadySatisfied
    );
    stop.store(true, Ordering::SeqCst);
    assert_eq!(
        t.join().unwrap().save().unwrap().state,
        serde_json::json!(1)
    );
}
#[test]
fn queued_timeout_parks_and_explicit_retry_uses_receipt() {
    let f = Fixture::new();
    let (mut a, mut g) = f.setup();
    let mut host = Host::new(f.identity());
    host.enqueue(effect()).unwrap();
    host.step(&mut a, &mut |_, _| Ok(())).unwrap();
    assert!(matches!(
        host.records().next().unwrap().state,
        DeliveryState::Uncertain(_)
    ));
    g.tick(true, grant).unwrap();
    g.tick(true, grant).unwrap();
    let (stop, t) = game_thread(g);
    host.step(&mut a, &mut |_, _| Ok(())).unwrap();
    assert!(matches!(
        host.records().next().unwrap().state,
        DeliveryState::Uncertain(_)
    ));
    host.authorize_retry(&effect().id).unwrap();
    host.step(&mut a, &mut |_, _| Ok(())).unwrap();
    assert_eq!(
        host.records().next().unwrap().state,
        DeliveryState::Completed
    );
    stop.store(true, Ordering::SeqCst);
    assert_eq!(
        t.join().unwrap().save().unwrap().state,
        serde_json::json!(1)
    );
}
#[test]
fn different_session_cannot_rebind_game_save() {
    let f = Fixture::new();
    let (mut a, mut g) = f.setup();
    let mut id = f.identity();
    id.seed = "other".into();
    a.bind(id).unwrap();
    assert!(g.tick(true, grant).unwrap_err().contains("different seed"));
    assert_eq!(g.save().unwrap().identity, f.identity());
}
#[test]
fn check_reporting_is_independent_of_reward_and_validated() {
    let f = Fixture::new();
    let (_a, mut g) = f.setup();
    g.report_check(10).unwrap();
    assert!(g.report_check(11).is_err());
    assert_eq!(g.save().unwrap().state, serde_json::json!(0));
    assert!(g.save().unwrap().receipts.is_empty());
    g.report_goal().unwrap();
    assert!(g.save().unwrap().goal);
}
#[test]
fn failed_transaction_cannot_record_receipt() {
    let f = Fixture::new();
    let (mut a, mut g) = f.setup();
    let t = std::thread::spawn(move || {
        for _ in 0..30 {
            g.tick(true, |_, _| Err("unsupported action".into()))
                .unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        g
    });
    assert!(matches!(
        a.apply(&f.identity(), &effect()),
        EffectOutcome::Rejected(_)
    ));
    let g = t.join().unwrap();
    assert!(g.save().unwrap().receipts.is_empty());
    assert_eq!(g.save().unwrap().state, serde_json::json!(0));
}
#[test]
fn slot_contract_is_required() {
    let f = Fixture::new();
    let a = FileAdapter::open(f.p.clone()).unwrap();
    assert!(a.validate_slot(&serde_json::json!({})).is_err());
    assert!(
        a.validate_slot(&serde_json::json!({"bridge_version":1,"bridge_contract":"fixture-v1"}))
            .is_ok()
    );
}

fn existing_world_validator(profile: &Profile, slot: &Value) -> Result<()> {
    if profile.contract == "fixture-v1" && slot.get("option").and_then(Value::as_bool).is_some() {
        Ok(())
    } else {
        Err("unsupported existing-world options".into())
    }
}

fn session(f: &Fixture) -> ProtocolSession {
    ProtocolSession {
        game: f.p.game.clone(),
        seed: "seed".into(),
        team: 0,
        slot: 1,
    }
}

#[test]
fn custom_validation_is_opt_in_and_pairing_revalidates() {
    let f = Fixture::new();
    let options = serde_json::json!({"option": true});
    let mut a = FileAdapter::open(f.p.clone()).unwrap();
    assert!(a.pair_session(&session(&f), &options).is_err());
    assert!(!f.p.bridge_dir.join("session.json").exists());
    a = a
        .with_slot_validator(existing_world_validator)
        .with_slot_data();
    assert!(
        a.pair_session(&session(&f), &serde_json::json!({"option": "invalid"}))
            .is_err()
    );
    assert!(!f.p.bridge_dir.join("session.json").exists());
    a.pair_session(&session(&f), &options).unwrap();
    let h: Handshake = read_json(&f.p.bridge_dir.join("session.json")).unwrap();
    assert_eq!(h.slot_data, Some(options));
    let mut foreign = session(&f);
    foreign.game = "other".into();
    assert!(
        a.pair_session(&foreign, &serde_json::json!({"option": false}))
            .is_err()
    );
}

#[test]
fn optional_slot_data_clears_on_bind_missing_or_foreign_handshake() {
    let f = Fixture::new();
    let mut a = FileAdapter::open(f.p.clone())
        .unwrap()
        .with_slot_validator(existing_world_validator)
        .with_slot_data();
    let mut g = GameEndpoint::open(f.p.clone(), serde_json::json!(0)).unwrap();
    let options = serde_json::json!({"option": true});
    a.pair_session(&session(&f), &options).unwrap();
    g.tick(true, grant).unwrap();
    assert_eq!(g.slot_data(), Some(&options));
    a.bind(f.identity()).unwrap();
    let raw: Value = read_json(&f.p.bridge_dir.join("session.json")).unwrap();
    assert!(raw.get("slot_data").is_none());
    g.tick(true, grant).unwrap();
    assert!(g.slot_data().is_none());
    a.pair_session(&session(&f), &options).unwrap();
    g.tick(true, grant).unwrap();
    fs::remove_file(f.p.bridge_dir.join("session.json")).unwrap();
    g.tick(true, grant).unwrap();
    assert!(g.slot_data().is_none());
    a.pair_session(&session(&f), &options).unwrap();
    g.tick(true, grant).unwrap();
    let mut foreign = session(&f);
    foreign.seed = "other".into();
    a.pair_session(&foreign, &serde_json::json!({"option": false}))
        .unwrap();
    assert!(g.tick(true, grant).is_err());
    assert!(g.slot_data().is_none());
}

#[test]
fn default_pairing_omits_options_and_legacy_handshake_remains_readable() {
    let f = Fixture::new();
    let mut a = FileAdapter::open(f.p.clone()).unwrap();
    a.pair_session(
        &session(&f),
        &serde_json::json!({"bridge_version":1,"bridge_contract":"fixture-v1"}),
    )
    .unwrap();
    let path = f.p.bridge_dir.join("session.json");
    let raw: Value = read_json(&path).unwrap();
    assert!(raw.get("slot_data").is_none());
    assert!(read_json::<Handshake>(&path).unwrap().slot_data.is_none());
}

fn partial_capabilities() -> Capabilities {
    Capabilities {
        locations: false,
        goal: false,
        death_link: false,
        permanent_effects: true,
        repeatable_effects: false,
    }
}

#[test]
fn partial_capabilities_reject_unsupported_mappings_and_services() {
    let f = Fixture::new();
    assert!(
        FileAdapter::open(f.p.clone())
            .unwrap()
            .with_capabilities(partial_capabilities())
            .is_err()
    );
    let mut capabilities = bridge_capabilities();
    capabilities.death_link = true;
    assert!(
        FileAdapter::open(f.p.clone())
            .unwrap()
            .with_capabilities(capabilities)
            .is_err()
    );
}

#[test]
fn partial_snapshot_requires_exact_capabilities_and_reports_unknown_progress() {
    let mut f = Fixture::new();
    f.p.items.get_mut(&1).unwrap().kind = EffectKind::Permanent;
    let (a, _g) = f.setup();
    let mut a = a.with_capabilities(partial_capabilities()).unwrap();
    assert!(a.current().is_err());
    let path = f.p.bridge_dir.join("snapshot.json");
    let mut s: Snapshot = read_json(&path).unwrap();
    s.capabilities = partial_capabilities();
    write_json(&path, &s).unwrap();
    let snapshot = a.snapshot();
    assert!(matches!(snapshot.readiness, Readiness::Ready));
    assert!(matches!(
        snapshot.checked_locations,
        Observation::Unknown(_)
    ));
    assert!(matches!(snapshot.goal, Observation::Unknown(_)));
    s.goal = true;
    write_json(&path, &s).unwrap();
    assert!(a.current().is_err());
    s.goal = false;
    s.checked_locations.insert(10);
    write_json(&path, &s).unwrap();
    assert!(a.current().is_err());
}
#[test]
fn corrupt_existing_save_is_not_reset() {
    let f = Fixture::new();
    let (_a, g) = f.setup();
    drop(g);
    fs::write(f.p.bridge_dir.join("game-save.json"), b"broken").unwrap();
    assert!(GameEndpoint::open(f.p.clone(), Value::Null).is_err());
}
#[test]
fn permanent_observation_uses_state_not_receipt_and_reconciles() {
    let mut f = Fixture::new();
    f.p.items.get_mut(&1).unwrap().kind = EffectKind::Permanent;
    let mut a = FileAdapter::open(f.p.clone()).unwrap();
    a.bind(f.identity()).unwrap();
    let mut g = GameEndpoint::open(f.p.clone(), serde_json::json!(false)).unwrap();
    let e = Effect {
        kind: EffectKind::Permanent,
        ..effect()
    };
    let s = GameSave {
        version: VERSION,
        contract: f.p.contract.clone(),
        identity: f.identity(),
        state: serde_json::json!(false),
        receipts: BTreeMap::from([(e.id.0.clone(), e.clone())]),
        checked_locations: BTreeSet::new(),
        goal: false,
    };
    g.commit(s).unwrap();
    g.update_state(|_| Ok(serde_json::json!(false))).unwrap();
    let observer = |v: &Value, _: &Effect<Value>| Ok(v.as_bool().unwrap());
    g.tick_with_observer(true, |_, _| Ok(serde_json::json!(true)), observer)
        .unwrap();
    assert!(a.current().is_err());
    g.tick_with_observer(true, |_, _| Ok(serde_json::json!(true)), observer)
        .unwrap();
    assert_eq!(a.observe_effect(&e), Observation::Known(false));
    let t = std::thread::spawn(move || {
        for _ in 0..30 {
            g.tick_with_observer(true, |_, _| Ok(serde_json::json!(true)), observer)
                .unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        g
    });
    assert_eq!(a.apply(&f.identity(), &e), EffectOutcome::Applied);
    let g = t.join().unwrap();
    assert_eq!(g.save().unwrap().state, serde_json::json!(true));
    assert_eq!(a.observe_effect(&e), Observation::Known(true));
}

#[test]
fn stale_boot_command_cannot_execute_after_game_restart() {
    let f = Fixture::new();
    let (_a, g) = f.setup();
    let s: Snapshot = read_json(&f.p.bridge_dir.join("snapshot.json")).unwrap();
    write_json(
        &f.p.bridge_dir.join("command.json"),
        &Command {
            version: VERSION,
            challenge: s.challenge,
            boot: s.boot,
            attempt: nonce(),
            identity: f.identity(),
            effect: effect(),
        },
    )
    .unwrap();
    drop(g);
    let mut g = GameEndpoint::open(f.p.clone(), Value::Null).unwrap();
    g.tick(true, grant).unwrap();
    assert_eq!(g.save().unwrap().state, serde_json::json!(0));
    assert!(g.save().unwrap().receipts.is_empty());
}
#[test]
fn handshake_contract_mismatch_cannot_mutate_save() {
    let f = Fixture::new();
    let (_a, mut g) = f.setup();
    let mut h: Handshake = read_json(&f.p.bridge_dir.join("session.json")).unwrap();
    h.contract = "wrong".into();
    write_json(&f.p.bridge_dir.join("session.json"), &h).unwrap();
    assert!(g.tick(true, grant).is_err());
    assert_eq!(g.save().unwrap().state, serde_json::json!(0));
}

#[test]
fn spent_repeatable_is_not_restored_on_replay() {
    let f = Fixture::new();
    let (mut a, g) = f.setup();
    let (stop, t) = game_thread(g);
    assert_eq!(a.apply(&f.identity(), &effect()), EffectOutcome::Applied);
    stop.store(true, Ordering::SeqCst);
    let mut g = t.join().unwrap();
    g.update_state(|_| Ok(serde_json::json!(0))).unwrap();
    let (stop, t) = game_thread(g);
    assert_eq!(
        a.apply(&f.identity(), &effect()),
        EffectOutcome::AlreadySatisfied
    );
    stop.store(true, Ordering::SeqCst);
    let g = t.join().unwrap();
    assert_eq!(g.save().unwrap().state, serde_json::json!(0));
    assert_eq!(g.save().unwrap().receipts.len(), 1);
}
#[test]
fn failed_ack_publication_preserves_not_ready_outcome() {
    let f = Fixture::new();
    let (_a, mut g) = f.setup();
    let s: Snapshot = read_json(&f.p.bridge_dir.join("snapshot.json")).unwrap();
    let c = Command {
        version: VERSION,
        challenge: s.challenge,
        boot: s.boot,
        attempt: nonce(),
        identity: f.identity(),
        effect: effect(),
    };
    write_json(&f.p.bridge_dir.join("command.json"), &c).unwrap();
    let ack_path = f.p.bridge_dir.join("ack.json");
    fs::create_dir(&ack_path).unwrap();
    assert!(g.tick(false, grant).is_err());
    fs::remove_dir(&ack_path).unwrap();
    g.tick(true, grant).unwrap();
    let ack: Ack = read_json(&ack_path).unwrap();
    assert!(matches!(ack.outcome, EffectOutcome::NotReady(_)));
    assert_eq!(g.save().unwrap().state, serde_json::json!(0));
}
