//! Icon constants for the UI
//!
//! Simple Unicode characters that render consistently across platforms.
//! These are from the basic Latin and Symbol ranges that are universally supported.

/// Search magnifying glass
pub const SEARCH: &str = "\u{1F50D}"; // 🔍 - fallback in case emoji works

/// Settings gear  
pub const SETTINGS: &str = "\u{2699}"; // ⚙

/// Close X
pub const CLOSE: &str = "\u{2715}"; // ✕

/// Clock/Timer
pub const CLOCK: &str = "\u{23F1}"; // ⏱

/// Storage/Save disk
pub const STORAGE: &str = "\u{1F4BE}"; // 💾

/// Globe/Server
pub const SERVER: &str = "\u{1F310}"; // 🌐

/// Refresh/Reload arrow
pub const REFRESH: &str = "\u{21BB}"; // ↻

/// Logout/Exit arrow
pub const LOGOUT: &str = "\u{2192}"; // →

/// Spinner/Loading
pub const SPINNER: &str = "\u{21BB}"; // ↻

/// Infinity symbol
pub const INFINITY: &str = "\u{221E}"; // ∞

/// Play triangle
pub const PLAY: &str = "\u{25B6}"; // ▶

// Alternative ASCII-safe versions for maximum compatibility
pub mod ascii {
    pub const SEARCH: &str = "Q";
    pub const SETTINGS: &str = "*";
    pub const CLOSE: &str = "X";
    pub const CLOCK: &str = "@";
    pub const STORAGE: &str = "#";
    pub const SERVER: &str = "O";
    pub const REFRESH: &str = "R";
    pub const LOGOUT: &str = ">";
    pub const SPINNER: &str = "*";
    pub const INFINITY: &str = "oo";
    pub const PLAY: &str = "|>";
}
