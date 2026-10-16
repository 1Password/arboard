//! Route library diagnostics to a file instead of the terminal.

use arboard::Clipboard;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let path = std::env::args().nth(1).unwrap_or_else(|| "clipboard.log".into());
	let file = std::fs::File::create(&path)?;
	env_logger::Builder::new()
		.filter_module("arboard", log::LevelFilter::Trace)
		.target(env_logger::Target::Pipe(Box::new(file)))
		.try_init()?;

	let mut clipboard = Clipboard::new()?;
	#[allow(unused_mut)]
	let mut set = clipboard.set();
	#[cfg(all(
		unix,
		not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
	))]
	{
		use arboard::SetExtLinux;
		// Avoid a clipboard-manager handoff so a quick drop can demonstrate the debug warning.
		set = set.exclude_from_history();
	}
	set.text("Clipboard logging example")?;
	drop(clipboard);
	log::logger().flush();
	println!("Clipboard diagnostics use {path}, not the terminal.");
	Ok(())
}
