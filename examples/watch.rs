//! Run `cargo run --example watch` and copy data in another application.
//! Use `--self-test` to write two text values and verify event delivery and stop.
use arboard::{Clipboard, ClipboardEvent, ClipboardWatcher};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let watcher = ClipboardWatcher::new()?;
	assert_eq!(watcher.wait()?, ClipboardEvent::Changed);
	println!("Initial clipboard event received.");

	if std::env::args().any(|arg| arg == "--self-test") {
		let mut clipboard = Clipboard::new()?;
		for text in ["arboard watcher first value", "arboard watcher second value"] {
			clipboard.set_text(text)?;
			assert_eq!(
				watcher.wait_timeout(Duration::from_secs(5))?,
				Some(ClipboardEvent::Changed)
			);
			assert_eq!(clipboard.get_text()?, text);
			println!("Clipboard change received: {text}");
		}
	} else {
		println!("Copy data within 10 seconds to receive a change event.");
		println!("Event: {:?}", watcher.wait_timeout(Duration::from_secs(10))?);
	}

	watcher.stop()?;
	assert_eq!(watcher.wait()?, ClipboardEvent::Stopped);
	println!("Watcher stopped and worker joined.");
	Ok(())
}
