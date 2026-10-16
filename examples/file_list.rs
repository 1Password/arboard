use arboard::Clipboard;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
	if files.is_empty() {
		return Err("Usage: cargo run --example file_list -- PATH [PATH ...]".into());
	}
	let mut clipboard = Clipboard::new()?;
	clipboard.set().file_list(&files)?;
	println!("Files: {:?}", clipboard.get().file_list()?);

	#[cfg(all(unix, not(target_os = "macos")))]
	for name in ["text/uri-list", "x-special/gnome-copied-files", "x-special/nautilus-clipboard"] {
		let format = arboard::ClipboardFormat::register(name)?;
		let bytes = clipboard.get().raw(&format)?;
		println!("{name}:\n{}", String::from_utf8(bytes)?);
	}
	Ok(())
}
