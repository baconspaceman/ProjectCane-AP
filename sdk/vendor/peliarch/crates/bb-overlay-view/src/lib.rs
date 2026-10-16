//! `bb-tracker-view-v1`: what the in-game overlay draws (bb-archipelago
//! `docs/SPEC-in-game-tracker.md`, section 7.3).
//!
//! The tracker window lays the overlay out and writes it here, atomically; the Vulkan layer in
//! shadPS4 reads it and draws it, deciding nothing. A file nobody refreshes goes stale and the
//! layer stops drawing, so a crashed or closed client never leaves a frozen overlay over the game.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const VIEW_FORMAT: &str = "bb-tracker-view-v1";
/// Writers refresh at least this often while running.
pub const HEARTBEAT_MS: u64 = 1_000;
/// Readers draw nothing from a file older than this.
pub const STALE_AFTER_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    #[default]
    Text,
    Muted,
    Accent,
    Warn,
    Hint,
}

impl Tone {
    /// sRGB colour the layer clears glyph pixels with.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Text => [0xd4, 0xd0, 0xc8],
            Self::Muted => [0x8a, 0x86, 0x80],
            Self::Accent => [0x7a, 0xb8, 0xe8],
            Self::Warn => [0xdd, 0xb5, 0x4f],
            Self::Hint => [0xb0, 0x8c, 0xd8],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    #[serde(default)]
    pub tone: Tone,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    #[default]
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    #[default]
    Full,
    Top,
    Bottom,
}

/// Where on the game image the overlay sits, as shares of the image so a resize keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub side: Side,
    /// 15-50, share of the image width.
    pub width_pct: u8,
    pub band: Band,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            side: Side::Right,
            width_pct: 30,
            band: Band::Full,
        }
    }
}

impl Placement {
    /// The overlay rectangle `(x, y, width, height)` inside a `width` x `height` image.
    pub fn rect(self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let pct = u32::from(self.width_pct.clamp(15, 50));
        let strip = (width * pct / 100).max(1);
        let partial = (height * 60 / 100).max(1);
        let (y, h) = match self.band {
            Band::Full => (0, height),
            Band::Top => (0, partial),
            Band::Bottom => (height - partial, partial),
        };
        let x = match self.side {
            Side::Left => 0,
            Side::Right => width - strip,
        };
        (x, y, strip, h)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayView {
    pub format: String,
    pub updated_unix_ms: u64,
    /// False draws nothing: the player hid the tracker (F6, or the controller chord).
    pub visible: bool,
    #[serde(default)]
    pub placement: Placement,
    pub lines: Vec<Line>,
}

impl OverlayView {
    pub fn new(
        updated_unix_ms: u64,
        visible: bool,
        placement: Placement,
        lines: Vec<Line>,
    ) -> Self {
        Self {
            format: VIEW_FORMAT.to_owned(),
            updated_unix_ms,
            visible,
            placement,
            lines,
        }
    }

    /// Whether a reader should draw this view at `now_unix_ms`.
    pub fn drawable(&self, now_unix_ms: u64) -> bool {
        self.format == VIEW_FORMAT
            && self.visible
            && now_unix_ms.saturating_sub(self.updated_unix_ms) <= STALE_AFTER_MS
    }

    /// The same view with the timestamp ignored, to decide whether a rewrite is needed.
    pub fn same_content(&self, other: &Self) -> bool {
        self.visible == other.visible
            && self.placement == other.placement
            && self.lines == other.lines
    }
}

/// Write via a temporary file and rename, so the layer never reads half a view.
pub fn write(path: &Path, view: &OverlayView) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let bytes = json::to_vec(view)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}

/// Read a view; any failure is `None`, which a reader treats as "draw nothing".
pub fn read(path: &Path) -> Option<OverlayView> {
    let bytes = std::fs::read(path).ok()?;
    json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_round_trips_and_goes_stale() {
        let directory =
            std::env::temp_dir().join(format!("bb-overlay-view-{}", std::process::id()));
        let path = directory.join("tracker-view.json");
        let view = OverlayView::new(
            10_000,
            true,
            Placement::default(),
            vec![Line {
                text: "Cathedral Ward  28/33  * 5".into(),
                tone: Tone::Text,
            }],
        );
        write(&path, &view).unwrap();
        let read_back = read(&path).unwrap();
        assert_eq!(view, read_back);
        assert!(read_back.drawable(10_000 + STALE_AFTER_MS));
        assert!(
            !read_back.drawable(10_001 + STALE_AFTER_MS),
            "stale views are not drawn"
        );
        let hidden = OverlayView {
            visible: false,
            ..read_back.clone()
        };
        assert!(!hidden.drawable(10_000));
        assert!(read_back.same_content(&OverlayView {
            updated_unix_ms: 99,
            ..read_back.clone()
        }));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_missing_or_foreign_file_is_simply_nothing() {
        assert_eq!(None, read(Path::new("Z:/definitely/not/here.json")));
        let foreign: OverlayView =
            json::from_str(r#"{"format":"other","updated_unix_ms":1,"visible":true,"lines":[]}"#)
                .unwrap();
        assert!(!foreign.drawable(1));
    }

    #[test]
    fn placement_rects_stay_inside_the_image() {
        for side in [Side::Left, Side::Right] {
            for band in [Band::Full, Band::Top, Band::Bottom] {
                for width_pct in [1, 15, 30, 50, 90] {
                    let placement = Placement {
                        side,
                        width_pct,
                        band,
                    };
                    let (x, y, w, h) = placement.rect(1920, 1080);
                    assert!(x + w <= 1920 && y + h <= 1080, "{placement:?}");
                    assert!((1920 * 15 / 100..=1920 / 2).contains(&w));
                }
            }
        }
        assert_eq!((1344, 0, 576, 1080), Placement::default().rect(1920, 1080));
    }
}
