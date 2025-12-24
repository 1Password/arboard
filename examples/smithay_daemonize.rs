// Example showing how to initialize the smithay backend and use `wait()` semantics.
// NOTE: In a real Wayland GUI app you should obtain a real `*mut wl_display` pointer from your
// toolkit (for example: `gdk_wayland_display_get_wl_display` or `RawDisplayHandle::Wayland`).
// This example compiles as-is but does **not** actually connect to Wayland.

use arboard::SetExtLinux;

fn main() -> Result<(), arboard::Error> {
    // SAFETY: you must pass a real wl_display pointer from your GUI toolkit or raw window handle.
    #[cfg(all(feature = "smithay-clipboard", unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
    unsafe {
        // Replace with a real pointer in a real app.
        let wl_display: *mut std::ffi::c_void = std::ptr::null_mut();
        // Ignore the result in this example; init will error on a null pointer.
        let _ = arboard::init_wayland_display(wl_display);
    }

    let mut cb = arboard::Clipboard::new()?;

    // This will block/daemonize until the clipboard is overwritten due to `wait()` behavior.
    cb.set().wait().text("Hello smithay".to_owned())?;
    Ok(())
}
