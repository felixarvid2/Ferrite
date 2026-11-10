//! The CEF `App`: runs in every process (browser, renderer, GPU, ...).

use super::{cef_str, SCHEME};
use cef::*;
use std::sync::OnceLock;

/// Extra Chromium switches decided in `main` before CEF starts (for example
/// the colour scheme). Only applied to the browser process; Chromium passes
/// what child processes need on to them itself.
pub static BROWSER_SWITCHES: OnceLock<Vec<(String, Option<String>)>> = OnceLock::new();

wrap_app! {
    pub struct FerriteApp;

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let is_browser = process_type.map(|p| p.to_string().is_empty()).unwrap_or(true);
            let (true, Some(command_line)) = (is_browser, command_line) else {
                return;
            };
            for (name, value) in BROWSER_SWITCHES.get().into_iter().flatten() {
                let name = cef_str(name);
                if command_line.has_switch(Some(&name)) != 0 {
                    continue;
                }
                match value {
                    Some(value) => command_line.append_switch_with_value(Some(&name), Some(&cef_str(value))),
                    None => command_line.append_switch(Some(&name)),
                }
            }
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            let Some(registrar) = registrar else { return };
            use sys::cef_scheme_options_t as o;
            // Standard + secure so internal pages get a real origin and may
            // load https favicons; not CSP-bypassing and not "local". Access
            // control is enforced by the scheme handler (see
            // browser::scheme) and by the navigation guard.
            let options = o::CEF_SCHEME_OPTION_STANDARD as i32
                | o::CEF_SCHEME_OPTION_SECURE as i32
                | o::CEF_SCHEME_OPTION_CORS_ENABLED as i32
                | o::CEF_SCHEME_OPTION_FETCH_ENABLED as i32;
            registrar.add_custom_scheme(Some(&cef_str(SCHEME)), options);
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(FerriteBrowserProcessHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct FerriteBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            crate::browser::start();
        }

        fn on_already_running_app_relaunch(
            &self,
            command_line: Option<&mut CommandLine>,
            _current_directory: Option<&CefString>,
        ) -> ::std::os::raw::c_int {
            let mut urls = Vec::new();
            let mut private = false;
            if let Some(command_line) = command_line {
                let mut args = CefStringList::new();
                command_line.arguments(Some(&mut args));
                urls = super::read_string_list(&mut args);
                private = command_line.has_switch(Some(&cef_str("private"))) != 0;
            }
            crate::browser::relaunch(urls, private);
            1
        }
    }
}
