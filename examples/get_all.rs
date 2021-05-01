use log::LevelFilter;
use simple_logger::SimpleLogger;

use arboard::{Clipboard, CustomItem};

fn main() {
	let _logger = SimpleLogger::new().with_level(LevelFilter::Trace).init().unwrap();

	let mut ctx = Clipboard::new().unwrap();
	//let custom_item = CustomItem::ImageSvg(data.into());
	let available = ctx.get_all().unwrap();
	for item in available {
		println!("Mime: {}", item.media_type());
		match item {
			CustomItem::Text(t) => {
				println!("Plain text data '{}'", t);
			}
			// CustomItem::TextHtml(t) => {
			// 	println!("Html data: '{}'", t);
			// }
			CustomItem::TextUriList(t) => {
				println!("Uri List:\n-----\n{}\n------", t);
			}
			CustomItem::ImagePng(img) => {
				continue;
				let name = "clipboard.png";
				std::fs::write(name, img.as_ref()).unwrap();
				println!("PNG written to {}", name);
			}
			CustomItem::RawImage(img) => {
				// continue;
				let name = "clipboard.png";
				image::save_buffer_with_format(
					name,
					img.bytes.as_ref(),
					img.width as u32,
					img.height as u32,
					image::ColorType::Rgba8,
					image::ImageFormat::Png,
				)
				.unwrap();
				println!("PNG written to {}", name);
			}
			_ => (),
		}
	}
	println!("Finished receiving custom data!");
}
