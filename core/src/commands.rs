//! Browser commands and their keyboard shortcuts.
//!
//! Key codes are Windows virtual-key codes, which is what Chromium/CEF report
//! on every platform. Modifier bits follow CEF's `cef_event_flags_t`.

use serde::{Deserialize, Serialize};

pub const SHIFT: u32 = 1 << 1;
pub const CONTROL: u32 = 1 << 2;
pub const ALT: u32 = 1 << 3;
pub const COMMAND: u32 = 1 << 7;

/// The "primary" modifier: Cmd on macOS, Ctrl elsewhere.
pub const PRIMARY: u32 = if cfg!(target_os = "macos") {
    COMMAND
} else {
    CONTROL
};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    NewTab,
    NewWindow,
    NewPrivateWindow,
    CloseTab,
    CloseWindow,
    ReopenClosedTab,
    FocusAddressBar,
    Reload,
    HardReload,
    Stop,
    Back,
    Forward,
    Home,
    NextTab,
    PrevTab,
    /// Select tab by index (0-based); `SelectLastTab` for Ctrl+9.
    SelectTab(usize),
    SelectLastTab,
    Find,
    FindNext,
    FindPrev,
    BookmarkPage,
    ToggleBookmarksBar,
    Downloads,
    History,
    Bookmarks,
    Settings,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fullscreen,
    DevTools,
    ViewSource,
    Print,
    DuplicateTab,
    PinTab,
    Quit,
}

const VK_TAB: i32 = 0x09;
const VK_ESCAPE: i32 = 0x1B;
const VK_PRIOR: i32 = 0x21;
const VK_NEXT: i32 = 0x22;
const VK_HOME: i32 = 0x24;
const VK_LEFT: i32 = 0x25;
const VK_RIGHT: i32 = 0x27;
const VK_ADD: i32 = 0x6B;
const VK_SUBTRACT: i32 = 0x6D;
const VK_NUMPAD0: i32 = 0x60;
const VK_F3: i32 = 0x72;
const VK_F4: i32 = 0x73;
const VK_F5: i32 = 0x74;
const VK_F6: i32 = 0x75;
const VK_F11: i32 = 0x7A;
const VK_F12: i32 = 0x7B;
const VK_OEM_PLUS: i32 = 0xBB;
const VK_OEM_MINUS: i32 = 0xBD;

/// Map a key-down to a browser command. Only the modifiers we care about
/// (shift/ctrl/alt/cmd) are compared, so lock keys do not interfere.
pub fn match_shortcut(key_code: i32, modifiers: u32) -> Option<Command> {
    use Command::*;
    let m = modifiers & (SHIFT | CONTROL | ALT | COMMAND);
    let p = PRIMARY;
    let key = |c: char| c as i32;
    let cmd = match (m, key_code) {
        (x, k) if x == p && k == key('T') => NewTab,
        (x, k) if x == p && k == key('N') => NewWindow,
        (x, k) if x == p | SHIFT && k == key('N') => NewPrivateWindow,
        (x, k) if x == p && (k == key('W') || k == VK_F4) => CloseTab,
        (x, k) if x == p | SHIFT && k == key('W') => CloseWindow,
        (x, k) if x == p | SHIFT && k == key('T') => ReopenClosedTab,
        (x, k) if x == p && k == key('L') => FocusAddressBar,
        (ALT, k) if k == key('D') => FocusAddressBar,
        (0, VK_F6) => FocusAddressBar,
        (x, k) if x == p && k == key('R') => Reload,
        (0, VK_F5) => Reload,
        (x, k) if x == p | SHIFT && k == key('R') => HardReload,
        (SHIFT, VK_F5) => HardReload,
        (x, VK_F5) if x == p => HardReload,
        (ALT, VK_LEFT) => Back,
        (ALT, VK_RIGHT) => Forward,
        (ALT, VK_HOME) => Home,
        (x, VK_TAB) if x == CONTROL => NextTab,
        (x, VK_TAB) if x == CONTROL | SHIFT => PrevTab,
        (x, VK_NEXT) if x == p => NextTab,
        (x, VK_PRIOR) if x == p => PrevTab,
        (x, k) if x == p && (key('1')..=key('8')).contains(&k) => {
            SelectTab((k - key('1')) as usize)
        }
        (x, k) if x == p && k == key('9') => SelectLastTab,
        (x, k) if x == p && k == key('F') => Find,
        (x, k) if x == p && k == key('G') => FindNext,
        (0, VK_F3) => FindNext,
        (x, k) if x == p | SHIFT && k == key('G') => FindPrev,
        (SHIFT, VK_F3) => FindPrev,
        (x, k) if x == p && k == key('D') => BookmarkPage,
        (x, k) if x == p | SHIFT && k == key('B') => ToggleBookmarksBar,
        (x, k) if x == p | SHIFT && k == key('O') => Bookmarks,
        (x, k) if x == p && k == key('J') => Downloads,
        (x, k) if x == p && k == key('H') => History,
        (x, k) if (x == p || x == p | SHIFT) && (k == VK_OEM_PLUS || k == VK_ADD) => ZoomIn,
        (x, k) if x == p && (k == VK_OEM_MINUS || k == VK_SUBTRACT) => ZoomOut,
        (x, k) if x == p && (k == key('0') || k == VK_NUMPAD0) => ZoomReset,
        (0, VK_F11) => Fullscreen,
        (0, VK_F12) => DevTools,
        (x, k) if x == p | SHIFT && (k == key('I') || k == key('J')) => DevTools,
        (x, k) if x == p && k == key('U') => ViewSource,
        (x, k) if x == p && k == key('P') => Print,
        (x, k) if x == p | SHIFT && k == key('Q') => Quit,
        _ => return None,
    };
    Some(cmd)
}

/// Escape is only a browser command while the find bar is open.
pub fn is_escape(key_code: i32, modifiers: u32) -> bool {
    key_code == VK_ESCAPE && modifiers & (SHIFT | CONTROL | ALT | COMMAND) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(c: char) -> i32 {
        c as i32
    }

    #[test]
    fn required_shortcuts() {
        assert_eq!(match_shortcut(k('T'), PRIMARY), Some(Command::NewTab));
        assert_eq!(match_shortcut(k('W'), PRIMARY), Some(Command::CloseTab));
        assert_eq!(
            match_shortcut(k('T'), PRIMARY | SHIFT),
            Some(Command::ReopenClosedTab)
        );
        assert_eq!(
            match_shortcut(k('L'), PRIMARY),
            Some(Command::FocusAddressBar)
        );
        assert_eq!(match_shortcut(k('R'), PRIMARY), Some(Command::Reload));
        assert_eq!(match_shortcut(VK_TAB, CONTROL), Some(Command::NextTab));
        assert_eq!(
            match_shortcut(VK_TAB, CONTROL | SHIFT),
            Some(Command::PrevTab)
        );
        assert_eq!(match_shortcut(k('F'), PRIMARY), Some(Command::Find));
        assert_eq!(match_shortcut(k('D'), PRIMARY), Some(Command::BookmarkPage));
        assert_eq!(match_shortcut(k('J'), PRIMARY), Some(Command::Downloads));
        assert_eq!(match_shortcut(k('H'), PRIMARY), Some(Command::History));
        assert_eq!(match_shortcut(VK_OEM_PLUS, PRIMARY), Some(Command::ZoomIn));
        assert_eq!(
            match_shortcut(VK_OEM_PLUS, PRIMARY | SHIFT),
            Some(Command::ZoomIn)
        );
        assert_eq!(
            match_shortcut(VK_OEM_MINUS, PRIMARY),
            Some(Command::ZoomOut)
        );
        assert_eq!(match_shortcut(VK_F11, 0), Some(Command::Fullscreen));
        assert_eq!(match_shortcut(VK_F12, 0), Some(Command::DevTools));
    }

    #[test]
    fn tab_numbers_and_misc() {
        assert_eq!(match_shortcut(k('1'), PRIMARY), Some(Command::SelectTab(0)));
        assert_eq!(match_shortcut(k('8'), PRIMARY), Some(Command::SelectTab(7)));
        assert_eq!(
            match_shortcut(k('9'), PRIMARY),
            Some(Command::SelectLastTab)
        );
        assert_eq!(match_shortcut(VK_LEFT, ALT), Some(Command::Back));
    }

    #[test]
    fn plain_typing_is_not_a_shortcut() {
        assert_eq!(match_shortcut(k('T'), 0), None);
        assert_eq!(match_shortcut(k('T'), SHIFT), None);
        assert_eq!(match_shortcut(VK_TAB, 0), None);
        assert_eq!(match_shortcut(VK_LEFT, 0), None);
        // Lock-key bits (caps lock = 1) are ignored.
        assert_eq!(match_shortcut(k('T'), PRIMARY | 1), Some(Command::NewTab));
    }
}
