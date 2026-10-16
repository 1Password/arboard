use arboard::{Clipboard, ImageLimits};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let arguments: Vec<_> = std::env::args().skip(1).collect();
	if arguments.len() != 4 {
		return Err("usage: image_limits <encoded-bytes> <width> <height> <decoded-bytes>".into());
	}
	let mut limits = ImageLimits::default();
	limits.max_encoded_bytes = Some(arguments[0].parse()?);
	limits.max_width = Some(arguments[1].parse()?);
	limits.max_height = Some(arguments[2].parse()?);
	limits.max_decoded_bytes = Some(arguments[3].parse()?);
	let image = Clipboard::new()?.get().image_limits(limits).image()?;
	println!("Image: {} x {}, {} RGBA bytes", image.width, image.height, image.bytes.len());
	Ok(())
}
