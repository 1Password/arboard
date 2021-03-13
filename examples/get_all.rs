extern crate arboard;

use arboard::{Clipboard, CustomItem};

fn main() {
	let mut ctx = Clipboard::new().unwrap();
	//let custom_item = CustomItem::ImageSvg(data.into());
	let available = ctx.get_all().unwrap();
	for item in available {
		println!("Mime: {}", item.media_type());
		match item {
			CustomItem::TextPlain(t) => {
				println!("Plain text data '{}'", t);
			}
			CustomItem::TextHtml(t) => {
				println!("Html data: '{}'", t);
			}
			_ => (),
		}
	}
	println!("Finished receiving custom data!");
}
