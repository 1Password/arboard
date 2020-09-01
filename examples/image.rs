extern crate clipboard;

use clipboard::{ClipboardContext, ClipboardProvider, ImageData};

fn main() {
	let mut ctx = ClipboardContext::new().unwrap();

	#[rustfmt::skip]
    let img_data = ImageData {
        width: 2,
        height: 2,
        bytes: [
            255, 100, 100, 255,
            100, 255, 100, 100,
            100, 100, 255, 100,
            0, 0, 0, 255,
        ].as_ref().into(),
    };

	ctx.set_image(img_data).unwrap();
}
