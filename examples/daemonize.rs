//! Example showcasing the use of `set_text_wait` and spawning a daemon to allow the clipboard's
//! contents to live longer than the process on Linux.

use arboard::Clipboard;
#[cfg(target_os = "linux")]
use arboard::ClipboardExtLinux;
use simple_logger::SimpleLogger;
use std::{env, error::Error, process};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
	#[cfg(target_os = "linux")]
	if env::args().nth(1).as_deref() == Some("__internal_daemonize") {
		Clipboard::new()?.set_text_wait("Hello, world!".into())?;
		return Ok(());
	}

	let _logger = SimpleLogger::new().init().unwrap();

	if cfg!(target_os = "linux") {
		process::Command::new(env::current_exe()?)
			.arg("__internal_daemonize")
			.stdin(process::Stdio::null())
			.stdout(process::Stdio::null())
			.stderr(process::Stdio::null())
			.current_dir("/")
			.spawn()?;
	} else {
		Clipboard::new()?.set_text("Hello, world!".into())?;
	}

	Ok(())
}
