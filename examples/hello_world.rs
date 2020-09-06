extern crate arboard;

use arboard::Clipboard;

fn main() {
	let mut ctx = Clipboard::new().unwrap();

	let the_string = "Hello, world!";

	println!("Text was: {}", ctx.get_text().unwrap());
}
