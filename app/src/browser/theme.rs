//! Light/dark theme plumbing for Chromium (the UI pages follow the settings
//! through CSS, see ui/common.css).

use super::State;
use cef::*;
use ferrite_core::settings::{Settings, Theme};

/// ARGB colour painted before content arrives.
pub fn window_background(settings: &Settings) -> u32 {
    match settings.theme {
        Theme::Light => 0xFFFF_FFFF,
        _ => 0xFF20_2124,
    }
}

/// Colour of the thin resize frame around frameless windows (matches the
/// tab strip in ui/chrome.css).
pub fn frame_color(settings: &Settings, private: bool) -> u32 {
    match (private, settings.theme) {
        (true, _) => 0xFF1A_1325,
        (false, Theme::Light) => 0xFFDD_E1E8,
        _ => 0xFF14_1518,
    }
}

/// Tell Chromium which colour scheme to use for its own UI (dialogs, menus,
/// internal chrome:// pages) in the given context (None = profile context).
pub fn apply_to_context(state: &State, ctx: Option<&RequestContext>) {
    let variant = match state.settings.theme {
        Theme::Dark => ColorVariant::DARK,
        Theme::Light => ColorVariant::LIGHT,
        Theme::System => ColorVariant::SYSTEM,
    };
    let ctx = match ctx {
        Some(ctx) => Some(ctx.clone()),
        None => request_context_get_global_context(),
    };
    if let Some(ctx) = ctx {
        ctx.set_chrome_color_scheme(variant, 0);
    }
}
