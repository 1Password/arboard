#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
	use arboard::{Clipboard, Error, GetExtLinux};
	use std::time::Duration;

	env_logger::init();
	let mut clipboard = Clipboard::new()?;
	match clipboard.get().read_timeout(Duration::from_millis(250)).text() {
		Ok(text) => println!("Clipboard text: {text}"),
		Err(Error::Timeout) => println!("The clipboard owner did not respond within 250 ms."),
		Err(error) => return Err(error.into()),
	}
	Ok(())
}

#[cfg(not(all(
	unix,
	not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
)))]
fn main() {
	eprintln!("This example requires an X11 clipboard.");
}
