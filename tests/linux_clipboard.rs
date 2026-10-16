#![cfg(target_os = "linux")]

use std::{
	io::Write,
	process::{Child, Command, Stdio},
	time::{Duration, Instant},
};

use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind, SetExtLinux};

fn external_read(mime: &str, primary: bool) -> Vec<u8> {
	let mut command = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
		let mut command = Command::new("wl-paste");
		command.args(["--no-newline", "--type", mime]);
		if primary {
			command.arg("--primary");
		}
		command
	} else {
		let mut command = Command::new("xclip");
		command.args([
			"-out",
			"-selection",
			if primary { "primary" } else { "clipboard" },
			"-target",
			if mime == "text/plain" { "UTF8_STRING" } else { mime },
		]);
		command
	};
	let output = command.output().expect("clipboard reader must be installed");
	assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
	output.stdout
}

struct ExternalOwner(Child);

impl ExternalOwner {
	fn text(text: &str) -> Self {
		let mut command = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
			let mut command = Command::new("wl-copy");
			command.args(["--foreground", "--type", "text/plain"]);
			command
		} else {
			let mut command = Command::new("xclip");
			command.args(["-quiet", "-in", "-selection", "clipboard", "-target", "UTF8_STRING"]);
			command
		};
		let mut child = command
			.stdin(Stdio::piped())
			.stdout(Stdio::null())
			.spawn()
			.expect("clipboard owner must be installed");
		child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
		Self(child)
	}
}

impl Drop for ExternalOwner {
	fn drop(&mut self) {
		let _ = self.0.kill();
		let _ = self.0.wait();
	}
}

#[test]
fn exchanges_data_with_native_clients() {
	let mut clipboard = Clipboard::new().expect("a real clipboard backend is required");
	let text = "Clipboard text: \u{03bb}\nsecond line";
	clipboard.set_text(text).unwrap();
	assert_eq!(external_read("text/plain", false), text.as_bytes());

	let html = "<strong>clipboard</strong>";
	clipboard.set_html(html, Some("clipboard")).unwrap();
	assert_eq!(external_read("text/html", false), html.as_bytes());
	assert_eq!(external_read("text/plain", false), b"clipboard");

	clipboard.set().clipboard(LinuxClipboardKind::Primary).text("primary selection").unwrap();
	assert_eq!(external_read("text/plain", true), b"primary selection");
	assert_eq!(
		clipboard.get().clipboard(LinuxClipboardKind::Primary).text().unwrap(),
		"primary selection"
	);

	#[cfg(feature = "image-data")]
	{
		let pixels = vec![255, 0, 0, 255, 0, 255, 0, 255];
		clipboard
			.set_image(arboard::ImageData { width: 2, height: 1, bytes: pixels.clone().into() })
			.unwrap();
		let png = external_read("image/png", false);
		let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();
		assert_eq!(decoded.into_rgba8().into_raw(), pixels);
	}

	let file = std::env::temp_dir().join(format!("arboard {}.txt", std::process::id()));
	std::fs::write(&file, b"clipboard file").unwrap();
	clipboard.set().file_list(&[&file]).unwrap();
	let uris = String::from_utf8(external_read("text/uri-list", false)).unwrap();
	assert!(uris.contains("%20"), "{uris}");
	assert_eq!(clipboard.get().file_list().unwrap(), vec![file.clone()]);
	std::fs::remove_file(file).unwrap();

	let mut owner = ExternalOwner::text("external owner");
	let deadline = Instant::now() + Duration::from_secs(5);
	loop {
		assert!(owner.0.try_wait().unwrap().is_none(), "the external owner exited early");
		if matches!(clipboard.get_text().as_deref(), Ok("external owner")) {
			break;
		}
		assert!(Instant::now() < deadline, "the external owner did not serve its text");
		std::thread::sleep(Duration::from_millis(10));
	}
	drop(owner);
	println!("Text, HTML, primary selection, files, and external ownership transfers passed.");
}
