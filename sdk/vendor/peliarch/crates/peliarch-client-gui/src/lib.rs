//! A generic native frontend. Only the worker owns adapters, transport and persistence.
use peliarch_client_core::ui::{ClientSnapshot, HostEndpoint, UiAction, UiBridge};
use std::{sync::mpsc::TryRecvError, thread, time::Duration};

pub mod observer;
mod render;
pub mod view;
mod worker;
pub use render::Frontend;
pub use worker::RunnerWorker;

/// Adapter/bootstrap implementations keep all blocking work off the render thread.
pub trait Worker: Send + 'static {
    fn action(&mut self, action: UiAction) -> Result<(), String>;
    fn tick(&mut self) -> ClientSnapshot;
}

/// Returns a bounded action bridge and the worker's join handle. Dropping the
/// frontend stops the worker; render latency never delays a delivery receipt.
pub fn spawn_worker<W: Worker>(mut worker: W) -> (HostEndpoint, thread::JoinHandle<()>) {
    let (client, host) = UiBridge::new(32).split();
    let handle = thread::spawn(move || {
        let mut error = None;
        loop {
            loop {
                match client.try_action() {
                    Ok(UiAction::RequestShutdown) | Err(TryRecvError::Disconnected) => return,
                    Ok(action) => {
                        error = worker.action(action).err();
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            let mut snapshot = worker.tick();
            if let Some(reason) = &error {
                // The worker owns activity sequence numbers. Surface action refusal
                // without manufacturing a row that could collide with its next event.
                snapshot.delivery_detail = Some(format!("Action refused: {reason}"));
            }
            client.publish(snapshot);
            thread::sleep(Duration::from_millis(50));
        }
    });
    (host, handle)
}

#[cfg(windows)]
pub fn run(title: &str, endpoint: HostEndpoint) -> Result<(), String> {
    eframe::run_native(
        title,
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([860.0, 680.0])
                .with_min_inner_size([600.0, 420.0]),
            event_loop_builder: Some(Box::new(|builder| {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            })),
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Frontend::new(endpoint)))
        }),
    )
    .map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn spawn(title: String, endpoint: HostEndpoint) -> thread::JoinHandle<Result<(), String>> {
    thread::spawn(move || run(&title, endpoint))
}

#[cfg(windows)]
pub fn run_runner<G, S>(
    title: &str,
    runner: peliarch_client_core::runner::Runner<G>,
    game: G,
    store: S,
) -> Result<(), String>
where
    G: peliarch_client_core::runner::GameIntegration + Send + 'static,
    G::Payload: Send,
    S: peliarch_client_core::runner::RunnerPersistence<G::Payload> + Send + 'static,
{
    let (endpoint, handle) = spawn_worker(RunnerWorker::new(runner, game, store));
    let result = run(title, endpoint);
    handle
        .join()
        .map_err(|_| "Client worker panicked".to_owned())?;
    result
}

#[cfg(windows)]
impl eframe::App for Frontend {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.draw(ctx);
    }
}

#[cfg(test)]
mod worker_bridge_tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Instant,
    };
    struct Fixture(Arc<AtomicUsize>);
    impl Worker for Fixture {
        fn action(&mut self, _: UiAction) -> Result<(), String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err("fixture refused".into())
        }
        fn tick(&mut self) -> ClientSnapshot {
            ClientSnapshot::default()
        }
    }
    #[test]
    fn worker_actions_are_processed_off_render_thread_and_shutdown_joins() {
        let count = Arc::new(AtomicUsize::new(0));
        let (endpoint, handle) = spawn_worker(Fixture(count.clone()));
        endpoint.send_action(UiAction::Disconnect).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(snapshot) = endpoint.latest_snapshot()
                && snapshot.delivery_detail.as_deref() == Some("Action refused: fixture refused")
            {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
        endpoint.send_action(UiAction::RequestShutdown).unwrap();
        handle.join().unwrap();
    }
    #[test]
    fn closing_full_action_queue_still_stops_worker_when_frontend_drops() {
        struct Paused {
            started: std::sync::mpsc::SyncSender<()>,
            resume: std::sync::mpsc::Receiver<()>,
            paused: bool,
        }
        impl Worker for Paused {
            fn action(&mut self, _: UiAction) -> Result<(), String> {
                Ok(())
            }
            fn tick(&mut self) -> ClientSnapshot {
                if !self.paused {
                    self.paused = true;
                    self.started.send(()).unwrap();
                    self.resume.recv().unwrap();
                }
                ClientSnapshot::default()
            }
        }
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let (endpoint, handle) = spawn_worker(Paused {
            started: started_tx,
            resume: resume_rx,
            paused: false,
        });
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        for _ in 0..32 {
            endpoint.send_action(UiAction::Disconnect).unwrap();
        }
        assert!(matches!(
            endpoint.send_action(UiAction::RequestShutdown),
            Err(std::sync::mpsc::TrySendError::Full(_))
        ));
        drop(endpoint);
        resume_tx.send(()).unwrap();
        handle.join().unwrap();
    }
}
