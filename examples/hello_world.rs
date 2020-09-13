use arboard::Clipboard;

fn main() {
	let mut clipboard = Clipboard::new().unwrap();
	println!("Clipboard text was: {}", clipboard.get_text().unwrap());

	let the_string = "Hello, world!";
	clipboard.set_text(the_string.into()).unwrap();
	println!("But now the clipboard text should be: \"{}\"", the_string);
}
