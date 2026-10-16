//! Shared frontend worker for upstream integrations that already own delivery.
use archipelago_rs::Event;
use peliarch_client_core::{
    observer::{ObserverSession, ObserverSnapshot},
    ui::*,
};

use crate::Worker;

/// Game-specific, read-only interpretation of validated server facts.
pub trait ObserverTracker: Send + 'static {
    fn project(&mut self, snapshot: &ObserverSnapshot) -> Result<Option<TrackerView>, String>;
}

/// Counts-only fallback when a world provides no supported tracker model.
impl ObserverTracker for () {
    fn project(&mut self, _: &ObserverSnapshot) -> Result<Option<TrackerView>, String> {
        Ok(None)
    }
}

pub struct ObserverWorker<P: ObserverTracker> {
    game: String,
    provider: P,
    session: Option<ObserverSession>,
    endpoint: Option<String>,
    slot: Option<String>,
    // Runtime only: not Debug, persisted, or included in snapshots.
    password: Option<String>,
    reducer: SnapshotReducer,
    last_provider_error: Option<String>,
}

impl<P: ObserverTracker> ObserverWorker<P> {
    pub fn new(game: impl Into<String>, provider: P) -> Self {
        Self {
            game: game.into(),
            provider,
            session: None,
            endpoint: None,
            slot: None,
            password: None,
            reducer: SnapshotReducer::default(),
            last_provider_error: None,
        }
    }

    fn connect(&mut self, endpoint: String, slot: String, password: Option<String>) {
        self.session = Some(ObserverSession::connect(
            &endpoint,
            &slot,
            &self.game,
            password.clone(),
        ));
        self.endpoint = Some(endpoint);
        self.slot = Some(slot);
        self.password = password;
        self.last_provider_error = None;
    }
}

impl<P: ObserverTracker> Worker for ObserverWorker<P> {
    fn action(&mut self, action: UiAction) -> Result<(), String> {
        let result = match action {
            UiAction::Connect { server, slot, password } => {
                self.connect(server, slot, password);
                Ok(())
            }
            UiAction::Disconnect | UiAction::RequestShutdown => {
                self.session = None;
                Ok(())
            }
            UiAction::SubmitCommand(command) if command == "reconnect" => {
                if let (Some(endpoint), Some(slot)) = (&self.endpoint, &self.slot) {
                    self.connect(endpoint.clone(), slot.clone(), self.password.clone());
                    Ok(())
                } else {
                    Err("Connect to a server first.".into())
                }
            }
            _ => Err("This tracker is read-only. Use the game's existing client for game actions and chat.".into()),
        };
        if let Err(reason) = &result {
            self.reducer.activity(ActivityKind::Error, reason);
        }
        result
    }

    fn tick(&mut self) -> ClientSnapshot {
        let mut facts = DeliveryFacts {
            observer_only: true,
            server: self.endpoint.clone(),
            slot: self.slot.clone(),
            version: Some(format!("Peliarch {} · {} tracker", env!("CARGO_PKG_VERSION"), self.game)),
            delivery_detail: Some("Read-only tracker. The game's existing integration handles checks and items; inventory and goal status are unknown here.".into()),
            ..Default::default()
        };
        if let Some(session) = &mut self.session {
            for event in session.update() {
                match event {
                    Event::Print(print) => self
                        .reducer
                        .activity(ActivityKind::Message, print.to_string()),
                    Event::Error(error) => {
                        let error = if matches!(error, archipelago_rs::Error::Elsewhere) {
                            session.failure().unwrap_or(&error)
                        } else {
                            &error
                        };
                        self.reducer
                            .activity(ActivityKind::Error, error.to_string());
                    }
                    Event::Connected => self
                        .reducer
                        .activity(ActivityKind::Message, "Tracker connected to Archipelago."),
                    _ => {}
                }
            }
            facts.ap = if session.is_connecting() {
                ApState::Connecting
            } else {
                ApState::Disconnected
            };
            if let Some(snapshot) = session.snapshot() {
                facts.ap = ApState::Authenticated;
                facts.seed = Some(snapshot.seed.clone());
                facts.locations = Some(LocationTotals {
                    checked: snapshot.checked.len() as u32,
                    total: snapshot.locations.len() as u32,
                });
                facts.unchecked_locations = snapshot
                    .locations
                    .iter()
                    .filter(|(id, _)| !snapshot.checked.contains(id))
                    .map(|(_, name)| UncheckedLocation {
                        name: name.clone(),
                        region: None,
                    })
                    .collect();
                match self.provider.project(&snapshot) {
                    Ok(tracker) => {
                        facts.tracker = tracker;
                        self.last_provider_error = None;
                    }
                    Err(reason) => {
                        if self.last_provider_error.as_ref() != Some(&reason) {
                            self.reducer.activity(
                                ActivityKind::Error,
                                format!("Tracker model refused: {reason}"),
                            );
                        }
                        facts.delivery_detail = Some(format!(
                            "Tracker model refused: {reason}. Showing server check counts only."
                        ));
                        self.last_provider_error = Some(reason);
                    }
                }
            }
        }
        self.reducer.reduce(facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fatal_connection_error_displays_the_cause_instead_of_elsewhere() {
        let mut worker = ObserverWorker::new("World of Warcraft", ());
        worker
            .action(UiAction::Connect {
                server: "ws://[".into(),
                slot: "slot".into(),
                password: None,
            })
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let snapshot = worker.tick();
            if let Some(error) = snapshot
                .activity
                .iter()
                .find(|e| e.kind == ActivityKind::Error)
            {
                assert!(!error.text.contains("elsewhere"), "{}", error.text);
                assert!(!error.text.is_empty());
                assert_eq!(snapshot.ap, ApState::Disconnected);
                assert!(snapshot.locations.is_none());
                assert!(snapshot.tracker.is_none());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn observer_has_no_delivery_actions_or_readiness() {
        let mut worker = ObserverWorker::new("World of Warcraft", ());
        for action in [
            UiAction::RetryBlocked { index: 0 },
            UiAction::SubmitCommand("retry 0 CONFIRM".into()),
            UiAction::SubmitCommand("say !release".into()),
        ] {
            assert!(worker.action(action).unwrap_err().contains("read-only"));
        }
        let snapshot = worker.tick();
        assert!(snapshot.observer_only);
        assert!(!snapshot.tracker_only);
        assert!(!snapshot.gameplay_ready);
        assert!(snapshot.receive_cursor.is_none());
        assert!(snapshot.save_identity.is_none());
        assert_eq!(snapshot.process, ProcessState::Waiting);
        assert_eq!(snapshot.delivery, DeliveryState::NotArmed);
        assert!(snapshot.tracker.is_none());
        assert!(snapshot.goal.is_none());
        assert!(snapshot.victory.is_none());
        assert!(snapshot.locations.is_none());
    }
}
