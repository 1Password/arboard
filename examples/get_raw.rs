use arboard::{Clipboard, ClipboardFormat};
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let name =
		std::env::args().nth(1).ok_or("Usage: cargo run --example get_raw -- NATIVE_FORMAT")?;
	let format = ClipboardFormat::register(&name)?;
	let bytes = Clipboard::new()?.get().raw(&format)?;
	std::io::stdout().lock().write_all(&bytes)?;
	Ok(())
}
