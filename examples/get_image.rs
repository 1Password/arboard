extern crate arboard;

use arboard::{ClipboardContext, ClipboardProvider};

fn main() {
	let mut ctx = ClipboardContext::new().unwrap();

	let img = ctx.get_image().unwrap();

	println!("Image data is:\n{:?}", img.bytes);
}
