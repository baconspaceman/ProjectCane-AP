//! Docked tracker placement (bb-archipelago `docs/SPEC-in-game-tracker.md`, section 4).
//!
//! With docking on, the client window covers a strip of the shadPS4 game image and ignores the
//! mouse. Everything here is pure: the renderer reads the game window's client rectangle and
//! focus state through Win32, and these functions turn that into where the window goes and
//! whether it draws anything. Hiding is done by drawing nothing rather than by hiding the
//! window, because showing a hidden window can take focus from the game.

use serde::{Deserialize, Serialize};

use crate::WindowGeometry;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockSide {
    Left,
    #[default]
    Right,
}

/// Which part of the game's height the strip covers. Bloodborne's HUD sits top-left (health,
/// stamina) and bottom-right (equipment), so `Right` + `Top` or `Left` + `Bottom` keeps it clear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockVertical {
    #[default]
    Full,
    /// The upper 60%.
    Top,
    /// The lower 60%.
    Bottom,
}

pub const DOCK_WIDTH_MIN_PCT: u8 = 15;
pub const DOCK_WIDTH_MAX_PCT: u8 = 50;
const DOCK_WIDTH_DEFAULT_PCT: u8 = 30;
const PARTIAL_HEIGHT_PCT: i32 = 60;

/// Persisted with the other window options. Every field defaults, so a state file written before
/// docking existed restores with docking off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockOptions {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub side: DockSide,
    /// Share of the game's client width, 15-50.
    #[serde(default = "default_width_pct")]
    pub width_pct: u8,
    #[serde(default)]
    pub vertical: DockVertical,
}

const fn default_width_pct() -> u8 {
    DOCK_WIDTH_DEFAULT_PCT
}

impl Default for DockOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            side: DockSide::default(),
            width_pct: DOCK_WIDTH_DEFAULT_PCT,
            vertical: DockVertical::default(),
        }
    }
}

impl DockOptions {
    pub fn normalized(mut self) -> Self {
        self.width_pct = self.width_pct.clamp(DOCK_WIDTH_MIN_PCT, DOCK_WIDTH_MAX_PCT);
        self
    }

    /// The strip of `game` (its client area, in screen pixels) the docked window covers.
    pub fn rect(self, game: WindowGeometry) -> WindowGeometry {
        let options = self.normalized();
        let width = (game.width * i32::from(options.width_pct) / 100).max(1);
        let partial = (game.height * PARTIAL_HEIGHT_PCT / 100).max(1);
        let (y, height) = match options.vertical {
            DockVertical::Full => (game.y, game.height),
            DockVertical::Top => (game.y, partial),
            DockVertical::Bottom => (game.y + game.height - partial, partial),
        };
        let x = match options.side {
            DockSide::Left => game.x,
            DockSide::Right => game.x + game.width - width,
        };
        WindowGeometry {
            x,
            y,
            width,
            height,
        }
    }
}

/// What F6 cycles through while docked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackerMode {
    #[default]
    Hidden,
    Regions,
    InLogic,
}

impl TrackerMode {
    pub fn next(self) -> Self {
        match self {
            Self::Hidden => Self::Regions,
            Self::Regions => Self::InLogic,
            Self::InLogic => Self::Hidden,
        }
    }
}

/// The game window as last observed. `client` is `None` when no shadPS4 window was found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameWindow {
    pub client: Option<WindowGeometry>,
    pub minimized: bool,
    pub foreground: bool,
}

/// Where the docked window goes this tick, and whether it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockPlacement {
    /// `None` leaves the window where it is: there is no game window to follow.
    pub rect: Option<WindowGeometry>,
    /// False draws nothing. The window stays mapped, transparent and click-through.
    pub visible: bool,
}

/// The follower's whole rule. Visible only with the game found, not minimized, in front, and the
/// tracker not toggled off; positioned over the game whenever the game is found, so a later F6 or
/// alt-tab back shows it in the right place at once.
pub fn dock_placement(options: DockOptions, mode: TrackerMode, game: GameWindow) -> DockPlacement {
    let rect = game
        .client
        .filter(|client| client.width > 0 && client.height > 0 && !game.minimized)
        .map(|client| options.rect(client));
    DockPlacement {
        rect,
        visible: rect.is_some() && game.foreground && mode != TrackerMode::Hidden,
    }
}

/// Read the shadPS4 game window for `pid`: its client area in screen pixels, whether it is
/// minimized, and whether it is the foreground window. Read-only Win32 calls; nothing here
/// touches the game process.
///
/// shadPS4 can own two top-level windows, the Qt launcher and the SDL game window. The SDL
/// window is preferred by class name; failing that, the largest visible unowned window.
#[cfg(windows)]
pub fn probe_game_window(pid: u32) -> GameWindow {
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetClassNameW, GetClientRect, GetForegroundWindow, GetWindow,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    };
    use windows::core::BOOL;

    struct Search {
        pid: u32,
        found: Vec<(HWND, bool, i64)>,
    }

    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `&mut Search` passed to `EnumWindows` below, alive for the call.
        let search = unsafe { &mut *(lparam.0 as *mut Search) };
        let mut owner_pid = 0u32;
        // SAFETY: plain queries on a window handle the system just enumerated.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut owner_pid));
            if owner_pid != search.pid
                || !IsWindowVisible(hwnd).as_bool()
                || GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid())
            {
                return BOOL(1);
            }
            let mut class = [0u16; 64];
            let length = GetClassNameW(hwnd, &mut class).max(0) as usize;
            let sdl = String::from_utf16_lossy(&class[..length]).starts_with("SDL");
            let mut rect = RECT::default();
            let area = if GetClientRect(hwnd, &mut rect).is_ok() {
                i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top)
            } else {
                0
            };
            search.found.push((hwnd, sdl, area));
        }
        BOOL(1)
    }

    let mut search = Search {
        pid,
        found: Vec::new(),
    };
    // SAFETY: the callback only dereferences `lparam` as the `Search` that outlives this call.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
    let Some(&(hwnd, _, _)) = search
        .found
        .iter()
        .max_by_key(|(_, sdl, area)| (*sdl, *area))
    else {
        return GameWindow::default();
    };
    // SAFETY: read-only queries on a window handle found above; a handle that has since closed
    // makes the calls fail, which reads as "no client area".
    unsafe {
        let minimized = IsIconic(hwnd).as_bool();
        let foreground = GetForegroundWindow() == hwnd;
        let mut rect = RECT::default();
        let mut origin = POINT::default();
        let client = (GetClientRect(hwnd, &mut rect).is_ok()
            && ClientToScreen(hwnd, &mut origin).as_bool())
        .then(|| WindowGeometry {
            x: origin.x,
            y: origin.y,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        });
        GameWindow {
            client,
            minimized,
            foreground,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: WindowGeometry = WindowGeometry {
        x: 100,
        y: 50,
        width: 1920,
        height: 1080,
    };

    fn dock(side: DockSide, vertical: DockVertical, width_pct: u8) -> DockOptions {
        DockOptions {
            enabled: true,
            side,
            width_pct,
            vertical,
        }
    }

    #[test]
    fn every_side_and_band_maps_inside_the_game_image() {
        for game in [
            GAME,
            WindowGeometry {
                x: -1280,
                y: 0,
                width: 1280,
                height: 720,
            },
            WindowGeometry {
                x: 0,
                y: 0,
                width: 3840,
                height: 2160,
            },
        ] {
            for side in [DockSide::Left, DockSide::Right] {
                for vertical in [DockVertical::Full, DockVertical::Top, DockVertical::Bottom] {
                    for width_pct in [DOCK_WIDTH_MIN_PCT, 30, DOCK_WIDTH_MAX_PCT] {
                        let rect = dock(side, vertical, width_pct).rect(game);
                        assert!(rect.x >= game.x && rect.y >= game.y, "{rect:?} in {game:?}");
                        assert!(rect.x + rect.width <= game.x + game.width);
                        assert!(rect.y + rect.height <= game.y + game.height);
                        assert_eq!(game.width * i32::from(width_pct) / 100, rect.width);
                        match side {
                            DockSide::Left => assert_eq!(game.x, rect.x),
                            DockSide::Right => {
                                assert_eq!(game.x + game.width, rect.x + rect.width)
                            }
                        }
                        match vertical {
                            DockVertical::Full => assert_eq!(game.height, rect.height),
                            DockVertical::Top => assert_eq!(game.y, rect.y),
                            DockVertical::Bottom => {
                                assert_eq!(game.y + game.height, rect.y + rect.height)
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_default_strip_is_the_right_thirty_percent() {
        let rect = DockOptions::default().rect(GAME);
        assert_eq!(
            WindowGeometry {
                x: 100 + 1920 - 576,
                y: 50,
                width: 576,
                height: 1080
            },
            rect
        );
    }

    #[test]
    fn widths_outside_the_range_are_clamped() {
        assert_eq!(
            GAME.width * 15 / 100,
            dock(DockSide::Right, DockVertical::Full, 1)
                .rect(GAME)
                .width
        );
        assert_eq!(
            GAME.width / 2,
            dock(DockSide::Right, DockVertical::Full, 99)
                .rect(GAME)
                .width
        );
    }

    #[test]
    fn an_older_state_file_restores_with_docking_off() {
        let restored: DockOptions = json::from_str("{}").unwrap();
        assert_eq!(DockOptions::default(), restored);
        assert!(!restored.enabled);
        let partial: DockOptions = json::from_str(r#"{"enabled":true,"side":"left"}"#).unwrap();
        assert_eq!(DockSide::Left, partial.side);
        assert_eq!(30, partial.width_pct);
    }

    #[test]
    fn the_overlay_draws_only_over_a_focused_unminimized_game() {
        let options = DockOptions::default();
        let focused = GameWindow {
            client: Some(GAME),
            minimized: false,
            foreground: true,
        };
        let shown = dock_placement(options, TrackerMode::Regions, focused);
        assert!(shown.visible);
        assert_eq!(Some(options.rect(GAME)), shown.rect);

        let hidden = dock_placement(options, TrackerMode::Hidden, focused);
        assert!(!hidden.visible);
        assert_eq!(shown.rect, hidden.rect, "hidden still follows the game");

        let background = GameWindow {
            foreground: false,
            ..focused
        };
        assert!(!dock_placement(options, TrackerMode::InLogic, background).visible);

        let minimized = GameWindow {
            minimized: true,
            ..focused
        };
        assert_eq!(
            DockPlacement {
                rect: None,
                visible: false
            },
            dock_placement(options, TrackerMode::Regions, minimized)
        );
        assert_eq!(
            DockPlacement {
                rect: None,
                visible: false
            },
            dock_placement(options, TrackerMode::Regions, GameWindow::default())
        );
    }

    #[test]
    fn f6_cycles_hidden_regions_in_logic() {
        let mut mode = TrackerMode::default();
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(mode);
            mode = mode.next();
        }
        assert_eq!(
            vec![
                TrackerMode::Hidden,
                TrackerMode::Regions,
                TrackerMode::InLogic,
                TrackerMode::Hidden
            ],
            seen
        );
    }
}
