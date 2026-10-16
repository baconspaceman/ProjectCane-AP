use peliarch_client_core::ui::{ApState, ClientSnapshot, DeliveryState, UiAction};

#[derive(Default)]
pub struct ViewModel {
    pub server: String,
    pub slot: String,
    pub password: String,
    pub chat: String,
    pub command: String,
    pub retry_index: Option<u64>,
    pub retry_confirmed: bool,
    pub feedback: Option<String>,
}

impl ViewModel {
    pub fn connect(&self) -> Result<UiAction, String> {
        if !peliarch_client_core::connection::is_connectable(self.server.trim()) {
            return Err("Enter a ws:// or wss:// server address with a host.".into());
        }
        if self.slot.trim().is_empty() {
            return Err("Enter a slot name.".into());
        }
        Ok(UiAction::Connect {
            server: self.server.trim().into(),
            slot: self.slot.trim().into(),
            password: (!self.password.is_empty()).then(|| self.password.clone()),
        })
    }
    pub fn chat_action(&self) -> Result<UiAction, String> {
        let text = self.chat.trim();
        if text.is_empty() {
            return Err("Enter a chat message.".into());
        }
        Ok(UiAction::SubmitCommand(format!("say {text}")))
    }
    pub fn command_action(&self) -> Result<UiAction, String> {
        let command = self.command.trim();
        if command.is_empty() {
            return Err("Enter a client command.".into());
        }
        let words: Vec<_> = command.trim_start_matches('!').split_whitespace().collect();
        if words.first() == Some(&"retry") && words.last() != Some(&"CONFIRM") {
            return Err("Recovery command requires retry INDEX CONFIRM; the effect may already have happened.".into());
        }
        Ok(UiAction::SubmitCommand(command.into()))
    }
    pub fn retry(&self, snapshot: &ClientSnapshot) -> Result<UiAction, String> {
        if snapshot.observer_only {
            return Err("This tracker does not deliver or recover items.".into());
        }
        let index = self.retry_index.ok_or("Select an effect to recover.")?;
        if !self.retry_confirmed {
            return Err("Confirm the duplicate delivery risk first.".into());
        }
        if !snapshot.blocked.iter().any(|e| e.index == index) {
            return Err("This effect is no longer available for recovery.".into());
        }
        Ok(UiAction::RetryBlocked { index })
    }
}

pub fn status(snapshot: &ClientSnapshot) -> String {
    if snapshot.stale {
        return "Worker status is stale; delivery readiness is unknown.".into();
    }
    if snapshot.ap != ApState::Authenticated {
        return match snapshot.ap {
            ApState::Connecting => "Connecting to Archipelago…",
            ApState::Reconnecting => "Reconnecting to Archipelago…",
            _ => "Disconnected from Archipelago.",
        }
        .into();
    }
    if let Some(reason) = snapshot
        .delivery_detail
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        return reason.into();
    }
    match snapshot.delivery {
        DeliveryState::Ready => "Ready to deliver effects.",
        DeliveryState::Parked => "Delivery continues; some effects need review.",
        DeliveryState::Blocked => "Delivery is blocked; review the activity log.",
        DeliveryState::CommandPending => "An effect is in flight.",
        DeliveryState::NotArmed => "Waiting for validated seed and save pairing.",
        _ => "Waiting for adapter readiness.",
    }
    .into()
}

/// A renderer describes only the reachability supplied by a worker. Unknown logic is not
/// evidence that a location is reachable, or that the player has run out of progression.
pub fn tracker_notice(snapshot: &ClientSnapshot) -> Option<&'static str> {
    if snapshot.observer_only {
        return Some(
            "Inventory is unknown to this tracker; reachability and goal completion are unavailable.",
        );
    }
    if snapshot.tracker.is_none() {
        return Some(if snapshot.locations.is_some() {
            "Reachability unavailable; showing check counts and unchecked locations."
        } else {
            "Waiting for check metadata. Reachability has not been supplied."
        });
    }
    if snapshot.stale {
        return Some("Last reported tracker state; waiting for a fresh update.");
    }
    if snapshot.gameplay_ready
        && (snapshot.tracker_only || snapshot.ap == ApState::Authenticated)
        && standalone_egui::view::tracker_stuck(snapshot)
    {
        return Some(
            "No checks outside questlines are currently reported in logic. Review the requirements below or available hints.",
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use peliarch_client_core::ui::BlockedEntry;
    #[test]
    fn retry_requires_confirmation_and_current_record() {
        let mut model = ViewModel {
            retry_index: Some(7),
            ..Default::default()
        };
        let mut snapshot = ClientSnapshot::default();
        snapshot.blocked.push(BlockedEntry {
            index: 7,
            item_name: "Effect".into(),
            reason: "uncertain".into(),
        });
        assert!(model.retry(&snapshot).is_err());
        model.retry_confirmed = true;
        assert_eq!(
            model.retry(&snapshot).unwrap(),
            UiAction::RetryBlocked { index: 7 }
        );
        snapshot.blocked.clear();
        assert!(model.retry(&snapshot).is_err());
    }
    #[test]
    fn connection_and_chat_preserve_password_and_validate_input() {
        let mut model = ViewModel::default();
        assert!(model.connect().is_err());
        model.server = " ws://localhost:38281 ".into();
        model.slot = " Slot ".into();
        model.password = " secret ".into();
        assert_eq!(
            model.connect().unwrap(),
            UiAction::Connect {
                server: "ws://localhost:38281".into(),
                slot: "Slot".into(),
                password: Some(" secret ".into())
            }
        );
        assert!(model.chat_action().is_err());
        model.chat = " Hello ".into();
        assert_eq!(
            model.chat_action().unwrap(),
            UiAction::SubmitCommand("say Hello".into())
        );
    }
    #[test]
    fn readiness_is_generic_and_stale_wins() {
        let mut snapshot = ClientSnapshot {
            ap: ApState::Authenticated,
            delivery: DeliveryState::Ready,
            ..Default::default()
        };
        assert_eq!(status(&snapshot), "Ready to deliver effects.");
        snapshot.stale = true;
        assert!(status(&snapshot).contains("unknown"));
    }

    #[test]
    fn missing_logic_and_stale_or_disconnected_views_never_claim_the_player_is_stuck() {
        use peliarch_client_core::ui::{LocationTotals, TrackerRegion, TrackerView};

        let mut snapshot = ClientSnapshot {
            ap: ApState::Authenticated,
            gameplay_ready: true,
            locations: Some(LocationTotals {
                checked: 1,
                total: 3,
            }),
            ..Default::default()
        };
        assert!(
            tracker_notice(&snapshot)
                .unwrap()
                .starts_with("Reachability unavailable")
        );
        snapshot.tracker = Some(TrackerView {
            regions: vec![TrackerRegion {
                name: "Closed region".into(),
                total: 3,
                checked: 1,
                needs: Some("A key".into()),
                ..Default::default()
            }],
            ..Default::default()
        });
        assert!(tracker_notice(&snapshot).unwrap().starts_with("No checks"));
        snapshot.ap = ApState::Disconnected;
        assert!(tracker_notice(&snapshot).is_none());
        snapshot.ap = ApState::Authenticated;
        snapshot.gameplay_ready = false;
        assert!(tracker_notice(&snapshot).is_none());
        snapshot.stale = true;
        assert!(
            tracker_notice(&snapshot)
                .unwrap()
                .starts_with("Last reported")
        );
    }
}
