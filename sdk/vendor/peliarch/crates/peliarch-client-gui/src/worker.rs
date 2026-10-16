use crate::Worker;
use archipelago_rs::Event;
use peliarch_client_core::{
    DeliveryState as RecordState, EffectId, Observation, Readiness,
    runner::{GameIntegration, Runner, RunnerPersistence},
    ui::*,
};
use std::{collections::BTreeMap, time::Instant};

pub struct RunnerWorker<G: GameIntegration, S> {
    runner: Runner<G>,
    game: G,
    store: S,
    reducer: SnapshotReducer,
    disconnected: bool,
    last_error: Option<String>,
    last_tracker_error: Option<String>,
    effect_outcomes: BTreeMap<EffectId, String>,
}
impl<G: GameIntegration, S> RunnerWorker<G, S> {
    pub fn new(runner: Runner<G>, game: G, store: S) -> Self {
        Self {
            runner,
            game,
            store,
            reducer: SnapshotReducer::default(),
            disconnected: false,
            last_error: None,
            last_tracker_error: None,
            effect_outcomes: BTreeMap::new(),
        }
    }
}
impl<G, S> Worker for RunnerWorker<G, S>
where
    G: GameIntegration + Send + 'static,
    G::Payload: Send,
    S: RunnerPersistence<G::Payload> + Send + 'static,
{
    fn action(&mut self, action: UiAction) -> Result<(), String> {
        let result = match action {
            UiAction::Connect {
                server,
                slot,
                password,
            } => {
                self.runner.connect(server, slot, password, Instant::now());
                self.disconnected = false;
                Ok(())
            }
            UiAction::Disconnect => {
                self.runner.disconnect();
                self.disconnected = true;
                Ok(())
            }
            UiAction::SubmitCommand(command) if command == "reconnect" => {
                self.runner.reconnect(Instant::now());
                self.disconnected = false;
                Ok(())
            }
            UiAction::SubmitCommand(command) => {
                if let Some(text) = command.strip_prefix("say ") {
                    self.runner.say(text.into())
                } else if command.split_whitespace().next() == Some("retry") {
                    let parts: Vec<_> = command.split_whitespace().collect();
                    if parts.len() != 3 || parts[2] != "CONFIRM" {
                        Err("Recovery requires retry INDEX CONFIRM; the effect may already have happened.".into())
                    } else {
                        parts[1]
                            .parse::<u64>()
                            .map_err(|_| "Retry index must be a nonnegative integer.".to_owned())
                            .and_then(|index| {
                                self.runner.retry(
                                    &EffectId::received_item(index),
                                    true,
                                    &mut self.store,
                                )
                            })
                    }
                } else {
                    Err("Unsupported command; use the connection and recovery controls.".into())
                }
            }
            UiAction::RetryBlocked { index } => {
                self.runner
                    .retry(&EffectId::received_item(index), true, &mut self.store)
            }
            UiAction::RequestShutdown => Ok(()),
            UiAction::OpenSessionFolder => {
                Err("This adapter does not expose a session folder.".into())
            }
        };
        self.reducer.activity(
            if result.is_ok() {
                ActivityKind::CommandResult
            } else {
                ActivityKind::Error
            },
            result.as_ref().map_or_else(
                |e| format!("Action refused: {e}"),
                |_| "Action accepted.".into(),
            ),
        );
        result
    }
    fn tick(&mut self) -> ClientSnapshot {
        let report = self
            .runner
            .poll(Instant::now(), &mut self.game, &mut self.store);
        let mut facts = DeliveryFacts {
            server: Some(self.runner.config().endpoint.clone()),
            slot: Some(self.runner.config().slot_name.clone()),
            version: Some(format!(
                "Peliarch {} · {}",
                env!("CARGO_PKG_VERSION"),
                self.runner.config().game
            )),
            ..Default::default()
        };
        match report {
            Ok(report) => {
                facts.ap = if report.connected {
                    ApState::Authenticated
                } else if self.disconnected {
                    ApState::Disconnected
                } else {
                    ApState::Connecting
                };
                facts.delivery_detail = report.diagnostic;
                for event in report.events {
                    match event {
                        Event::Print(print) => self
                            .reducer
                            .activity(ActivityKind::Message, print.to_string()),
                        Event::Error(error) => self
                            .reducer
                            .activity(ActivityKind::Error, error.to_string()),
                        Event::Connected => self
                            .reducer
                            .activity(ActivityKind::Message, "Authenticated with Archipelago."),
                        Event::ReceivedItems(index) => {
                            if let Some(client) = self.runner.client() {
                                for item in client
                                    .received_items()
                                    .iter()
                                    .filter(|item| item.index() >= index)
                                {
                                    self.reducer.activity_with_class(
                                        ActivityKind::ReceivedItem,
                                        format!(
                                            "#{} {} from {}",
                                            item.index(),
                                            item.item(),
                                            item.sender()
                                        ),
                                        Some(ItemClass::from_flags(
                                            item.is_progression(),
                                            item.is_useful(),
                                            item.is_trap(),
                                        )),
                                    );
                                }
                            }
                        }
                        Event::Updated(_) => self.reducer.activity(
                            ActivityKind::LocationCheck,
                            "Server session/check state updated.",
                        ),
                        _ => {}
                    }
                }
                if let Some(step) = report.step {
                    facts.process = ProcessState::Attached;
                    facts.gameplay_ready = step.snapshot.readiness == Readiness::Ready;
                    facts.delivery = if facts.gameplay_ready {
                        DeliveryState::Ready
                    } else {
                        DeliveryState::WaitingForGameplay
                    };
                    if let Readiness::NotReady(reason) = step.snapshot.readiness {
                        facts.delivery_detail = Some(reason);
                    }
                    if let Observation::Unknown(reason) = step.snapshot.identity {
                        facts.gameplay_ready = false;
                        facts.delivery = DeliveryState::Blocked;
                        facts.delivery_detail = Some(reason);
                    }
                    if let Observation::Known(goal) = step.snapshot.goal {
                        facts.goal = Some(if goal { "Complete" } else { "In progress" }.into());
                    }
                    for effect in step.effects {
                        let outcome = format!("{:?}", effect.outcome);
                        if outcome_changed(&mut self.effect_outcomes, effect.id.clone(), &outcome) {
                            self.reducer.activity(
                                ActivityKind::CommandResult,
                                format!("{}: {outcome}", effect.id.0),
                            );
                        }
                    }
                }
                self.last_error = None;
                match self.runner.tracker_view(&self.game) {
                    Ok(tracker) => {
                        facts.tracker = tracker;
                        self.last_tracker_error = None;
                    }
                    Err(reason) => {
                        if self.last_tracker_error.as_ref() != Some(&reason) {
                            self.reducer.activity(
                                ActivityKind::Error,
                                format!("Tracker unavailable: {reason}"),
                            );
                        }
                        self.last_tracker_error = Some(reason);
                    }
                }
            }
            Err(reason) => {
                facts.delivery = DeliveryState::Blocked;
                facts.delivery_detail = Some(reason.clone());
                facts.ap = if self.runner.client().is_some() {
                    ApState::Authenticated
                } else {
                    ApState::Disconnected
                };
                if self.last_error.as_ref() != Some(&reason) {
                    self.reducer.activity(ActivityKind::Error, &reason);
                }
                self.last_error = Some(reason);
            }
        }
        if let Some(host) = self.runner.host() {
            facts.seed = Some(host.identity().seed.clone());
            facts.save_identity = Some(host.identity().save.clone());
            for record in host.records() {
                match &record.state {
                    RecordState::Pending => facts.ledger.queued += 1,
                    RecordState::Completed => facts.ledger.delivered += 1,
                    RecordState::Rejected(reason) | RecordState::Uncertain(reason) => {
                        facts.ledger.parked += 1;
                        if let Some(index) = record
                            .effect
                            .id
                            .0
                            .strip_prefix("received:")
                            .and_then(|s| s.parse().ok())
                        {
                            facts.blocked.push(BlockedEntry {
                                index,
                                item_name: record.effect.id.0.clone(),
                                reason: format!("{:?}: {reason}", record.state),
                            });
                        }
                    }
                }
            }
            if facts.ledger.parked > 0 && facts.delivery == DeliveryState::Ready {
                facts.delivery = DeliveryState::Parked;
            }
        }
        if let Some(client) = self.runner.client() {
            let checked = client.server_checked_locations().count() as u32;
            let optimistic_checked = client.checked_locations().count() as u32;
            let unchecked = client.unchecked_locations().count() as u32;
            facts.locations = Some(LocationTotals {
                checked,
                total: optimistic_checked + unchecked,
            });
            facts.unchecked_locations = client
                .unchecked_locations()
                .map(|l| UncheckedLocation {
                    name: l.to_string(),
                    region: None,
                })
                .collect();
            facts.receive_cursor = client.received_items().last().map(|i| i.index() as u64);
        }
        self.reducer.reduce(facts)
    }
}

fn outcome_changed(outcomes: &mut BTreeMap<EffectId, String>, id: EffectId, outcome: &str) -> bool {
    if outcomes.get(&id).is_some_and(|old| old == outcome) {
        return false;
    }
    outcomes.insert(id, outcome.into());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use archipelago_rs::ReceivedItem;
    use peliarch_client_core::{
        AdapterSnapshot, Capabilities, DeliveryRecord, Effect, EffectKind, EffectOutcome,
        GameAdapter, HostSnapshot, SessionIdentity,
        connection::ConnectionProfile,
        runner::{ProtocolSession, RunnerConfig},
    };
    use std::time::Duration;
    struct OfflineGame;
    impl GameAdapter for OfflineGame {
        type Payload = ();
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
        fn snapshot(&mut self) -> AdapterSnapshot {
            panic!("disconnected worker must not poll adapter")
        }
        fn observe_effect(&mut self, _: &Effect<()>) -> Observation<bool> {
            panic!("must not observe")
        }
        fn apply(&mut self, _: &SessionIdentity, _: &Effect<()>) -> EffectOutcome {
            panic!("must not mutate")
        }
    }
    impl GameIntegration for OfflineGame {
        type SlotData = ();
        fn validate_slot(&self, _: &()) -> Result<(), String> {
            panic!("must not validate disconnected session")
        }
        fn pair_session(&mut self, _: &ProtocolSession, _: &()) -> Result<SessionIdentity, String> {
            panic!("must not pair")
        }
        fn map_item(&self, _: &ReceivedItem) -> Result<Option<(EffectKind, ())>, String> {
            panic!("must not map")
        }
        fn protocol_tick(
            &mut self,
            _: &mut archipelago_rs::Client<()>,
            _: &[Event],
        ) -> Result<(), String> {
            panic!("must not process links while disconnected")
        }
        fn tracker_view(
            &self,
            _: &archipelago_rs::Client<()>,
        ) -> Result<Option<TrackerView>, String> {
            panic!("must not project a disconnected tracker")
        }
    }
    struct OfflineStore;
    impl RunnerPersistence<()> for OfflineStore {
        fn load(&mut self, _: &SessionIdentity) -> Result<Option<HostSnapshot<()>>, String> {
            panic!("must not load without pairing")
        }
        fn checkpoint(
            &mut self,
            _: &SessionIdentity,
            _: &DeliveryRecord<()>,
        ) -> Result<(), String> {
            panic!("must not checkpoint without pairing")
        }
    }
    #[test]
    fn disconnect_action_suppresses_adapter_and_retry_without_confirm_is_refused() {
        let runner = Runner::new(
            RunnerConfig {
                endpoint: "ws://127.0.0.1:9".into(),
                slot_name: "Offline".into(),
                game: "Fixture".into(),
                profile: ConnectionProfile::new(true, None),
                reconnect_delay: Duration::from_secs(1),
                report_interval: Duration::from_secs(1),
            },
            Instant::now(),
        );
        let mut worker = RunnerWorker::new(runner, OfflineGame, OfflineStore);
        worker.action(UiAction::Disconnect).unwrap();
        for _ in 0..5 {
            let snapshot = worker.tick();
            assert_eq!(snapshot.ap, ApState::Disconnected);
            assert!(!snapshot.gameplay_ready);
            assert!(snapshot.save_identity.is_none());
        }
        assert!(
            worker
                .action(UiAction::SubmitCommand("retry 7".into()))
                .unwrap_err()
                .contains("CONFIRM")
        );
        assert!(
            worker
                .action(UiAction::SubmitCommand("retry 7 CONFIRM".into()))
                .unwrap_err()
                .contains("no paired session")
        );
        assert!(
            worker
                .action(UiAction::RetryBlocked { index: 7 })
                .unwrap_err()
                .contains("no paired session")
        );
    }
    #[test]
    fn unchanged_permanent_outcomes_do_not_flood_activity() {
        let mut outcomes = BTreeMap::new();
        let id = EffectId::received_item(1);
        assert!(outcome_changed(&mut outcomes, id.clone(), "Applied"));
        assert!(outcome_changed(
            &mut outcomes,
            id.clone(),
            "AlreadySatisfied"
        ));
        for _ in 0..1000 {
            assert!(!outcome_changed(
                &mut outcomes,
                id.clone(),
                "AlreadySatisfied"
            ));
        }
        assert!(outcome_changed(&mut outcomes, id, "NotReady"));
    }
}
