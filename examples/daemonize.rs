//! Example showcasing the use of `set_text_wait` and spawning a daemon to allow the clipboard's
//! contents to live longer than the process on Linux.

use arboard::{Clipboard, ClipboardExtLinux};
use simple_logger::SimpleLogger;
use std::{env, error::Error, process};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
	if env::args().nth(1).as_deref() == Some("__internal_daemonize") {
		Clipboard::new()?.set_text_wait("Hello, world!".into()).unwrap();
		return Ok(());
	}

	let _logger = SimpleLogger::new().init().unwrap();

	process::Command::new(dbg!(env::current_exe()?))
		.arg("__internal_daemonize")
		.stdin(process::Stdio::null())
		.stdout(process::Stdio::null())
		.stderr(process::Stdio::null())
        .current_dir("/")
		.spawn()?;

	Ok(())
}
