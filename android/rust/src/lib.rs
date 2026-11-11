//! Ferrite for Android: ad and tracker blocking with Brave's `adblock` engine.
//!
//! [`Blocker`] mirrors the desktop `ferrite-core` blocker (`should_block`,
//! `cosmetic`) and `app/src/browser/inject.rs` (`blocking_script`); the
//! [`jni`] module exposes it to Kotlin.

pub mod blocker;
mod jni_api;

pub use blocker::Blocker;
