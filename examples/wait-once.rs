//! Example showcasing the use of `wait_once`.

use arboard::Clipboard;
#[cfg(all(
	unix,
	not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
))]
use arboard::SetExtLinux;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	env_logger::init();

	let mut clipboard = Clipboard::new()?;
	let mut set = clipboard.set();
	#[cfg(all(
		unix,
		not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
	))]
	{
		set = set.wait_once();
		eprintln!("Waiting for clipboard to be pasted once before exiting...");
	}
	set.text("Hello, world!")?;

	Ok(())
}
