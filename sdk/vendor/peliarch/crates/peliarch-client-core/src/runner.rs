//! Nonblocking Archipelago transport orchestration for new game adapters.
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use crate::session::SessionRunner as Connection;
use archipelago_rs::{ClientStatus, Event, ReceivedItem};
use serde::de::DeserializeOwned;

use crate::{
    AdapterSnapshot, DeliveryRecord, Effect, EffectId, EffectKind, GameAdapter, Host, HostSnapshot,
    Observation, SessionIdentity, StepReport, connection::ConnectionProfile,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolSession {
    pub game: String,
    pub seed: String,
    pub team: u32,
    pub slot: u32,
}

/// Game-specific validation and save pairing precede every delivery. An ignored
/// item is explicitly represented by `None`; mapping errors are fatal contract
/// failures and stop delivery. Unsupported individual items should instead map
/// to a payload whose adapter returns `EffectOutcome::Rejected`, so the host
/// durably parks them and continues the stream.
pub trait GameIntegration: GameAdapter {
    type SlotData: DeserializeOwned + Send + 'static;
    fn validate_slot(&self, slot_data: &Self::SlotData) -> Result<(), String>;
    fn pair_session(
        &mut self,
        session: &ProtocolSession,
        slot_data: &Self::SlotData,
    ) -> Result<SessionIdentity, String>;
    fn map_item(&self, item: &ReceivedItem) -> Result<Option<(EffectKind, Self::Payload)>, String>;

    /// Process optional protocol services after slot validation and save pairing.
    /// Runs on every connected poll, including while indexed history is pending.
    /// Links are ephemeral: adapters must check native readiness, suppress echoes
    /// and retain any temporarily deferred events themselves. Item delivery stays
    /// in the Host journal; this hook must not consume the client's event stream.
    fn protocol_tick(
        &mut self,
        _client: &mut archipelago_rs::Client<Self::SlotData>,
        _events: &[Event],
    ) -> Result<(), String> {
        Ok(())
    }

    /// Read-only game logic projection. The Runner calls this only for a validated
    /// connection with authoritative indexed history, and clears it on reconnect.
    fn tracker_view(
        &self,
        _client: &archipelago_rs::Client<Self::SlotData>,
    ) -> Result<Option<crate::ui::TrackerView>, String> {
        Ok(None)
    }
}

/// Implementations must durably checkpoint before returning success. A failed
/// load must be an error, never silently replaced with an empty journal.
pub trait RunnerPersistence<P> {
    fn load(&mut self, identity: &SessionIdentity) -> Result<Option<HostSnapshot<P>>, String>;
    fn checkpoint(
        &mut self,
        identity: &SessionIdentity,
        record: &DeliveryRecord<P>,
    ) -> Result<(), String>;
}

pub struct RunnerConfig {
    pub endpoint: String,
    pub slot_name: String,
    pub game: String,
    pub profile: ConnectionProfile,
    pub reconnect_delay: Duration,
    pub report_interval: Duration,
}

pub struct RunnerReport {
    pub connected: bool,
    pub events: Vec<Event>,
    pub step: Option<StepReport>,
    pub diagnostic: Option<String>,
}

pub struct Runner<G: GameIntegration> {
    config: RunnerConfig,
    connection: Connection<G::SlotData>,
    host: Option<Host<G::Payload>>,
    validated: bool,
    history_ready: bool,
    enabled: bool,
    retry_at: Instant,
    report_at: Instant,
}

impl<G: GameIntegration> Runner<G> {
    pub fn new(config: RunnerConfig, now: Instant) -> Self {
        let connection = Connection::new(
            config.endpoint.clone(),
            config.slot_name.as_str(),
            Some(config.game.as_str()),
            config.profile.options(),
        );
        Self {
            config,
            connection,
            host: None,
            validated: false,
            history_ready: false,
            enabled: true,
            retry_at: now,
            report_at: now,
        }
    }

    /// Drop the transport and schedule a fresh handshake; keep the delivery journal.
    pub fn reconnect(&mut self, now: Instant) {
        self.enabled = true;
        self.connection = Connection::new(
            self.config.endpoint.clone(),
            self.config.slot_name.as_str(),
            Some(self.config.game.as_str()),
            self.config.profile.options(),
        );
        self.validated = false;
        self.history_ready = false;
        self.retry_at = now + self.config.reconnect_delay;
    }

    pub fn host(&self) -> Option<&Host<G::Payload>> {
        self.host.as_ref()
    }

    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }

    pub fn client(&self) -> Option<&archipelago_rs::Client<G::SlotData>> {
        self.connection.client()
    }

    pub fn tracker_view(&self, game: &G) -> Result<Option<crate::ui::TrackerView>, String> {
        if !self.enabled || !self.validated || !self.history_ready {
            return Ok(None);
        }
        match self.connection.client() {
            Some(client) => game.tracker_view(client),
            None => Ok(None),
        }
    }

    pub fn disconnect(&mut self) {
        self.enabled = false;
        self.validated = false;
        self.history_ready = false;
        self.connection.disconnect();
    }

    /// A new endpoint may reconnect the same session. Existing host/save pairing
    /// still refuses a different seed/team/slot; this never resets a journal.
    pub fn connect(
        &mut self,
        endpoint: String,
        slot_name: String,
        password: Option<String>,
        now: Instant,
    ) {
        self.config.endpoint = endpoint;
        self.config.slot_name = slot_name;
        self.config.profile = self.config.profile.with_password(password);
        self.reconnect(now);
    }

    pub fn say(&mut self, text: String) -> Result<(), String> {
        self.connection
            .client_mut()
            .ok_or("not connected")?
            .say(text)
            .map_err(|error| error.to_string())
    }

    /// Persist explicit recovery authorization before the in-memory host can
    /// execute again. A failed checkpoint leaves the effect parked.
    pub fn retry<S: RunnerPersistence<G::Payload>>(
        &mut self,
        id: &EffectId,
        confirmed: bool,
        store: &mut S,
    ) -> Result<(), String> {
        if !confirmed {
            return Err(
                "retry requires explicit confirmation; the effect may already have happened".into(),
            );
        }
        let host = self.host.as_mut().ok_or("no paired session")?;
        let mut record = host
            .records()
            .find(|record| &record.effect.id == id)
            .cloned()
            .ok_or_else(|| format!("unknown effect {}", id.0))?;
        if !matches!(
            record.state,
            crate::DeliveryState::Rejected(_) | crate::DeliveryState::Uncertain(_)
        ) {
            return Err(format!("effect {} is not parked", id.0));
        }
        record.state = crate::DeliveryState::Pending;
        store.checkpoint(host.identity(), &record)?;
        host.authorize_retry(id).map_err(|error| error.to_string())
    }
    pub fn host_mut(&mut self) -> Option<&mut Host<G::Payload>> {
        self.host.as_mut()
    }

    /// Poll once per application tick. Disconnects retry after the configured
    /// delay; validation or persistence failures return an error without mutating
    /// the game. Reconnect replay preserves parked and completed delivery records.
    pub fn poll<S: RunnerPersistence<G::Payload>>(
        &mut self,
        now: Instant,
        game: &mut G,
        store: &mut S,
    ) -> Result<RunnerReport, String> {
        if !self.enabled {
            return Ok(RunnerReport {
                connected: false,
                events: vec![],
                step: None,
                diagnostic: None,
            });
        }
        if self.connection.is_disconnected() && now >= self.retry_at {
            self.connection = Connection::new(
                self.config.endpoint.clone(),
                self.config.slot_name.as_str(),
                Some(self.config.game.as_str()),
                self.config.profile.options(),
            );
            self.validated = false;
            self.history_ready = false;
            self.retry_at = now + self.config.reconnect_delay;
        }
        let events = self.connection.update();
        if events
            .iter()
            .any(|event| matches!(event, Event::ReceivedItems(0)))
        {
            self.history_ready = true;
        }
        if self.connection.is_disconnected() {
            // Schedule from the failure, rather than from the start of a long handshake.
            if events.iter().any(|event| matches!(event, Event::Error(_))) {
                self.retry_at = now + self.config.reconnect_delay;
            }
            return Ok(RunnerReport {
                connected: false,
                events,
                step: None,
                diagnostic: Some(self.connection.err().to_string()),
            });
        }
        let Some(client) = self.connection.client_mut() else {
            return Ok(RunnerReport {
                connected: false,
                events,
                step: None,
                diagnostic: None,
            });
        };
        // Request an authoritative replay before delivery. Some server versions
        // omit empty histories even after Sync; check reporting must not depend
        // on receiving an item, or a zero-item start cannot earn its first one.
        if !self.history_ready && events.iter().any(|event| matches!(event, Event::Connected)) {
            client.sync().map_err(|error| error.to_string())?;
        }
        if !self.validated {
            game.validate_slot(client.slot_data())?;
            let player = client.this_player();
            let protocol = ProtocolSession {
                game: player.game().to_string(),
                seed: client.seed_name().to_owned(),
                team: player.team(),
                slot: player.slot(),
            };
            let identity = game.pair_session(&protocol, client.slot_data())?;
            if identity.game != protocol.game
                || identity.seed != protocol.seed
                || identity.team != protocol.team
                || identity.slot != protocol.slot
                || identity.save.is_empty()
            {
                return Err(
                    "game paired save with a different Archipelago session or empty save identity"
                        .into(),
                );
            }
            if let Some(host) = &self.host {
                if host.identity() != &identity {
                    return Err("reconnect session differs from active delivery journal".into());
                }
            } else {
                self.host = Some(match store.load(&identity)? {
                    Some(snapshot) => {
                        Host::restore(identity, snapshot).map_err(|e| e.to_string())?
                    }
                    None => Host::new(identity),
                });
            }
            self.validated = true;
            self.report_at = now;
        }
        game.protocol_tick(client, &events)?;
        if !self.history_ready {
            let host = self.host.as_ref().expect("validated connection has host");
            let snapshot = host.observe(game).map_err(|error| error.to_string())?;
            if now >= self.report_at {
                report_observations(client, &snapshot)?;
                self.report_at = now + self.config.report_interval;
            }
            return Ok(RunnerReport {
                connected: true,
                events,
                // The UI can show the paired game as ready to earn checks even
                // though no item delivery has run before authoritative history.
                step: Some(StepReport {
                    snapshot,
                    effects: vec![],
                }),
                diagnostic: Some(
                    "awaiting full indexed item history; native progress reporting active".into(),
                ),
            });
        }
        let host = self.host.as_mut().expect("validated connection has host");
        // Use explicit wire indices, never enumerate the accumulated vector:
        // overlapping protocol batches may contain repeated indices.
        for item in client.received_items() {
            let mapped = game
                .map_item(item)
                .map_err(|reason| format!("received item {}: {reason}", item.index()))?;
            enqueue_mapping(host, EffectId::received_item(item.index() as u64), mapped)?;
        }
        let received_ids: BTreeSet<_> = client
            .received_items()
            .iter()
            .map(|item| EffectId::received_item(item.index() as u64))
            .collect();
        validate_history(host, &received_ids)?;
        let step = host
            .step(game, &mut |identity, record| {
                store.checkpoint(identity, record)
            })
            .map_err(|e| e.to_string())?;
        if now >= self.report_at {
            report_observations(client, &step.snapshot)?;
            self.report_at = now + self.config.report_interval;
        }
        Ok(RunnerReport {
            connected: true,
            events,
            step: Some(step),
            diagnostic: None,
        })
    }
}

fn report_observations<S: DeserializeOwned + Send + 'static>(
    client: &mut archipelago_rs::Client<S>,
    snapshot: &AdapterSnapshot,
) -> Result<(), String> {
    let confirmed: BTreeSet<_> = client
        .server_checked_locations()
        .map(|location| location.id())
        .collect();
    if let Observation::Known(locations) = &snapshot.checked_locations {
        let pending: Vec<_> = locations.difference(&confirmed).copied().collect();
        if !pending.is_empty() {
            client
                .mark_checked(pending)
                .map_err(|error| error.to_string())?;
        }
    }
    if snapshot.goal == Observation::Known(true) {
        client
            .set_status(ClientStatus::Goal)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn enqueue_mapping<P: Clone + Eq>(
    host: &mut Host<P>,
    id: EffectId,
    mapped: Option<(EffectKind, P)>,
) -> Result<(), String> {
    if let Some((kind, payload)) = mapped {
        host.enqueue(Effect { id, kind, payload })
            .map_err(|error| error.to_string())
    } else if host.records().any(|record| record.effect.id == id) {
        Err(format!(
            "received item {} was previously mapped but is now ignored; delivery paused",
            id.0
        ))
    } else {
        Ok(())
    }
}

fn validate_history<P: Clone + Eq>(
    host: &Host<P>,
    received_ids: &BTreeSet<EffectId>,
) -> Result<(), String> {
    for record in host.records() {
        if record.effect.id.0.starts_with("received:") && !received_ids.contains(&record.effect.id)
        {
            return Err(format!(
                "journal effect {} is absent from server item history; delivery paused",
                record.effect.id.0
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdapterSnapshot, Capabilities, EffectOutcome, Readiness};
    use std::{net::TcpListener, sync::mpsc, thread};

    struct Game {
        identity: SessionIdentity,
        snapshots: usize,
        grants: usize,
        checked: BTreeSet<i64>,
        goal: bool,
    }
    impl GameAdapter for Game {
        type Payload = u32;
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                repeatable_effects: true,
                locations: true,
                goal: true,
                ..Default::default()
            }
        }
        fn snapshot(&mut self) -> AdapterSnapshot {
            self.snapshots += 1;
            AdapterSnapshot {
                identity: Observation::Known(self.identity.clone()),
                readiness: Readiness::Ready,
                checked_locations: Observation::Known(self.checked.clone()),
                goal: Observation::Known(self.goal),
            }
        }
        fn observe_effect(&mut self, _: &Effect<u32>) -> Observation<bool> {
            Observation::Known(false)
        }
        fn apply(&mut self, _: &SessionIdentity, _: &Effect<u32>) -> EffectOutcome {
            self.grants += 1;
            EffectOutcome::Applied
        }
    }
    impl GameIntegration for Game {
        type SlotData = json::Value;
        fn validate_slot(&self, _: &json::Value) -> Result<(), String> {
            Ok(())
        }
        fn pair_session(
            &mut self,
            _: &ProtocolSession,
            _: &json::Value,
        ) -> Result<SessionIdentity, String> {
            Ok(self.identity.clone())
        }
        fn map_item(&self, item: &ReceivedItem) -> Result<Option<(EffectKind, u32)>, String> {
            Ok(Some((EffectKind::Repeatable, item.item().id() as u32)))
        }
    }
    struct Store {
        snapshot: HostSnapshot<u32>,
    }
    fn pending_host() -> Host<u32> {
        let mut host = Host::new(SessionIdentity {
            game: "Fixture".into(),
            seed: "seed".into(),
            team: 0,
            slot: 1,
            save: "save".into(),
        });
        host.enqueue(Effect {
            id: EffectId::received_item(2),
            kind: EffectKind::Repeatable,
            payload: 99,
        })
        .unwrap();
        host
    }

    fn test_runner() -> Runner<Game> {
        Runner::new(
            RunnerConfig {
                endpoint: "ws://127.0.0.1:1".into(),
                slot_name: "Fixture".into(),
                game: "Fixture".into(),
                profile: ConnectionProfile::new(true, None),
                reconnect_delay: Duration::from_secs(1),
                report_interval: Duration::from_secs(1),
            },
            Instant::now(),
        )
    }

    #[test]
    fn user_disconnect_stays_offline_and_does_not_tick_game() {
        let mut runner = test_runner();
        let host = pending_host();
        let mut game = Game {
            identity: host.identity().clone(),
            snapshots: 0,
            grants: 0,
            checked: BTreeSet::new(),
            goal: false,
        };
        let mut store = Store {
            snapshot: host.export(),
        };
        runner.host = Some(host);
        runner.disconnect();
        let report = runner
            .poll(
                Instant::now() + Duration::from_secs(100),
                &mut game,
                &mut store,
            )
            .unwrap();
        assert!(!report.connected);
        assert_eq!(game.snapshots, 0);
        assert_eq!(game.grants, 0);
        assert!(runner.client().is_none());
        assert!(runner.say("hello".into()).is_err());
        assert!(runner.host().is_some());
    }

    #[test]
    fn recovery_requires_confirmation_and_durable_authorization() {
        struct RefusingStore;
        impl RunnerPersistence<u32> for RefusingStore {
            fn load(&mut self, _: &SessionIdentity) -> Result<Option<HostSnapshot<u32>>, String> {
                unreachable!()
            }
            fn checkpoint(
                &mut self,
                _: &SessionIdentity,
                _: &DeliveryRecord<u32>,
            ) -> Result<(), String> {
                Err("disk full".into())
            }
        }
        let mut snapshot = pending_host().export();
        snapshot.records[0].state = crate::DeliveryState::Uncertain("crash".into());
        let id = snapshot.records[0].effect.id.clone();
        let mut runner = test_runner();
        runner.host = Some(Host::restore(snapshot.identity.clone(), snapshot.clone()).unwrap());
        let mut store = Store {
            snapshot: snapshot.clone(),
        };
        assert!(runner.retry(&id, false, &mut store).is_err());
        assert_eq!(store.snapshot, snapshot);
        assert!(runner.retry(&id, true, &mut RefusingStore).is_err());
        assert_eq!(runner.host().unwrap().export(), snapshot);
        runner.retry(&id, true, &mut store).unwrap();
        assert_eq!(
            store.snapshot.records[0].state,
            crate::DeliveryState::Pending
        );
        assert_eq!(runner.host().unwrap().export(), store.snapshot);
        assert!(runner.retry(&id, true, &mut store).is_err());
    }

    #[test]
    fn absent_restored_pending_index_refuses_delivery_until_history_recovers() {
        let host = pending_host();
        let restored = Host::restore(host.identity().clone(), host.export()).unwrap();
        let incomplete = BTreeSet::from([EffectId::received_item(0), EffectId::received_item(1)]);
        let error = validate_history(&restored, &incomplete).unwrap_err();
        assert!(error.contains(&EffectId::received_item(2).0));
        assert!(error.contains("absent from server item history"));
        assert_eq!(
            restored.records().next().unwrap().state,
            crate::DeliveryState::Pending
        );
        let mut complete = incomplete;
        complete.insert(EffectId::received_item(2));
        validate_history(&restored, &complete).unwrap();
    }

    #[test]
    fn ignored_mapping_cannot_execute_a_previously_mapped_receipt() {
        let mut host = pending_host();
        let before = host.export();
        let error = enqueue_mapping(&mut host, EffectId::received_item(2), None).unwrap_err();
        assert!(error.contains("previously mapped but is now ignored"));
        assert_eq!(host.export(), before);
        enqueue_mapping(&mut host, EffectId::received_item(3), None).unwrap();
        assert_eq!(host.export(), before);
        assert!(
            enqueue_mapping(
                &mut host,
                EffectId::received_item(2),
                Some((EffectKind::Repeatable, 100))
            )
            .is_err()
        );
        enqueue_mapping(
            &mut host,
            EffectId::received_item(2),
            Some((EffectKind::Repeatable, 99)),
        )
        .unwrap();
    }

    impl RunnerPersistence<u32> for Store {
        fn load(&mut self, _: &SessionIdentity) -> Result<Option<HostSnapshot<u32>>, String> {
            Ok(Some(self.snapshot.clone()))
        }
        fn checkpoint(
            &mut self,
            _: &SessionIdentity,
            record: &DeliveryRecord<u32>,
        ) -> Result<(), String> {
            if let Some(old) = self
                .snapshot
                .records
                .iter_mut()
                .find(|old| old.effect.id == record.effect.id)
            {
                *old = record.clone();
            } else {
                self.snapshot.records.push(record.clone());
            }
            Ok(())
        }
    }

    struct LinkedGame {
        game: Game,
        reject_slot: bool,
        reject_pairing: bool,
        protocol_ticks: usize,
        bounces: usize,
    }
    impl GameAdapter for LinkedGame {
        type Payload = u32;
        fn capabilities(&self) -> Capabilities {
            self.game.capabilities()
        }
        fn snapshot(&mut self) -> AdapterSnapshot {
            self.game.snapshot()
        }
        fn observe_effect(&mut self, effect: &Effect<u32>) -> Observation<bool> {
            self.game.observe_effect(effect)
        }
        fn apply(&mut self, identity: &SessionIdentity, effect: &Effect<u32>) -> EffectOutcome {
            self.game.apply(identity, effect)
        }
    }
    impl GameIntegration for LinkedGame {
        type SlotData = json::Value;
        fn validate_slot(&self, _: &json::Value) -> Result<(), String> {
            if self.reject_slot {
                Err("slot rejected".into())
            } else {
                Ok(())
            }
        }
        fn pair_session(
            &mut self,
            _: &ProtocolSession,
            _: &json::Value,
        ) -> Result<SessionIdentity, String> {
            if self.reject_pairing {
                Err("pairing rejected".into())
            } else {
                Ok(self.game.identity.clone())
            }
        }
        fn map_item(&self, item: &ReceivedItem) -> Result<Option<(EffectKind, u32)>, String> {
            self.game.map_item(item)
        }
        fn protocol_tick(
            &mut self,
            client: &mut archipelago_rs::Client<json::Value>,
            events: &[Event],
        ) -> Result<(), String> {
            self.protocol_ticks += 1;
            if events.iter().any(|event| matches!(event, Event::Connected)) {
                client
                    .update_connection(None, Some(["AP", "RingLink"]))
                    .map_err(|e| e.to_string())?;
                client
                    .set("fixture_stage_0_1", json::json!(3), false)
                    .map_err(|e| e.to_string())?;
                client
                    .bounce(
                        json::json!({"amount":2,"source":123}),
                        archipelago_rs::BounceOptions::new().tags(["RingLink"]),
                    )
                    .map_err(|e| e.to_string())?;
            }
            for event in events {
                if let Event::Bounce {
                    data: Some(data), ..
                } = event
                {
                    assert_eq!(data["amount"], 4);
                    self.bounces += 1;
                }
            }
            Ok(())
        }
        fn tracker_view(
            &self,
            _: &archipelago_rs::Client<json::Value>,
        ) -> Result<Option<crate::ui::TrackerView>, String> {
            Ok(Some(crate::ui::TrackerView {
                regions: vec![crate::ui::TrackerRegion {
                    name: "Fixture region".into(),
                    total: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }))
        }
    }

    // Exercise the actual wire/client/Runner boundary, including a zero-item
    // start. Protocol services cannot bypass slot, pairing or journal validation.
    #[test]
    fn protocol_services_require_pairing_and_tracker_requires_authoritative_history() {
        for rejection in ["none", "slot", "pairing", "journal"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("ws://{}", listener.local_addr().unwrap());
            let (history_tx, history_rx) = mpsc::channel();
            let (finish_tx, finish_rx) = mpsc::channel();
            let server = thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut socket =
                    tungstenite::accept_with_config(stream, Some(Default::default())).unwrap();
                socket.send(tungstenite::Message::Text(json::json!([{"cmd":"RoomInfo","version":{"major":0,"minor":6,"build":0,"class":"Version"},"generator_version":{"major":0,"minor":6,"build":0,"class":"Version"},"tags":[],"password":false,"permissions":{"release":0,"collect":0,"remaining":0},"hint_cost":10,"location_check_points":1,"games":["Fixture"],"datapackage_checksums":{},"seed_name":"seed","time":0.0}]).to_string().into())).unwrap();
                let connect: json::Value =
                    json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                assert_eq!(connect[0]["cmd"], "Connect");
                socket.send(tungstenite::Message::Text(json::json!([{"cmd":"Connected","team":0,"slot":1,"players":[{"team":0,"slot":1,"alias":"slot","name":"slot"}],"missing_locations":[],"checked_locations":[],"slot_data":{},"slot_info":{"1":{"name":"slot","game":"Fixture","type":1,"group_members":[]}},"hint_points":0}]).to_string().into())).unwrap();
                let sync: json::Value =
                    json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                assert_eq!(sync[0]["cmd"], "Sync");
                if rejection == "none" {
                    let mut seen = BTreeSet::new();
                    while seen.len() < 3 {
                        let packet: json::Value =
                            json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                        for message in packet.as_array().unwrap() {
                            match message["cmd"].as_str().unwrap() {
                                "ConnectUpdate" => {
                                    assert!(
                                        message["tags"]
                                            .as_array()
                                            .unwrap()
                                            .contains(&json::json!("RingLink"))
                                    );
                                    seen.insert("tags");
                                }
                                "Set" => {
                                    assert_eq!(message["key"], "fixture_stage_0_1");
                                    assert_eq!(message["operations"][0]["value"], 3);
                                    seen.insert("storage");
                                }
                                "Bounce" => {
                                    assert_eq!(message["data"]["amount"], 2);
                                    seen.insert("link");
                                }
                                other => panic!("unexpected outgoing protocol message {other}"),
                            }
                        }
                    }
                    history_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    socket.send(tungstenite::Message::Text(json::json!([
                        {"cmd":"Bounced","tags":["RingLink"],"data":{"amount":4,"source":456}},
                        {"cmd":"ReceivedItems","index":0,"items":[]}
                    ]).to_string().into())).unwrap();
                }
                finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            });
            let identity = SessionIdentity {
                game: "Fixture".into(),
                seed: "seed".into(),
                team: 0,
                slot: 1,
                save: "save".into(),
            };
            let mut game = LinkedGame {
                game: Game {
                    identity: identity.clone(),
                    snapshots: 0,
                    grants: 0,
                    checked: BTreeSet::new(),
                    goal: false,
                },
                reject_slot: rejection == "slot",
                reject_pairing: rejection == "pairing",
                protocol_ticks: 0,
                bounces: 0,
            };
            let mut store = Store {
                snapshot: HostSnapshot {
                    identity,
                    records: vec![],
                },
            };
            if rejection == "journal" {
                store.snapshot.identity.seed = "another seed".into();
            }
            let mut runner = Runner::new(
                RunnerConfig {
                    endpoint,
                    slot_name: "slot".into(),
                    game: "Fixture".into(),
                    profile: ConnectionProfile::new(true, None),
                    reconnect_delay: Duration::from_secs(5),
                    report_interval: Duration::from_secs(1),
                },
                Instant::now(),
            );
            assert!(runner.tracker_view(&game).unwrap().is_none());
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let result = runner.poll(Instant::now(), &mut game, &mut store);
                if rejection != "none" {
                    if result.is_err() {
                        assert_eq!(game.protocol_ticks, 0, "rejected {rejection} ran services");
                        assert_eq!(game.game.snapshots, 0);
                        assert!(runner.tracker_view(&game).unwrap().is_none());
                        break;
                    }
                } else {
                    let report = result.unwrap();
                    if game.protocol_ticks > 0 {
                        assert!(report.connected);
                        assert!(
                            runner.tracker_view(&game).unwrap().is_none(),
                            "projection must wait for indexed history"
                        );
                        break;
                    }
                }
                assert!(Instant::now() < deadline, "connection stalled: {rejection}");
                thread::sleep(Duration::from_millis(1));
            }
            if rejection == "none" {
                history_tx.send(()).unwrap();
                let mut report_retained_bounce = false;
                while game.bounces == 0 {
                    let report = runner.poll(Instant::now(), &mut game, &mut store).unwrap();
                    report_retained_bounce |= report
                        .events
                        .iter()
                        .any(|event| matches!(event, Event::Bounce { .. }));
                    assert!(Instant::now() < deadline, "incoming link/history stalled");
                    thread::sleep(Duration::from_millis(1));
                }
                assert!(report_retained_bounce, "service hook consumed UI events");
                assert_eq!(
                    runner.tracker_view(&game).unwrap().unwrap().regions[0].name,
                    "Fixture region"
                );
                let ticks = game.protocol_ticks;
                runner.disconnect();
                runner.poll(Instant::now(), &mut game, &mut store).unwrap();
                assert_eq!(game.protocol_ticks, ticks);
                assert!(runner.tracker_view(&game).unwrap().is_none());
            }
            finish_tx.send(()).unwrap();
            server.join().unwrap();
        }
    }

    #[test]
    fn zero_starting_items_syncs_then_reports_first_check_and_receives_reward() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let (finish_tx, finish_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket =
                tungstenite::accept_with_config(stream, Some(Default::default())).unwrap();
            let room = json::json!([{"cmd":"RoomInfo","version":{"major":0,"minor":6,"build":0,"class":"Version"},"generator_version":{"major":0,"minor":6,"build":0,"class":"Version"},"tags":[],"password":false,"permissions":{"release":0,"collect":0,"remaining":0},"hint_cost":10,"location_check_points":1,"games":["Fixture"],"datapackage_checksums":{},"seed_name":"seed","time":0.0}]);
            socket
                .send(tungstenite::Message::Text(room.to_string().into()))
                .unwrap();
            let packet: json::Value =
                json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(packet[0]["cmd"], "Connect");
            let connected = json::json!([{"cmd":"Connected","team":0,"slot":1,"players":[{"team":0,"slot":1,"alias":"slot","name":"slot"}],"missing_locations":[7],"checked_locations":[],"slot_data":{},"slot_info":{"1":{"name":"slot","game":"Fixture","type":1,"group_members":[]}},"hint_points":0}]);
            socket
                .send(tungstenite::Message::Text(connected.to_string().into()))
                .unwrap();
            // Match a real server with zero starting items: no unsolicited history.
            let packet: json::Value =
                json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(
                packet[0]["cmd"], "Sync",
                "must request authoritative history before delivering items"
            );
            // The pinned production AP server omits empty history even for Sync.
            // Only the first earned check below makes an item packet available.
            let packet: json::Value =
                json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(packet[0]["cmd"], "LocationChecks");
            assert_eq!(packet[0]["locations"], json::json!([7]));
            socket.send(tungstenite::Message::Text(json::json!([{"cmd":"ReceivedItems","index":0,"items":[{"item":99,"location":-2,"player":0,"flags":0}]}]).to_string().into())).unwrap();
            finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let identity = SessionIdentity {
            game: "Fixture".into(),
            seed: "seed".into(),
            team: 0,
            slot: 1,
            save: "save".into(),
        };
        let mut game = Game {
            identity: identity.clone(),
            snapshots: 0,
            grants: 0,
            checked: BTreeSet::from([7]),
            goal: false,
        };
        let mut store = Store {
            snapshot: HostSnapshot {
                identity,
                records: vec![],
            },
        };
        let mut runner = Runner::new(
            RunnerConfig {
                endpoint,
                slot_name: "slot".into(),
                game: "Fixture".into(),
                profile: ConnectionProfile::new(true, None),
                reconnect_delay: Duration::from_secs(5),
                report_interval: Duration::from_secs(1),
            },
            Instant::now(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while game.grants == 0 {
            let report = runner.poll(Instant::now(), &mut game, &mut store).unwrap();
            assert!(
                Instant::now() < deadline,
                "zero-item start stalled: {:?}",
                report.diagnostic
            );
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(game.grants, 1);
        finish_tx.send(()).unwrap();
        server.join().unwrap();
    }

    // Mirrors AP 0.6.8's empty-history behavior: neither Connected nor Sync
    // produces ReceivedItems until a check creates a nonempty received list.
    #[test]
    fn two_empty_slots_report_native_progress_then_receive_first_remote_item() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let (permit_tx, permit_rx) = mpsc::channel();
        let (progress_tx, progress_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let mut sockets = Vec::new();
            for _ in 1..=2 {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut socket =
                    tungstenite::accept_with_config(stream, Some(Default::default())).unwrap();
                let room = json::json!([{"cmd":"RoomInfo","version":{"major":0,"minor":6,"build":8,"class":"Version"},"generator_version":{"major":0,"minor":6,"build":8,"class":"Version"},"tags":[],"password":false,"permissions":{"release":0,"collect":0,"remaining":0},"hint_cost":10,"location_check_points":1,"games":["Fixture"],"datapackage_checksums":{},"seed_name":"seed","time":0.0}]);
                socket
                    .send(tungstenite::Message::Text(room.to_string().into()))
                    .unwrap();
                let packet: json::Value =
                    json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                assert_eq!(packet[0]["cmd"], "Connect");
                let slot = match packet[0]["name"].as_str().unwrap() {
                    "one" => 1,
                    "two" => 2,
                    name => panic!("unexpected slot name {name}"),
                };
                let connected = json::json!([{"cmd":"Connected","team":0,"slot":slot,"players":[{"team":0,"slot":1,"alias":"one","name":"one"},{"team":0,"slot":2,"alias":"two","name":"two"}],"missing_locations":[41],"checked_locations":[],"slot_data":{},"slot_info":{"1":{"name":"one","game":"Fixture","type":1,"group_members":[]},"2":{"name":"two","game":"Fixture","type":1,"group_members":[]}},"hint_points":0}]);
                socket
                    .send(tungstenite::Message::Text(connected.to_string().into()))
                    .unwrap();
                sockets.push((slot, socket));
            }
            assert_eq!(
                sockets
                    .iter()
                    .map(|(slot, _)| *slot)
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([1, 2])
            );
            for (_, socket) in &mut sockets {
                let mut checked = false;
                let mut goal = false;
                let mut sync = false;
                while !(checked && goal && sync) {
                    let packet: json::Value =
                        json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                    for command in packet.as_array().unwrap() {
                        match command["cmd"].as_str().unwrap() {
                            "LocationChecks" => {
                                assert_eq!(command["locations"], json::json!([41]));
                                checked = true;
                            }
                            "StatusUpdate" => {
                                assert_eq!(command["status"], 30);
                                goal = true;
                            }
                            "Sync" => sync = true, // Empty history: deliberately no packet.
                            "GetDataPackage" => {}
                            other => panic!("unexpected command {other}"),
                        }
                    }
                }
            }
            progress_tx.send(()).unwrap();
            permit_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            for (slot, socket) in &mut sockets {
                let other_slot = if *slot == 1 { 2 } else { 1 };
                let first_item = json::json!([{"cmd":"ReceivedItems","index":0,"items":[{"item":99,"location":41,"player":other_slot,"flags":0}]}]);
                socket
                    .send(tungstenite::Message::Text(first_item.to_string().into()))
                    .unwrap();
            }
            permit_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let mut sessions = Vec::new();
        for slot in 1..=2 {
            let identity = SessionIdentity {
                game: "Fixture".into(),
                seed: "seed".into(),
                team: 0,
                slot,
                save: format!("save-{slot}"),
            };
            let game = Game {
                identity: identity.clone(),
                snapshots: 0,
                grants: 0,
                checked: BTreeSet::from([41]),
                goal: true,
            };
            let store = Store {
                snapshot: HostSnapshot {
                    identity,
                    records: vec![],
                },
            };
            let runner = Runner::new(
                RunnerConfig {
                    endpoint: endpoint.clone(),
                    slot_name: if slot == 1 { "one" } else { "two" }.into(),
                    game: "Fixture".into(),
                    profile: ConnectionProfile::new(true, None),
                    reconnect_delay: Duration::from_secs(5),
                    report_interval: Duration::from_secs(1),
                },
                Instant::now(),
            );
            sessions.push((runner, game, store));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            for (runner, game, store) in &mut sessions {
                let report = runner.poll(Instant::now(), game, store).unwrap();
                if report.connected {
                    assert!(report.step.unwrap().effects.is_empty());
                    assert!(
                        report
                            .diagnostic
                            .unwrap()
                            .contains("native progress reporting active")
                    );
                }
                assert!(!runner.history_ready);
                assert_eq!(game.grants, 0);
                assert!(runner.host().is_none_or(|host| host.records().count() == 0));
            }
            if progress_rx.try_recv().is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "empty slots failed to send progress"
            );
            thread::sleep(Duration::from_millis(1));
        }
        permit_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            for (runner, game, store) in &mut sessions {
                runner.poll(Instant::now(), game, store).unwrap();
            }
            if sessions.iter().all(|(_, game, _)| game.grants == 1) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "first remote items were not delivered"
            );
            thread::sleep(Duration::from_millis(1));
        }
        for (runner, _, store) in &sessions {
            assert!(runner.history_ready);
            assert_eq!(store.snapshot.records.len(), 1);
            assert_eq!(store.snapshot.records[0].effect.payload, 99);
            assert_eq!(
                store.snapshot.records[0].state,
                crate::DeliveryState::Completed
            );
        }
        permit_tx.send(()).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn real_socket_waits_for_history_and_reconnect_preserves_repeatable_receipts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let (permit_tx, permit_rx) = mpsc::channel();
        let (connected_tx, connected_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut socket =
                    tungstenite::accept_with_config(stream, Some(Default::default())).unwrap();
                let room = json::json!([{"cmd":"RoomInfo","version":{"major":0,"minor":6,"build":0,"class":"Version"},"generator_version":{"major":0,"minor":6,"build":0,"class":"Version"},"tags":[],"password":false,"permissions":{"release":0,"collect":0,"remaining":0},"hint_cost":10,"location_check_points":1,"games":["Fixture"],"datapackage_checksums":{},"seed_name":"seed","time":0.0}]);
                socket
                    .send(tungstenite::Message::Text(room.to_string().into()))
                    .unwrap();
                let packet: json::Value =
                    json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                assert_eq!(packet[0]["cmd"], "Connect");
                let connected = json::json!([{"cmd":"Connected","team":0,"slot":1,"players":[{"team":0,"slot":1,"alias":"slot","name":"slot"}],"missing_locations":[],"checked_locations":[],"slot_data":{},"slot_info":{"1":{"name":"slot","game":"Fixture","type":1,"group_members":[]}},"hint_points":0}]);
                socket
                    .send(tungstenite::Message::Text(connected.to_string().into()))
                    .unwrap();
                connected_tx.send(()).unwrap();
                permit_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                socket
                    .send(tungstenite::Message::Text(
                        json::json!([{"cmd":"ReceivedItems","index":0,"items":[{"item":99,"location":-2,"player":0,"flags":0}]}])
                            .to_string()
                            .into(),
                    ))
                    .unwrap();
                permit_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        });
        let identity = SessionIdentity {
            game: "Fixture".into(),
            seed: "seed".into(),
            team: 0,
            slot: 1,
            save: "save".into(),
        };
        let mut game = Game {
            identity: identity.clone(),
            snapshots: 0,
            grants: 0,
            checked: BTreeSet::new(),
            goal: false,
        };
        let mut store = Store {
            snapshot: HostSnapshot {
                identity,
                records: vec![DeliveryRecord {
                    effect: Effect {
                        id: EffectId::received_item(0),
                        kind: EffectKind::Repeatable,
                        payload: 99,
                    },
                    state: crate::DeliveryState::Pending,
                }],
            },
        };
        let mut runner = Runner::new(
            RunnerConfig {
                endpoint,
                slot_name: "slot".into(),
                game: "Fixture".into(),
                profile: ConnectionProfile::new(true, None),
                reconnect_delay: Duration::from_secs(5),
                report_interval: Duration::from_secs(1),
            },
            Instant::now(),
        );
        for attempt in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let report = runner.poll(Instant::now(), &mut game, &mut store).unwrap();
                if report.connected {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "handshake timed out: {:?}",
                    report.diagnostic
                );
                thread::sleep(Duration::from_millis(1));
            }
            connected_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let grants = game.grants;
            let journal = runner.host().unwrap().export();
            for _ in 0..3 {
                let report = runner.poll(Instant::now(), &mut game, &mut store).unwrap();
                assert!(report.step.unwrap().effects.is_empty());
                assert!(report.diagnostic.unwrap().contains("awaiting full"));
                assert!(!runner.history_ready);
            }
            assert_eq!(
                game.grants, grants,
                "must not grant restored items before full replay"
            );
            assert_eq!(runner.host().unwrap().export(), journal);
            permit_tx.send(()).unwrap();
            loop {
                runner.poll(Instant::now(), &mut game, &mut store).unwrap();
                if runner.history_ready {
                    break;
                }
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(game.grants, 1, "repeatable must not repeat after reconnect");
            if attempt == 0 {
                runner.reconnect(Instant::now());
            }
            permit_tx.send(()).unwrap();
        }
        server.join().unwrap();
    }
}
