#[no_mangle]
#[cfg(target_os = "android")]
fn android_main(app: android_activity::AndroidApp) {
	use android_activity::{MainEvent, PollEvent};
	use arboard::Clipboard;

	println!("app started");

	let mut quit = false;
	let mut redraw_pending = true;
	let mut native_window: Option<ndk::native_window::NativeWindow> = None;

	let text = "hello world";

	let mut ctx = Clipboard::new().unwrap();

	assert!(
		ctx.set_text(text).is_ok(),
		"We can write to the clipboard even if we don't have focus"
	);

	assert!(ctx.get_text().is_err(), "We can't read the clipboard if we don't have focus");

	while !quit {
		app.poll_events(Some(std::time::Duration::from_secs(1)) /* timeout */, |event| {
			match event {
				PollEvent::Wake => {}
				PollEvent::Timeout => {
					redraw_pending = true;
				}
				PollEvent::Main(main_event) => {
					match main_event {
						MainEvent::InitWindow { .. } => {
							native_window = app.native_window();
							redraw_pending = true;
						}
						MainEvent::TerminateWindow { .. } => {
							native_window = None;
						}
						MainEvent::WindowResized { .. } => {
							redraw_pending = true;
						}
						MainEvent::RedrawNeeded { .. } => {
							redraw_pending = true;
						}
						MainEvent::InputAvailable { .. } => {
							redraw_pending = true;
						}
						MainEvent::GainedFocus => {
							assert_eq!(
								ctx.get_text().unwrap(),
								text,
								"Since we have focus we can access the clipboard"
							);

							ctx.clear().unwrap();
							assert!(ctx.get_text().is_err());

							quit = true;
						}
						_ => { /* ... */ }
					}
				}
				_ => {}
			}

			if redraw_pending {
				if let Some(native_window) = &native_window {
					redraw_pending = false;

					dummy_render(native_window);
				}
			}
		});
	}
}

/// Post a NOP frame to the window
///
/// Since this is a bare minimum test app we don't depend
/// on any GPU graphics APIs but we do need to at least
/// convince Android that we're drawing something and are
/// responsive, otherwise it will stop delivering input
/// events to us.
#[cfg(target_os = "android")]
fn dummy_render(native_window: &ndk::native_window::NativeWindow) {
	unsafe {
		let mut buf: ndk_sys::ANativeWindow_Buffer = std::mem::zeroed();
		let mut rect: ndk_sys::ARect = std::mem::zeroed();
		ndk_sys::ANativeWindow_lock(
			native_window.ptr().as_ptr() as _,
			&mut buf as _,
			&mut rect as _,
		);
		// Note: we don't try and touch the buffer since that
		// also requires us to handle various buffer formats
		ndk_sys::ANativeWindow_unlockAndPost(native_window.ptr().as_ptr() as _);
	}
}
