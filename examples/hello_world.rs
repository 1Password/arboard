extern crate arboard;

use arboard::ClipboardContext;
use arboard::ClipboardProvider;

fn main() {
	let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();

	let the_string = "Hello, world!";

	ctx.set_text(the_string.to_owned()).unwrap();
}
