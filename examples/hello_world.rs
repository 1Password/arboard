extern crate arboard;

use arboard::Clipboard;

fn main() {
	let mut ctx = Clipboard::new().unwrap();

	let the_string = "Hello, world!";

	ctx.set_text(the_string.to_owned()).unwrap();
}
