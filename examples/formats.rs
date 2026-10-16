use arboard::{Clipboard, ClipboardContent, ClipboardFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut clipboard = Clipboard::new()?;
	#[cfg(target_os = "macos")]
	let custom_name = "org.arboard.example";
	#[cfg(not(target_os = "macos"))]
	let custom_name = "application/x-arboard-example";
	let custom = ClipboardFormat::register(custom_name)?;
	let rtf = b"{\\rtf1\\ansi Rich text \xe9}";
	clipboard.set().formats([
		ClipboardContent::Text("Plain text".into()),
		ClipboardContent::Html("<b>Rich text</b>".into()),
		ClipboardContent::Rtf(rtf.as_slice().into()),
		ClipboardContent::Raw(custom.clone(), [0, 1, 255].as_slice().into()),
	])?;
	for format in clipboard.get().formats()? {
		println!("Format: {format:?}, name: {:?}", format.name()?);
	}
	assert!(clipboard.get().contains(&custom)?);
	assert!(clipboard.get().raw(&custom)?.starts_with(&[0, 1, 255]));
	assert!(clipboard.get().rtf()?.starts_with(rtf));
	assert_eq!(clipboard.get().text()?, "Plain text");
	assert!(clipboard.get().html()?.contains("<b>Rich text</b>"));
	println!("Text, HTML, RTF, and custom bytes are available together.");
	#[cfg(windows)]
	{
		// CF_UNICODETEXT is a standard numeric format, not a registered MIME name.
		let text = ClipboardFormat::from_windows_id(13)?;
		assert!(clipboard.get().contains(&text)?);
		println!(
			"Windows Unicode text ID: {}, registered name: {:?}",
			text.windows_id(),
			text.name()?
		);
	}

	clipboard.set().rtf(rtf.as_slice())?;
	assert!(clipboard.get().rtf()?.starts_with(rtf));
	clipboard.set().raw(custom.clone(), [3, 2, 1].as_slice())?;
	assert!(clipboard.get().raw(&custom)?.starts_with(&[3, 2, 1]));
	println!("Standalone RTF and raw byte writes also succeeded.");
	Ok(())
}
