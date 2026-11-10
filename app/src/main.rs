//! Ferrite — a desktop web browser written in Rust on top of Chromium (CEF).
//!
//! The same executable runs every Chromium process. Child processes (renderer,
//! GPU, utility, ...) return from `execute_process`; only the browser process
//! continues to initialise the UI.

mod browser;
mod engine;

use cef::*;
use ferrite_core::{profile::ProfilePaths, settings::Settings};
use std::sync::OnceLock;

/// Options from Ferrite's own command line, read once in the browser process.
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    pub profile: String,
    pub private: bool,
    pub urls: Vec<String>,
    pub no_sandbox: bool,
}

pub static LAUNCH: OnceLock<LaunchOptions> = OnceLock::new();

fn parse_launch_options() -> Result<LaunchOptions, String> {
    let mut opts = LaunchOptions {
        profile: "default".into(),
        ..Default::default()
    };
    for arg in std::env::args().skip(1) {
        if let Some(name) = arg.strip_prefix("--profile=") {
            opts.profile = name.to_string();
        } else if arg == "--private" || arg == "--incognito" {
            opts.private = true;
        } else if arg == "--no-sandbox" {
            opts.no_sandbox = true;
        } else if arg == "--help" || arg == "-h" {
            return Err(String::new());
        } else if !arg.starts_with('-') {
            opts.urls.push(arg);
        }
        // Other `--switches` are Chromium switches; CEF reads them itself.
    }
    Ok(opts)
}

const HELP: &str = "\
Usage: ferrite [OPTIONS] [URL...]

Options:
  --profile=NAME   Use the profile NAME (default: \"default\")
  --private        Open a private window (nothing is stored on disk)
  --no-sandbox     Disable Chromium's sandbox. Only for environments where it
                   cannot work (for example running as root in a container).
  -h, --help       Show this help

Any other --switch is passed to Chromium.
Environment: FERRITE_DATA_DIR (profile base directory), RUST_LOG (log filter),
FERRITE_UI_DIR (serve the UI files from this directory instead of the
copies built into the binary; for UI development).";

/// "Clear site storage" (settings page) is applied here, before Chromium
/// opens the profile: it removes per-site storage (local/session storage,
/// IndexedDB, service workers, cache storage, file systems).
fn clear_site_data_if_requested(paths: &ProfilePaths) {
    let flag = paths.root.join("clear-site-data");
    if !flag.exists() {
        return;
    }
    let profile = paths.cef_cache.join("Default");
    for dir in [
        "Local Storage",
        "Session Storage",
        "IndexedDB",
        "Service Worker",
        "File System",
        "WebStorage",
        "databases",
        "shared_proto_db",
    ] {
        let path = profile.join(dir);
        if path.exists() {
            if let Err(err) = std::fs::remove_dir_all(&path) {
                tracing::warn!("could not remove {}: {err}", path.display());
            }
        }
    }
    let _ = std::fs::remove_file(flag);
    tracing::info!("cleared site storage");
}

fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

fn main() {
    // Must happen before any other CEF call.
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = args::Args::new();
    let mut app = engine::app::FerriteApp::new();

    // Child processes run here and exit.
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 {
        std::process::exit(code);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let launch = match parse_launch_options() {
        Ok(opts) => opts,
        Err(_) => {
            println!("{HELP}");
            return;
        }
    };

    if is_root() && !launch.no_sandbox {
        eprintln!(
            "ferrite: refusing to run as root with the sandbox enabled (Chromium does not \
             support it). Run as a normal user, or pass --no-sandbox if you accept the risk."
        );
        std::process::exit(1);
    }
    if launch.no_sandbox {
        tracing::warn!("Chromium sandbox DISABLED (--no-sandbox). Web content is less isolated.");
    }

    let paths = match ProfilePaths::for_profile(&launch.profile) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("ferrite: invalid profile: {err:#}");
            std::process::exit(2);
        }
    };
    if let Err(err) = paths.ensure_dirs() {
        eprintln!("ferrite: cannot create profile directory: {err:#}");
        std::process::exit(2);
    }
    clear_site_data_if_requested(&paths);
    let settings = Settings::load(&paths.settings);
    let _ = engine::app::BROWSER_SWITCHES.set(browser::chromium_switches(&settings));

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();

    let cef_settings = cef::Settings {
        no_sandbox: launch.no_sandbox as i32,
        root_cache_path: CefString::from(paths.cef_cache.to_string_lossy().as_ref()),
        cache_path: CefString::from(paths.cef_cache.to_string_lossy().as_ref()),
        persist_session_cookies: 1,
        log_file: CefString::from(paths.root.join("chromium.log").to_string_lossy().as_ref()),
        log_severity: LogSeverity::WARNING,
        resources_dir_path: CefString::from(exe_dir.to_string_lossy().as_ref()),
        locales_dir_path: CefString::from(exe_dir.join("locales").to_string_lossy().as_ref()),
        background_color: browser::theme::window_background(&settings),
        ..Default::default()
    };

    browser::preload(paths, settings, launch.clone());
    let _ = LAUNCH.set(launch);

    if initialize(
        Some(args.as_main_args()),
        Some(&cef_settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        // CEF exits early when another instance already owns this profile;
        // it forwards our command line to it (on_already_running_app_relaunch).
        tracing::info!("CEF did not initialise (another instance may own this profile)");
        std::process::exit(0);
    }

    run_message_loop();
    browser::before_shutdown();
    shutdown();
}
