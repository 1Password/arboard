use arboard::Clipboard;

fn main() {
	env_logger::init();
	let mut clipboard = Clipboard::new().unwrap();
	println!("Clipboard text was: {:?}", clipboard.get_html());

	let html = "<h1>Hello, world!</h1><p>This is HTML.</p>";
	let alt_text = "Hello, world!\nThis is HTML.";
	clipboard.set_html(html, Some(alt_text)).unwrap();
	println!(
		"But now the clipboard text should be this HTML: \"{}\" with this alternate text: \"{}\"",
		html, alt_text
	);
}