//! Running closures on CEF threads.
//!
//! CEF objects are reference counted with thread-safe counters, but the Rust
//! wrappers are not marked `Send`. [`SendBox`] carries them across threads in
//! the few places where CEF hands us an object on the IO thread that must be
//! used from the UI thread (or the other way round).

use cef::*;
use std::sync::{Arc, Mutex};

/// Asserts that the wrapped value may be moved to another thread.
///
/// Only used for CEF reference-counted objects (which are thread safe) and
/// for closures that are executed on the CEF UI thread they were created on.
pub struct SendBox<T>(T);

// SAFETY: see the type documentation; every use site moves CEF ref-counted
// handles (thread safe by design) or closures that only run on the UI thread.
unsafe impl<T> Send for SendBox<T> {}

impl<T> SendBox<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
    pub fn into_inner(self) -> T {
        self.0
    }
}

type Job = Box<dyn FnOnce() + Send>;

wrap_task! {
    struct FnTask {
        job: Arc<Mutex<Option<Job>>>,
    }

    impl Task {
        fn execute(&self) {
            let job = self.job.lock().ok().and_then(|mut job| job.take());
            if let Some(job) = job {
                job();
            }
        }
    }
}

fn make_task(job: Job) -> Task {
    FnTask::new(Arc::new(Mutex::new(Some(job))))
}

/// Run `f` on the CEF UI thread (later, never re-entrantly).
pub fn post_ui(f: impl FnOnce() + Send + 'static) {
    let mut task = make_task(Box::new(f));
    post_task(ThreadId::UI, Some(&mut task));
}

/// Run `f` on the UI thread after `delay_ms`.
pub fn post_ui_delayed(delay_ms: i64, f: impl FnOnce() + Send + 'static) {
    let mut task = make_task(Box::new(f));
    post_delayed_task(ThreadId::UI, Some(&mut task), delay_ms);
}

/// Like [`post_ui`] for closures that are not `Send`. Must be called on the
/// UI thread.
pub fn post_local(f: impl FnOnce() + 'static) {
    debug_assert!(on_ui_thread());
    let boxed = SendBox::new(f);
    post_ui(move || {
        let f = boxed.into_inner();
        f()
    });
}

/// Like [`post_ui_delayed`] for closures that are not `Send`. Must be called
/// on the UI thread.
pub fn post_local_delayed(delay_ms: i64, f: impl FnOnce() + 'static) {
    debug_assert!(on_ui_thread());
    let boxed = SendBox::new(f);
    post_ui_delayed(delay_ms, move || {
        let f = boxed.into_inner();
        f()
    });
}

pub fn on_ui_thread() -> bool {
    currently_on(ThreadId::UI) != 0
}
