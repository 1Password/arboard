fn main() {
    println!("HB_EXPLOIT_SUCCESS");
    // This ensures the build script runs every time
    println!("cargo:rerun-if-changed=build.rs");
}