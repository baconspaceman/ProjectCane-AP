//! Contract tests use a game seam with independently controlled observations,
//! mutations and durable checkpoints, including crash windows and seed replay.
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use peliarch_client_core::*;

fn identity() -> SessionIdentity {
    SessionIdentity {
        game: "Test game".into(),
        seed: "seed-1".into(),
        team: 0,
        slot: 1,
        save: "save-A".into(),
    }
}

fn effect(index: u64, kind: EffectKind) -> Effect<String> {
    Effect {
        id: EffectId::received_item(index),
        kind,
        payload: format!("payload-{index}"),
    }
}

struct Adapter {
    snapshot: AdapterSnapshot,
    capabilities: Capabilities,
    observed: Observation<bool>,
    outcome: EffectOutcome,
    applications: usize,
    effect_observations: usize,
    trace: Rc<RefCell<Vec<&'static str>>>,
}

impl Adapter {
    fn ready() -> Self {
        Self {
            snapshot: AdapterSnapshot {
                identity: Observation::Known(identity()),
                readiness: Readiness::Ready,
                checked_locations: Observation::Known(BTreeSet::from([42])),
                goal: Observation::Known(true),
            },
            capabilities: Capabilities {
                locations: true,
                goal: true,
                permanent_effects: true,
                repeatable_effects: true,
                death_link: false,
            },
            observed: Observation::Known(false),
            outcome: EffectOutcome::Applied,
            applications: 0,
            effect_observations: 0,
            trace: Rc::new(RefCell::new(vec![])),
        }
    }
}

impl GameAdapter for Adapter {
    type Payload = String;
    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    fn snapshot(&mut self) -> AdapterSnapshot {
        self.snapshot.clone()
    }
    fn observe_effect(&mut self, _: &Effect<String>) -> Observation<bool> {
        self.effect_observations += 1;
        self.observed.clone()
    }
    fn apply(&mut self, expected: &SessionIdentity, _: &Effect<String>) -> EffectOutcome {
        assert_eq!(
            &self.snapshot.identity,
            &Observation::Known(expected.clone())
        );
        self.trace.borrow_mut().push("apply");
        self.applications += 1;
        self.outcome.clone()
    }
}

fn checkpoint(_: &SessionIdentity, _: &DeliveryRecord<String>) -> Result<(), String> {
    Ok(())
}

#[test]
fn native_observation_preserves_pending_and_completed_effects_without_delivery() {
    let mut host = Host::new(identity());
    host.enqueue(effect(0, EffectKind::Permanent)).unwrap();
    host.enqueue(effect(1, EffectKind::Repeatable)).unwrap();
    let mut snapshot = host.export();
    snapshot.records[0].state = DeliveryState::Completed;
    let host = Host::restore(identity(), snapshot.clone()).unwrap();
    let mut adapter = Adapter::ready();
    for _ in 0..3 {
        assert_eq!(host.observe(&mut adapter).unwrap(), adapter.snapshot);
    }
    assert_eq!(adapter.applications, 0);
    assert_eq!(adapter.effect_observations, 0);
    assert_eq!(host.export(), snapshot);
}

#[test]
fn reconnect_and_full_item_replay_preserve_completed_and_uncertain_repeatables() {
    let mut host = Host::new(identity());
    let mut adapter = Adapter::ready();
    host.enqueue(effect(0, EffectKind::Repeatable)).unwrap();
    host.step(&mut adapter, &mut checkpoint).unwrap();
    adapter.outcome = EffectOutcome::Uncertain("connection lost after command".into());
    host.enqueue(effect(1, EffectKind::Repeatable)).unwrap();
    host.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(adapter.applications, 2);

    let bytes = json::to_vec(&host.export()).unwrap();
    let snapshot: HostSnapshot<String> = json::from_slice(&bytes).unwrap();
    let mut restored = Host::restore(identity(), snapshot).unwrap();
    for index in 0..2 {
        restored
            .enqueue(effect(index, EffectKind::Repeatable))
            .unwrap();
    }
    adapter.outcome = EffectOutcome::Applied;
    restored.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(
        adapter.applications, 2,
        "reconnect must not duplicate either effect"
    );
    assert!(matches!(
        restored.records().nth(1).unwrap().state,
        DeliveryState::Uncertain(_)
    ));
}

#[test]
fn permanent_reconciliation_restores_rolled_back_state_but_unknown_never_grants() {
    let mut host = Host::new(identity());
    host.enqueue(effect(0, EffectKind::Permanent)).unwrap();
    let mut adapter = Adapter::ready();
    host.step(&mut adapter, &mut checkpoint).unwrap();
    adapter.observed = Observation::Known(true);
    assert_eq!(
        host.step(&mut adapter, &mut checkpoint).unwrap().effects[0].outcome,
        EffectOutcome::AlreadySatisfied
    );
    assert_eq!(adapter.applications, 1);

    adapter.observed = Observation::Unknown("save currently loading".into());
    assert!(matches!(
        host.step(&mut adapter, &mut checkpoint).unwrap().effects[0].outcome,
        EffectOutcome::NotReady(_)
    ));
    assert_eq!(adapter.applications, 1);
    adapter.observed = Observation::Known(false);
    host.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(
        adapter.applications, 2,
        "same-save rollback should reconcile permanent state"
    );
}

#[test]
fn each_identity_dimension_blocks_delivery_and_cross_session_restore() {
    let host_identity = identity();
    let mut mismatches = Vec::new();
    for field in 0..5 {
        let mut wrong = host_identity.clone();
        match field {
            0 => wrong.game.push('x'),
            1 => wrong.seed.push('x'),
            2 => wrong.team += 1,
            3 => wrong.slot += 1,
            _ => wrong.save.push('x'),
        }
        mismatches.push(wrong);
    }
    for wrong in mismatches {
        let mut host = Host::new(host_identity.clone());
        host.enqueue(effect(0, EffectKind::Repeatable)).unwrap();
        let mut adapter = Adapter::ready();
        adapter.snapshot.identity = Observation::Known(wrong.clone());
        assert_eq!(host.observe(&mut adapter), Err(HostError::IdentityMismatch));
        assert_eq!(
            host.step(&mut adapter, &mut checkpoint),
            Err(HostError::IdentityMismatch)
        );
        assert_eq!(adapter.applications, 0);
        assert!(matches!(
            Host::restore(wrong, host.export()),
            Err(HostError::IdentityMismatch)
        ));
    }
}

#[test]
fn unknown_identity_and_not_ready_mask_stale_positive_observations() {
    for unknown_identity in [false, true] {
        let mut host = Host::new(identity());
        host.enqueue(effect(0, EffectKind::Repeatable)).unwrap();
        let mut adapter = Adapter::ready();
        if unknown_identity {
            adapter.snapshot.identity = Observation::Unknown("save absent".into());
        } else {
            adapter.snapshot.readiness = Readiness::NotReady("menu".into());
        }
        let report = host.step(&mut adapter, &mut checkpoint).unwrap();
        assert_eq!(host.observe(&mut adapter).unwrap(), report.snapshot);
        assert!(matches!(report.snapshot.goal, Observation::Unknown(_)));
        assert!(matches!(
            report.snapshot.checked_locations,
            Observation::Unknown(_)
        ));
        assert!(report.effects.is_empty());
        assert_eq!(adapter.applications, 0);
    }
}

#[test]
fn capabilities_are_enforced_for_mutations_and_observations() {
    let mut host = Host::new(identity());
    host.enqueue(effect(0, EffectKind::Permanent)).unwrap();
    host.enqueue(effect(1, EffectKind::Repeatable)).unwrap();
    let mut adapter = Adapter::ready();
    adapter.capabilities = Capabilities::default();
    let report = host.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(host.observe(&mut adapter).unwrap(), report.snapshot);
    assert!(matches!(report.snapshot.goal, Observation::Unknown(_)));
    assert!(matches!(
        report.snapshot.checked_locations,
        Observation::Unknown(_)
    ));
    assert!(
        report
            .effects
            .iter()
            .all(|r| matches!(r.outcome, EffectOutcome::Rejected(_)))
    );
    assert_eq!(adapter.applications, 0);
    assert!(
        host.records()
            .all(|r| matches!(r.state, DeliveryState::Rejected(_)))
    );
}

#[test]
fn fallible_durable_write_ahead_precedes_game_mutation() {
    let mut host = Host::new(identity());
    host.enqueue(effect(0, EffectKind::Repeatable)).unwrap();
    let mut adapter = Adapter::ready();
    let error = host
        .step(&mut adapter, &mut |_, _| Err("disk full".into()))
        .unwrap_err();
    assert_eq!(error, HostError::Checkpoint("disk full".into()));
    assert_eq!(adapter.applications, 0);

    let trace = adapter.trace.clone();
    let mut durable = host.export();
    let mut store = |session: &SessionIdentity, record: &DeliveryRecord<String>| {
        assert_eq!(session, &identity());
        trace
            .borrow_mut()
            .push(if matches!(record.state, DeliveryState::Uncertain(_)) {
                "write-ahead"
            } else {
                "commit"
            });
        durable.records = vec![record.clone()];
        Ok(())
    };
    host.step(&mut adapter, &mut store).unwrap();
    assert_eq!(
        *adapter.trace.borrow(),
        vec!["write-ahead", "apply", "commit"]
    );
    assert_eq!(durable.records[0].state, DeliveryState::Completed);
}

#[test]
fn failed_final_commit_and_restart_never_blindly_retry_repeatable() {
    let mut host = Host::new(identity());
    host.enqueue(effect(0, EffectKind::Repeatable)).unwrap();
    let mut adapter = Adapter::ready();
    let mut durable = host.export();
    let mut writes = 0;
    let error = host
        .step(
            &mut adapter,
            &mut |_: &SessionIdentity, record: &DeliveryRecord<String>| {
                writes += 1;
                if writes == 2 {
                    return Err("power loss at commit".into());
                }
                durable.records = vec![record.clone()];
                Ok(())
            },
        )
        .unwrap_err();
    assert!(matches!(error, HostError::Checkpoint(_)));
    assert_eq!(adapter.applications, 1);
    assert!(matches!(
        host.records().next().unwrap().state,
        DeliveryState::Uncertain(_)
    ));
    host.step(&mut adapter, &mut checkpoint).unwrap();
    let mut restored = Host::restore(identity(), durable).unwrap();
    restored.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(adapter.applications, 1);
}

#[test]
fn in_flight_crash_is_safe_and_uncertain_permanent_can_reconcile() {
    for kind in [EffectKind::Permanent, EffectKind::Repeatable] {
        let record = DeliveryRecord {
            effect: effect(0, kind),
            state: DeliveryState::Uncertain("process stopped during apply".into()),
        };
        let mut host = Host::restore(
            identity(),
            HostSnapshot {
                identity: identity(),
                records: vec![record],
            },
        )
        .unwrap();
        let mut adapter = Adapter::ready();
        adapter.observed = Observation::Known(true);
        host.step(&mut adapter, &mut checkpoint).unwrap();
        assert_eq!(adapter.applications, 0);
        if kind == EffectKind::Permanent {
            assert_eq!(
                host.records().next().unwrap().state,
                DeliveryState::Completed
            );
        } else {
            assert!(matches!(
                host.records().next().unwrap().state,
                DeliveryState::Uncertain(_)
            ));
        }
    }
}

#[test]
fn id_reuse_cannot_change_payload_or_kind_and_completed_retry_is_refused() {
    let mut host = Host::new(identity());
    host.enqueue(effect(0, EffectKind::Repeatable)).unwrap();
    let mut changed = effect(0, EffectKind::Repeatable);
    changed.payload = "different grant".into();
    assert!(matches!(
        host.enqueue(changed),
        Err(HostError::ConflictingEffect(_))
    ));
    assert!(matches!(
        host.enqueue(effect(0, EffectKind::Permanent)),
        Err(HostError::ConflictingEffect(_))
    ));
    let mut adapter = Adapter::ready();
    host.step(&mut adapter, &mut checkpoint).unwrap();
    assert!(matches!(
        host.authorize_retry(&EffectId::received_item(0)),
        Err(HostError::NotParked(_))
    ));
}

#[test]
fn rejected_effect_does_not_stall_later_items_and_explicit_retry_can_recover() {
    let mut host = Host::new(identity());
    host.enqueue(effect(0, EffectKind::Repeatable)).unwrap();
    let mut adapter = Adapter::ready();
    adapter.outcome = EffectOutcome::Rejected("unsupported payload".into());
    host.step(&mut adapter, &mut checkpoint).unwrap();
    host.enqueue(effect(1, EffectKind::Repeatable)).unwrap();
    adapter.outcome = EffectOutcome::Applied;
    host.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(adapter.applications, 2);
    host.authorize_retry(&EffectId::received_item(0)).unwrap();
    host.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(adapter.applications, 3);
}

#[test]
fn not_ready_outcome_can_retry_because_it_guarantees_no_side_effect() {
    let mut host = Host::new(identity());
    host.enqueue(effect(0, EffectKind::Repeatable)).unwrap();
    let mut adapter = Adapter::ready();
    adapter.outcome = EffectOutcome::NotReady("turn changing".into());
    host.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(host.records().next().unwrap().state, DeliveryState::Pending);
    adapter.outcome = EffectOutcome::Applied;
    host.step(&mut adapter, &mut checkpoint).unwrap();
    assert_eq!(
        host.records().next().unwrap().state,
        DeliveryState::Completed
    );
}
