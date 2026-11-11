//! JNI entry points for `se.ferrite.browser.adblock.Native`. Every entry point
//! catches panics (unwinding across the JNI boundary is undefined behaviour)
//! and returns a neutral value instead.
//!
//! The handle is a leaked `Box<Blocker>`; Kotlin guards `free` against
//! concurrent use with a read/write lock.

use crate::Blocker;
use jni::objects::{JBooleanArray, JClass, JObjectArray, JString};
use jni::sys::{jboolean, jlong, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn string(env: &mut JNIEnv, s: &JString) -> Option<String> {
    if s.is_null() {
        return None;
    }
    env.get_string(s).ok().map(Into::into)
}

fn build(
    env: &mut JNIEnv,
    lists: &JObjectArray,
    trusted: &JBooleanArray,
    resources: &JString,
) -> Option<Blocker> {
    let n = env.get_array_length(lists).ok()? as usize;
    let mut flags = vec![0u8; n];
    env.get_boolean_array_region(trusted, 0, &mut flags).ok()?;
    let mut texts = Vec::with_capacity(n);
    for (i, flag) in flags.iter().enumerate() {
        let obj = env.get_object_array_element(lists, i as i32).ok()?;
        let text = string(env, &JString::from(obj))?;
        texts.push((text, *flag != 0));
    }
    let json = string(env, resources);
    Some(Blocker::new(&texts, json.as_deref()))
}

#[no_mangle]
pub extern "system" fn Java_se_ferrite_browser_adblock_Native_create(
    mut env: JNIEnv,
    _class: JClass,
    lists: JObjectArray,
    trusted: JBooleanArray,
    resources: JString,
) -> jlong {
    catch_unwind(AssertUnwindSafe(|| {
        build(&mut env, &lists, &trusted, &resources)
            .map(|b| Box::into_raw(Box::new(b)) as jlong)
            .unwrap_or(0)
    }))
    .unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_se_ferrite_browser_adblock_Native_free(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        // SAFETY: the handle came from `create` and Kotlin frees it once.
        let _ = catch_unwind(AssertUnwindSafe(|| {
            drop(unsafe { Box::from_raw(handle as *mut Blocker) })
        }));
    }
}

#[no_mangle]
pub extern "system" fn Java_se_ferrite_browser_adblock_Native_shouldBlock(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    url: JString,
    source_url: JString,
    request_type: JString,
) -> jboolean {
    let blocked = catch_unwind(AssertUnwindSafe(|| {
        if handle == 0 {
            return false;
        }
        // SAFETY: valid while Kotlin holds its read lock.
        let blocker = unsafe { &*(handle as *const Blocker) };
        let (Some(u), Some(s), Some(t)) = (
            string(&mut env, &url),
            string(&mut env, &source_url),
            string(&mut env, &request_type),
        ) else {
            return false;
        };
        blocker.should_block(&u, &s, &t)
    }))
    .unwrap_or(false);
    if blocked {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

#[no_mangle]
pub extern "system" fn Java_se_ferrite_browser_adblock_Native_cosmetic(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    url: JString,
) -> jstring {
    let script = catch_unwind(AssertUnwindSafe(|| {
        if handle == 0 {
            return None;
        }
        // SAFETY: valid while Kotlin holds its read lock.
        let blocker = unsafe { &*(handle as *const Blocker) };
        blocker.cosmetic(&string(&mut env, &url)?)
    }))
    .unwrap_or(None);
    match script.and_then(|s| env.new_string(s).ok()) {
        Some(s) => s.into_raw(),
        None => std::ptr::null_mut(),
    }
}
