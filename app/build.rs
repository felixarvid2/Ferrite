fn main() {
    // libcef.so and its resources are copied next to the executable by the
    // cef-dll-sys build script (and by scripts/bundle.sh for dist builds), so
    // look for shared libraries in the executable's own directory.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../ui");
}
