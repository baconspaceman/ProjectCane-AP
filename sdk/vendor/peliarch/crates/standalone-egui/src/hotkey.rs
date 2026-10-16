//! The click-through escape hatch.
//!
//! Click-through makes the window ignore the mouse, which means it also ignores the checkbox that
//! turned it on. Shipping that without a way back is shipping a window a player can lose, so the
//! toggle is armed by a system-wide hotkey registered outside the egui event loop:
//! **Ctrl+Shift+F10** sets a flag that the next frame reads and clears.
//!
//! It is a thread of its own because `RegisterHotKey` delivers `WM_HOTKEY` to the *registering*
//! thread's message queue, and this crate does not own the queue winit runs on. The thread holds
//! no window handle, no client state and no lock the renderer waits on; it flips one atomic.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;

/// Set when the hotkey fires. The UI thread takes the flag and turns click-through off.
#[derive(Clone, Default)]
pub struct Escape(Arc<AtomicBool>);

impl Escape {
    /// Consumes a pending press, if there is one.
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

/// Registers the hotkey and returns the flag it sets.
///
/// A failed registration -- most often another application already owns Ctrl+Shift+F10 -- returns
/// `None` rather than an error: it is a reason to disable the click-through toggle, not a reason to
/// fail a client launch.
pub fn register() -> Option<Escape> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOD_CONTROL, MOD_SHIFT, RegisterHotKey, VK_F10,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    const HOTKEY_ID: i32 = 0x4243;

    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let escape = Escape::default();
    let flag = Arc::clone(&escape.0);
    thread::spawn(move || {
        // SAFETY: the hotkey is registered against this thread's own message queue (a null window
        // handle), and every message read below is owned by that queue.
        let registered = unsafe {
            RegisterHotKey(
                None,
                HOTKEY_ID,
                MOD_CONTROL | MOD_SHIFT,
                u32::from(VK_F10.0),
            )
        }
        .is_ok();
        let _ = ready_tx.send(registered);
        if !registered {
            return;
        }
        let mut message = MSG::default();
        // SAFETY: `GetMessageW` blocks this thread on its own queue and writes into `message`.
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            if message.message == WM_HOTKEY && message.wParam.0 as i32 == HOTKEY_ID {
                flag.store(true, Ordering::Relaxed);
            }
        }
    });

    ready_rx.recv().ok().and_then(|ok| ok.then_some(escape))
}

/// One docked-tracker key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackerKey {
    /// F6: Hidden -> Regions -> In logic.
    Cycle,
    /// F7: previous page of the in-logic list.
    PageUp,
    /// F8: next page.
    PageDown,
}

const TRACKER_KEYS: [(TrackerKey, u8); 3] = [
    (TrackerKey::Cycle, 1 << 0),
    (TrackerKey::PageUp, 1 << 1),
    (TrackerKey::PageDown, 1 << 2),
];

/// F6/F7/F8 for the docked tracker (bb-archipelago SPEC-in-game-tracker, section 1).
///
/// Unlike the escape hatch these are bare function keys, so they are registered only while
/// `arm(true)` says the docked game window is in front, and unregistered the moment it is not:
/// a system-wide F6 would otherwise be taken from every other application. Presses arrive as bits
/// the UI thread takes once per frame.
#[derive(Clone, Default)]
pub struct TrackerKeys {
    armed: Arc<AtomicBool>,
    pressed: Arc<AtomicU8>,
}

impl TrackerKeys {
    pub fn arm(&self, armed: bool) {
        self.armed.store(armed, Ordering::Relaxed);
    }

    /// Consumes every press since the last call, in F6, F7, F8 order.
    pub fn take(&self) -> Vec<TrackerKey> {
        let bits = self.pressed.swap(0, Ordering::Relaxed);
        TRACKER_KEYS
            .iter()
            .filter(|(_, bit)| bits & bit != 0)
            .map(|(key, _)| *key)
            .collect()
    }

    /// Starts the registering thread. It polls the armed flag every 30 ms; a key another
    /// application already owns stays unregistered until the next arm.
    pub fn spawn() -> Self {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_F6, VK_F7, VK_F8,
        };
        use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW, WM_HOTKEY};

        const FIRST_ID: i32 = 0x4250;
        let keys = Self::default();
        let armed = Arc::clone(&keys.armed);
        let pressed = Arc::clone(&keys.pressed);
        thread::spawn(move || {
            let codes = [VK_F6, VK_F7, VK_F8];
            let mut registered = false;
            loop {
                let want = armed.load(Ordering::Relaxed);
                if want != registered {
                    for (index, code) in codes.iter().enumerate() {
                        let id = FIRST_ID + index as i32;
                        // SAFETY: registered against this thread's own queue (null window), and
                        // only ever unregistered by this same thread.
                        unsafe {
                            if want {
                                let _ = RegisterHotKey(None, id, MOD_NOREPEAT, u32::from(code.0));
                            } else {
                                let _ = UnregisterHotKey(None, id);
                            }
                        }
                    }
                    registered = want;
                }
                let mut message = MSG::default();
                // SAFETY: drains this thread's own queue into `message` without blocking.
                while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                    if message.message == WM_HOTKEY {
                        let index = message.wParam.0 as i32 - FIRST_ID;
                        if let Some((_, bit)) = usize::try_from(index)
                            .ok()
                            .and_then(|index| TRACKER_KEYS.get(index))
                        {
                            pressed.fetch_or(*bit, Ordering::Relaxed);
                        }
                    }
                }
                thread::sleep(std::time::Duration::from_millis(30));
            }
        });
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_presses_are_taken_once_in_key_order() {
        let keys = TrackerKeys::default();
        assert!(keys.take().is_empty());
        keys.pressed.fetch_or(0b101, Ordering::Relaxed);
        assert_eq!(vec![TrackerKey::Cycle, TrackerKey::PageDown], keys.take());
        assert!(keys.take().is_empty());
    }

    #[test]
    fn an_escape_press_is_consumed_exactly_once() {
        let escape = Escape::default();
        assert!(!escape.take(), "no press, no toggle");
        escape.0.store(true, Ordering::Relaxed);
        assert!(escape.take());
        assert!(!escape.take(), "a single press must not toggle twice");
    }
}
