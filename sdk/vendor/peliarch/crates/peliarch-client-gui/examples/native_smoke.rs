//! Real native-window render verification. Saves the framebuffer then closes.
#[cfg(windows)]
fn main() {
    use peliarch_client_core::ui::*;
    struct Smoke {
        frontend: peliarch_client_gui::Frontend,
        frames: usize,
    }
    impl eframe::App for Smoke {
        fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
            self.frontend.draw(ctx);
            self.frames += 1;
            if self.frames == 3 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(screenshot) = screenshot {
                let pixels: Vec<u8> = screenshot
                    .pixels
                    .iter()
                    .flat_map(|c| c.to_array())
                    .collect();
                let output = std::env::var("PELIARCH_SMOKE_OUTPUT")
                    .unwrap_or_else(|_| "peliarch-gui-smoke.png".into());
                image::save_buffer(
                    output,
                    &pixels,
                    screenshot.size[0] as u32,
                    screenshot.size[1] as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            assert!(
                self.frames < 300,
                "native renderer did not return a screenshot"
            );
        }
    }
    let (client, endpoint) = UiBridge::new(4).split();
    let mut snapshot = ClientSnapshot {
        ap: ApState::Authenticated,
        delivery: DeliveryState::Ready,
        server: Some("ws://localhost:38281".into()),
        slot: Some("Synthetic fixture".into()),
        seed: Some("Render verification seed".into()),
        save_identity: Some("synthetic-save".into()),
        gameplay_ready: true,
        goal: Some("Finish the fixture adventure".into()),
        locations: Some(LocationTotals {
            checked: 3,
            total: 8,
        }),
        ledger: LedgerTotals {
            queued: 1,
            delivered: 2,
            ..Default::default()
        },
        tracker: Some(TrackerView {
            regions: vec![
                TrackerRegion {
                    name: "Fixture valley".into(),
                    checked: 3,
                    total: 5,
                    in_logic: vec!["Ancient track".into(), "Bridge balloon".into()],
                    ..Default::default()
                },
                TrackerRegion {
                    name: "Fixture mountain".into(),
                    total: 3,
                    needs: Some("Mountain key".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        unchecked_locations: vec![UncheckedLocation {
            name: "Fixture checkpoint".into(),
            region: Some("Synthetic region".into()),
        }],
        ..Default::default()
    };
    snapshot.push_activity(
        ActivityEvent::now(1, ActivityKind::Message, "Connected to synthetic fixture"),
        10,
    );
    snapshot.push_activity(
        ActivityEvent::now(
            2,
            ActivityKind::ReceivedItem,
            "Received fixture progression effect",
        ),
        10,
    );
    snapshot.push_activity(
        ActivityEvent::now(
            3,
            ActivityKind::Message,
            "Synthetic tracker fixture: counts, reachability and requirements supplied by worker",
        ),
        10,
    );
    client.publish(snapshot);
    eframe::run_native(
        "Peliarch native render verification",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1060.0, 780.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Smoke {
                frontend: peliarch_client_gui::Frontend::new(endpoint),
                frames: 0,
            }))
        }),
    )
    .unwrap();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Native smoke is Windows-only; portable egui tessellation is covered by tests.");
}
