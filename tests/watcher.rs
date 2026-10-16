use arboard::{Clipboard, ClipboardEvent, ClipboardWatcher};
use std::{
	io::Write,
	process::{Command, Stdio},
	time::Duration,
};

/// This test overwrites the regular clipboard. It requires a native desktop or test compositor.
/// Run explicitly with `cargo test --test watcher -- --ignored`.
#[test]
#[ignore = "requires a native clipboard and pbcopy, PowerShell, xclip, or wl-copy; overwrites clipboard"]
fn native_changes_and_external_writer() {
	let watcher = ClipboardWatcher::new().expect("native watcher must be available");
	assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Changed);
	let mut clipboard = Clipboard::new().unwrap();
	for text in ["watcher native first", "watcher native second"] {
		wait_for_quiet(&watcher);
		clipboard.set_text(text).unwrap();
		assert_eq!(
			watcher.wait_timeout(Duration::from_secs(5)).unwrap(),
			Some(ClipboardEvent::Changed)
		);
		assert_eq!(clipboard.get_text().unwrap(), text);
	}

	// Drain native notifications before the independent writer changes the same text format.
	wait_for_quiet(&watcher);
	external_copy("watcher external writer");
	assert_eq!(
		watcher.wait_timeout(Duration::from_secs(5)).unwrap(),
		Some(ClipboardEvent::Changed)
	);
	assert_eq!(clipboard.get_text().unwrap(), "watcher external writer");
	wait_for_quiet(&watcher);
	clipboard.clear().unwrap();
	assert_eq!(
		watcher.wait_timeout(Duration::from_secs(5)).unwrap(),
		Some(ClipboardEvent::Changed)
	);
	watcher.stop().unwrap();
	assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Stopped);
	drop(ClipboardWatcher::new().unwrap());
}

fn wait_for_quiet(watcher: &ClipboardWatcher) {
	for _ in 0..20 {
		if watcher.wait_timeout(Duration::from_millis(200)).unwrap().is_none() {
			return;
		}
	}
	panic!("the clipboard must become idle before the next test write");
}

fn external_copy(text: &str) {
	#[cfg(target_os = "macos")]
	let mut command = Command::new("pbcopy");
	#[cfg(windows)]
	let mut command = {
		let mut command = Command::new("powershell.exe");
		command.args([
			"-NoProfile",
			"-Command",
			"Set-Clipboard -Value ([Console]::In.ReadToEnd())",
		]);
		command
	};
	#[cfg(all(unix, not(target_os = "macos")))]
	let mut command = {
		if cfg!(feature = "wayland-data-control")
			&& (std::env::var_os("WAYLAND_DISPLAY").is_some()
				|| std::env::var_os("WAYLAND_SOCKET").is_some())
		{
			Command::new("wl-copy")
		} else {
			let mut command = Command::new("xclip");
			command.args(["-selection", "clipboard"]);
			command
		}
	};
	let mut child =
		command.stdin(Stdio::piped()).spawn().expect("external clipboard tool is required");
	child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
	assert!(child.wait().unwrap().success());
}
