use crate::view::{ViewModel, status, tracker_notice};
use egui::{Color32, RichText};
use peliarch_client_core::ui::{
    ClientSnapshot, HINT_COMMAND, HintOffer, HostEndpoint, UiAction, group_unchecked_locations,
};
use std::time::{Duration, Instant};

pub struct Frontend {
    endpoint: HostEndpoint,
    pub model: ViewModel,
    snapshot: ClientSnapshot,
    received_snapshot: bool,
    last_snapshot: Instant,
}

impl Frontend {
    pub fn new(endpoint: HostEndpoint) -> Self {
        Self {
            endpoint,
            model: ViewModel::default(),
            snapshot: ClientSnapshot::default(),
            received_snapshot: false,
            last_snapshot: Instant::now(),
        }
    }
    fn send(&mut self, action: Result<UiAction, String>) {
        self.model.feedback =
            match action {
                Ok(action) => self.endpoint.send_action(action).err().map(|_| {
                    "Worker unavailable or busy; action was not queued. Try again.".into()
                }),
                Err(reason) => Some(reason),
            };
    }
    pub fn draw(&mut self, ctx: &egui::Context) {
        if let Some(snapshot) = self.endpoint.latest_snapshot() {
            if !self.received_snapshot {
                self.model.server = snapshot
                    .server
                    .clone()
                    .unwrap_or_else(|| "ws://localhost:38281".into());
                self.model.slot = snapshot.slot.clone().unwrap_or_default();
            }
            self.snapshot = snapshot;
            self.received_snapshot = true;
            self.last_snapshot = Instant::now();
        }
        if self.last_snapshot.elapsed() > Duration::from_secs(3) {
            self.snapshot.stale = true;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        egui::TopBottomPanel::top("connection").show(ctx, |ui| {
            ui.heading("Peliarch · Universal Archipelago Client");
            ui.label(status(&self.snapshot));
            ui.horizontal(|ui| {
                ui.label("Server");
                ui.add(egui::TextEdit::singleline(&mut self.model.server).desired_width(240.0));
                ui.label("Slot");
                ui.add(egui::TextEdit::singleline(&mut self.model.slot).desired_width(160.0));
            });
            ui.horizontal(|ui| {
                ui.label("Password");
                ui.add(
                    egui::TextEdit::singleline(&mut self.model.password)
                        .password(true)
                        .desired_width(180.0),
                );
                if ui.button("Connect").clicked() {
                    self.send(self.model.connect());
                }
                if ui.button("Disconnect").clicked() {
                    self.send(Ok(UiAction::Disconnect));
                }
                if ui.button("Reconnect").clicked() {
                    self.send(Ok(UiAction::SubmitCommand("reconnect".into())));
                }
            });
            if let Some(reason) = &self.model.feedback {
                ui.colored_label(Color32::LIGHT_RED, reason);
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Connection: {:?}", self.snapshot.ap));
                ui.label(format!(
                    "Seed: {}",
                    self.snapshot
                        .seed
                        .as_deref()
                        .unwrap_or("awaiting validation")
                ));
                if self.snapshot.observer_only {
                    ui.label("Read-only tracker · delivery handled by game integration");
                } else {
                    ui.label(format!(
                        "Save: {}",
                        self.snapshot.save_identity.as_deref().unwrap_or("unpaired")
                    ));
                    ui.label(if self.snapshot.gameplay_ready {
                        "Adapter: ready"
                    } else {
                        "Adapter: waiting"
                    });
                }
            });
            if let Some(version) = &self.snapshot.version {
                ui.small(version);
            }
        });
        egui::TopBottomPanel::bottom("chat").show(ctx, |ui| {
            if self.snapshot.observer_only {
                ui.small("Use the game's existing client for chat and commands.");
                return;
            }
            ui.horizontal(|ui| {
                ui.label("Chat");
                let input = ui.add(
                    egui::TextEdit::singleline(&mut self.model.chat)
                        .desired_width(ui.available_width() - 70.0),
                );
                if ui.button("Send").clicked()
                    || (input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    self.send(self.model.chat_action());
                    if self.model.feedback.is_none() {
                        self.model.chat.clear();
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Command");
                let input = ui.add(
                    egui::TextEdit::singleline(&mut self.model.command)
                        .hint_text("status / blocked / native recovery command")
                        .desired_width(ui.available_width() - 70.0),
                );
                if ui.button("Run").clicked()
                    || (input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    self.send(self.model.command_action());
                    if self.model.feedback.is_none() {
                        self.model.command.clear();
                    }
                }
            });
        });
        egui::SidePanel::right("session")
            .default_width(360.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Session");
                if let Some((fraction, label)) =
                    standalone_egui::view::checks_progress(&self.snapshot)
                {
                    ui.add(egui::ProgressBar::new(fraction).text(label));
                } else {
                    ui.label("Checks: awaiting seed metadata");
                }
                if !self.snapshot.observer_only {
                ui.label(format!(
                    "Received cursor: {}",
                    self.snapshot
                        .receive_cursor
                        .map_or_else(|| "unknown".into(), |v| v.to_string())
                ));
                ui.label(format!(
                    "Queued: {} · Completed: {} · Parked: {}",
                    self.snapshot.ledger.queued,
                    self.snapshot.ledger.delivered,
                    self.snapshot.ledger.parked
                ));
                }
                if let Some(goal) = &self.snapshot.goal {
                    ui.label(format!("Goal: {goal}"));
                }
                if self.snapshot.victory.is_some() {
                    ui.colored_label(Color32::LIGHT_GREEN, "Goal complete");
                }
                ui.separator();
                if let Some(action) = draw_tracker(ui, &self.snapshot) {
                    self.send(Ok(action));
                }
                ui.separator();
                if !self.snapshot.observer_only {
                egui::CollapsingHeader::new("Parked / uncertain effects")
                    .default_open(!self.snapshot.blocked.is_empty())
                    .show(ui, |ui| {
                        ui.small("Uncertain effects may have been applied. Retrying can grant them twice.");
                        egui::ScrollArea::vertical()
                            .id_salt("recovery")
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for row in &self.snapshot.blocked {
                                    if ui
                                        .selectable_label(
                                            self.model.retry_index == Some(row.index),
                                            format!("#{} {}", row.index, row.item_name),
                                        )
                                        .clicked()
                                    {
                                        self.model.retry_index = Some(row.index);
                                        self.model.retry_confirmed = false;
                                    }
                                    ui.small(&row.reason);
                                }
                                if self.snapshot.blocked.is_empty() {
                                    ui.label("None reported");
                                }
                            });
                        ui.checkbox(
                            &mut self.model.retry_confirmed,
                            "I accept the duplicate delivery risk",
                        );
                        if ui
                            .add_enabled(
                                self.model.retry_index.is_some() && self.model.retry_confirmed,
                                egui::Button::new("Confirm retry selected effect"),
                            )
                            .clicked()
                        {
                            self.send(self.model.retry(&self.snapshot));
                            if self.model.feedback.is_none() {
                                self.model.retry_index = None;
                                self.model.retry_confirmed = false;
                            }
                        }
                    });
                ui.separator();
                }
                ui.heading("Unchecked locations");
                egui::ScrollArea::vertical()
                    .id_salt("locations")
                    .show(ui, |ui| {
                        for group in group_unchecked_locations(&self.snapshot.unchecked_locations) {
                            ui.collapsing(
                                format!("{} ({})", group.region, group.locations.len()),
                                |ui| {
                                    for name in group.locations {
                                        ui.label(name);
                                    }
                                },
                            );
                        }
                    });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Activity log");
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for event in &self.snapshot.activity {
                        let style = standalone_egui::view::event_style(event);
                        let text = RichText::new(format!("{} {}", style.glyph, event.text)).color(
                            Color32::from_rgb(style.color[0], style.color[1], style.color[2]),
                        );
                        ui.add(
                            egui::Label::new(if style.monospace {
                                text.monospace()
                            } else {
                                text
                            })
                            .selectable(true),
                        );
                    }
                });
        });
    }
}

/// The worker supplies the entire tracker projection. Rendering never evaluates game rules,
/// reads placements, or participates in delivery acknowledgements.
fn draw_tracker(ui: &mut egui::Ui, snapshot: &ClientSnapshot) -> Option<UiAction> {
    use standalone_egui::view::{tracker_header, tracker_hints, tracker_region_line};

    ui.heading("Tracker");
    if snapshot.observer_only {
        if let Some(locations) = &snapshot.locations {
            ui.label(format!(
                "{} / {} checked",
                locations.checked, locations.total
            ));
        }
    } else if let Some(header) = tracker_header(snapshot) {
        ui.label(header);
    }
    if let Some(notice) = tracker_notice(snapshot) {
        ui.small(notice);
    }
    let tracker = snapshot.tracker.as_ref()?;
    let mut action = None;
    egui::ScrollArea::vertical()
        .id_salt("tracker-regions")
        .max_height(if snapshot.blocked.is_empty() {
            360.0
        } else {
            160.0
        })
        .show(ui, |ui| {
            for (index, region) in tracker.regions.iter().enumerate() {
                // Egui's bundled font does not cover the legacy diamond marker on every host.
                let label = if snapshot.observer_only {
                    format!(
                        "{} · {} / {} checked",
                        region.name, region.checked, region.total
                    )
                } else {
                    tracker_region_line(region).replace(" \u{25c6} ", " · in logic: ")
                };
                egui::CollapsingHeader::new(label)
                    .id_salt(("tracker-region", index))
                    .default_open(!region.in_logic.is_empty() || !region.quest.is_empty())
                    .show(ui, |ui| {
                        for location in &region.in_logic {
                            ui.label(location);
                        }
                        for location in &region.quest {
                            ui.label(format!("{location} (quest)"));
                        }
                        if snapshot.observer_only {
                            ui.small("Reachability unknown: inventory is unavailable.");
                        } else if region.in_logic.is_empty() && region.quest.is_empty() {
                            ui.small("No remaining checks reported in logic here.");
                        }
                    });
            }
            for hint in tracker_hints(snapshot) {
                ui.label(hint);
            }
            if snapshot.tracker_only {
                match &tracker.hint_offer {
                    Some(HintOffer::Available) => {
                        if ui
                            .add_enabled(!snapshot.stale, egui::Button::new("Reveal a hint"))
                            .clicked()
                        {
                            action = Some(UiAction::SubmitCommand(HINT_COMMAND.into()));
                        }
                    }
                    Some(HintOffer::Exhausted { report_url }) => {
                        ui.hyperlink_to("No further hints; report this tracker state", report_url);
                    }
                    None => {}
                }
            }
        });
    action
}

impl Drop for Frontend {
    fn drop(&mut self) {
        let _ = self.endpoint.send_action(UiAction::RequestShutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peliarch_client_core::ui::{ActivityEvent, ActivityKind, ApState, BlockedEntry, UiBridge};

    fn tracker_text(snapshot: &ClientSnapshot) -> String {
        let ctx = egui::Context::default();
        let mut output = egui::FullOutput::default();
        for _ in 0..2 {
            output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 1200.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        assert!(draw_tracker(ui, snapshot).is_none());
                    });
                },
            );
        }
        output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(shape) = &shape.shape {
                    Some(shape.galley.job.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn tracker_renders_supplied_frontier_and_only_explicitly_revealed_hints() {
        use peliarch_client_core::ui::{
            LocationTotals, TrackerRegion, TrackerView, UncheckedLocation,
        };

        let mut snapshot = ClientSnapshot {
            locations: Some(LocationTotals {
                checked: 2,
                total: 5,
            }),
            tracker: Some(TrackerView {
                regions: vec![
                    TrackerRegion {
                        name: "Valley".into(),
                        checked: 2,
                        total: 4,
                        in_logic: vec!["Track finish".into()],
                        quest: vec!["Quest hand-in".into()],
                        ..Default::default()
                    },
                    TrackerRegion {
                        name: "Mountain".into(),
                        total: 1,
                        needs: Some("Mountain key".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            unchecked_locations: vec![UncheckedLocation {
                name: "Unproven reachability".into(),
                region: None,
            }],
            ..Default::default()
        };
        let text = tracker_text(&snapshot);
        for expected in [
            "2 / 5 checked",
            "1 in logic",
            "Track finish",
            "Quest hand-in (quest)",
            "needs Mountain key",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert!(!text.contains("Unproven reachability"));
        assert!(!text.contains("hint:"));
        snapshot
            .tracker
            .as_mut()
            .unwrap()
            .hints
            .push("Revealed item - Valley: Track finish".into());
        assert!(tracker_text(&snapshot).contains("hint: Revealed item - Valley: Track finish"));
    }

    #[test]
    fn older_seed_renders_counts_without_inventing_logic() {
        use peliarch_client_core::ui::LocationTotals;
        let text = tracker_text(&ClientSnapshot {
            locations: Some(LocationTotals {
                checked: 2,
                total: 5,
            }),
            ..Default::default()
        });
        assert!(text.contains("2 / 5 checked"));
        assert!(text.contains("Reachability unavailable"));
        assert!(!text.contains("in logic"));
        assert!(!text.contains("No checks"));
    }

    #[test]
    fn observer_renders_unknown_inventory_without_zero_reachability_or_delivery_controls() {
        use peliarch_client_core::ui::{LocationTotals, TrackerRegion, TrackerView};
        let snapshot = ClientSnapshot {
            observer_only: true,
            locations: Some(LocationTotals {
                checked: 2,
                total: 5,
            }),
            tracker: Some(TrackerView {
                regions: vec![TrackerRegion {
                    name: "Elwynn".into(),
                    checked: 2,
                    total: 5,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let text = tracker_text(&snapshot);
        assert!(text.contains("2 / 5 checked"));
        assert!(text.contains("Inventory is unknown"));
        assert!(!text.contains("0 in logic"));
        assert!(!text.contains("No remaining checks"));
        let (client, endpoint) = UiBridge::new(4).split();
        client.publish(snapshot);
        let mut frontend = Frontend::new(endpoint);
        let ctx = egui::Context::default();
        let _ = frame(&mut frontend, &ctx, vec![]);
        let output = frame(&mut frontend, &ctx, vec![]);
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(shape) = &shape.shape {
                    Some(shape.galley.job.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Read-only tracker"));
        for absent in [
            "Parked / uncertain effects",
            "Received cursor:",
            "Save: unpaired",
            "Adapter: waiting",
            "Queued:",
            "Confirm retry",
        ] {
            assert!(!text.contains(absent), "unexpected {absent}");
        }
    }

    #[test]
    fn an_ap_session_never_offers_the_standalone_hint_action() {
        use peliarch_client_core::ui::TrackerView;
        let mut snapshot = ClientSnapshot {
            tracker: Some(TrackerView {
                hint_offer: Some(HintOffer::Available),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!tracker_text(&snapshot).contains("Reveal a hint"));
        snapshot.tracker_only = true;
        assert!(tracker_text(&snapshot).contains("Reveal a hint"));
    }

    fn frame(
        frontend: &mut Frontend,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(860.0, 680.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| frontend.draw(ctx),
        )
    }
    fn click_text(frontend: &mut Frontend, ctx: &egui::Context, text: &str) {
        let _ = frame(frontend, ctx, vec![]);
        let output = frame(frontend, ctx, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(shape) = &shape.shape
                    && shape.galley.job.text == text
                {
                    Some(shape.pos + shape.galley.size() / 2.0)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("missing rendered control {text}"));
        for pressed in [true, false] {
            let _ = frame(
                frontend,
                ctx,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::default(),
                    },
                ],
            );
        }
    }
    #[test]
    fn native_widgets_send_connection_chat_and_explicitly_confirmed_retry() {
        let (client, endpoint) = UiBridge::new(8).split();
        client.publish(ClientSnapshot {
            server: Some("ws://localhost:38281".into()),
            slot: Some("Fixture".into()),
            blocked: vec![BlockedEntry {
                index: 7,
                item_name: "Fixture effect".into(),
                reason: "uncertain".into(),
            }],
            ..Default::default()
        });
        let mut frontend = Frontend::new(endpoint);
        let ctx = egui::Context::default();
        click_text(&mut frontend, &ctx, "Connect");
        assert_eq!(
            client.try_action().unwrap(),
            UiAction::Connect {
                server: "ws://localhost:38281".into(),
                slot: "Fixture".into(),
                password: None
            }
        );
        frontend.model.chat = "Fixture hello".into();
        click_text(&mut frontend, &ctx, "Send");
        assert_eq!(
            client.try_action().unwrap(),
            UiAction::SubmitCommand("say Fixture hello".into())
        );
        assert!(frontend.model.chat.is_empty());
        frontend.model.command = "status".into();
        click_text(&mut frontend, &ctx, "Run");
        assert_eq!(
            client.try_action().unwrap(),
            UiAction::SubmitCommand("status".into())
        );
        frontend.model.command = "retry 7".into();
        click_text(&mut frontend, &ctx, "Run");
        assert!(client.try_action().is_err());
        click_text(&mut frontend, &ctx, "#7 Fixture effect");
        click_text(&mut frontend, &ctx, "Confirm retry selected effect");
        assert!(client.try_action().is_err());
        click_text(&mut frontend, &ctx, "I accept the duplicate delivery risk");
        click_text(&mut frontend, &ctx, "Confirm retry selected effect");
        assert_eq!(
            client.try_action().unwrap(),
            UiAction::RetryBlocked { index: 7 }
        );
        assert!(!frontend.model.retry_confirmed);
        assert!(frontend.model.retry_index.is_none());
    }
    #[test]
    fn renderer_produces_tessellated_content_with_recovery_and_activity() {
        let (client, endpoint) = UiBridge::new(4).split();
        let mut snapshot = ClientSnapshot {
            ap: ApState::Authenticated,
            seed: Some("fixture-seed".into()),
            ..Default::default()
        };
        snapshot.blocked.push(BlockedEntry {
            index: 2,
            item_name: "Uncertain effect".into(),
            reason: "receipt not observed".into(),
        });
        snapshot.push_activity(
            ActivityEvent::now(1, ActivityKind::ReceivedItem, "Received fixture item"),
            10,
        );
        client.publish(snapshot);
        let mut frontend = Frontend::new(endpoint);
        let ctx = egui::Context::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(860.0, 680.0),
                )),
                ..Default::default()
            },
            |ctx| frontend.draw(ctx),
        );
        let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
        assert!(!meshes.is_empty());
        assert!(frontend.received_snapshot);
        assert_eq!(frontend.snapshot.blocked.len(), 1);
    }
    #[test]
    fn stopped_worker_snapshot_becomes_stale() {
        let (_client, endpoint) = UiBridge::new(4).split();
        let mut frontend = Frontend::new(endpoint);
        frontend.last_snapshot = Instant::now() - Duration::from_secs(4);
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| frontend.draw(ctx));
        assert!(frontend.snapshot.stale);
    }
}
