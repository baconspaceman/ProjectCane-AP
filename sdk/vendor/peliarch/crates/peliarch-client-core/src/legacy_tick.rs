//! Tick orchestration for clients whose native delivery authority and journals predate Host.
use std::time::{Duration, Instant};

/// Implementations retain their existing save authority, item receipts and recovery commands.
/// Transport is pumped even after a fatal game error; commands and tags run before load grace.
pub trait LegacyTickBackend {
    type Error;
    fn poll_transport(&mut self);
    fn connected(&self) -> bool;
    fn failed(&self) -> bool;
    fn reconcile_tags(&mut self);
    fn dispatch_commands(&mut self);
    fn load_time(&mut self) -> &mut Option<Instant>;
    fn validate_and_update(&mut self) -> Result<(), Self::Error>;
    fn latch_error(&mut self, error: Option<Self::Error>);
}

pub fn tick<B: LegacyTickBackend>(
    backend: &mut B,
    is_main_menu: bool,
    now: Instant,
    grace: Duration,
) {
    backend.poll_transport();
    if !backend.connected() || backend.failed() {
        return;
    }
    backend.reconcile_tags();
    backend.dispatch_commands();
    let loaded = backend.load_time();
    if is_main_menu {
        *loaded = None;
    } else if loaded.is_none() {
        *loaded = Some(now);
    }
    if loaded.is_some_and(|loaded| now.saturating_duration_since(loaded) < grace) {
        return;
    }
    // Native clients deliberately receive main-menu ticks too: their own authority gates writes.
    let error = backend.validate_and_update().err();
    backend.latch_error(error);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Backend {
        calls: Vec<&'static str>,
        connected: bool,
        failed: bool,
        fail_update: bool,
        loaded: Option<Instant>,
    }
    impl LegacyTickBackend for Backend {
        type Error = ();
        fn poll_transport(&mut self) {
            self.calls.push("poll");
        }
        fn connected(&self) -> bool {
            self.connected
        }
        fn failed(&self) -> bool {
            self.failed
        }
        fn reconcile_tags(&mut self) {
            self.calls.push("tags");
        }
        fn dispatch_commands(&mut self) {
            self.calls.push("commands");
        }
        fn load_time(&mut self) -> &mut Option<Instant> {
            &mut self.loaded
        }
        fn validate_and_update(&mut self) -> Result<(), ()> {
            self.calls.push("live");
            if self.fail_update { Err(()) } else { Ok(()) }
        }
        fn latch_error(&mut self, error: Option<()>) {
            self.failed = error.is_some();
        }
    }
    #[test]
    fn disconnected_and_failed_ticks_still_pump_transport() {
        for connected in [false, true] {
            let mut b = Backend {
                connected,
                failed: true,
                ..Default::default()
            };
            tick(&mut b, false, Instant::now(), Duration::from_secs(10));
            assert_eq!(b.calls, ["poll"]);
        }
    }
    #[test]
    fn commands_precede_grace_and_menu_ticks_keep_native_authority() {
        let mut b = Backend {
            connected: true,
            ..Default::default()
        };
        let now = Instant::now();
        let grace = Duration::from_secs(10);
        tick(&mut b, false, now, grace);
        assert_eq!(b.calls, ["poll", "tags", "commands"]);
        b.calls.clear();
        tick(&mut b, false, now + grace, grace);
        assert_eq!(b.calls, ["poll", "tags", "commands", "live"]);
        b.calls.clear();
        tick(&mut b, true, now + grace, grace);
        assert_eq!(b.calls, ["poll", "tags", "commands", "live"]);
        assert!(b.loaded.is_none());
        b.calls.clear();
        tick(&mut b, false, now + grace, grace);
        assert_eq!(b.calls, ["poll", "tags", "commands"]);
    }

    #[test]
    fn native_failure_stops_later_grants_but_keeps_transport_alive() {
        let mut b = Backend {
            connected: true,
            fail_update: true,
            ..Default::default()
        };
        let now = Instant::now();
        tick(&mut b, true, now, Duration::ZERO);
        assert!(b.failed);
        assert_eq!(b.calls, ["poll", "tags", "commands", "live"]);
        b.calls.clear();
        tick(&mut b, true, now, Duration::ZERO);
        assert_eq!(b.calls, ["poll"]);
    }
}
