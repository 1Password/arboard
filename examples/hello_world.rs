use arboard::Clipboard;
use simple_logger::SimpleLogger;

fn main() {
	SimpleLogger::new().init().unwrap();
	let mut clipboard = Clipboard::new().unwrap();

	for format in [b"text/html" as &[u8], b"text/plain", b"image/png", b"application/json"] {
		println!("Format: {:?}", String::from_utf8_lossy(format));
		println!(
			"Content: {:#?}",
			clipboard
				.get_bytes(format)
				.map(|bytes| bytes.into_iter().map(|c| c as char).collect::<String>())
		);
	}

	println!("Clipboard text was: {:?}", clipboard.get_text());

	let the_string = "Hello, world!";
	// clipboard.set_text(the_string).unwrap();
	println!("But now the clipboard text should be: \"{}\"", the_string);
}
