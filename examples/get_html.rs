use arboard::Clipboard;

fn main() {
	let mut ctx = Clipboard::new().unwrap();

	let html_data = ctx.get_html().unwrap();

	println!("HTML data is:\n{:?}\nAlt text is:\n{:?}", html_data.html, html_data.alt_text);
}