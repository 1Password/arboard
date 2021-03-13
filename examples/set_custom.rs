extern crate arboard;

use arboard::{Clipboard, CustomItem};

fn main() {
	let mut ctx = Clipboard::new().unwrap();

	#[rustfmt::skip]
	let data = "
        <svg height='300' width='300'>
            <rect x='0' y='0' width='100' height='100' fill='#529fca' />
            <circle cx='50' cy='50' r='50' fill='#dd2a33' />
        </svg>
    ";
	//let custom_item = CustomItem::ImageSvg(data.into());
	let custom_item = CustomItem::ImageSvg(data.into());
	ctx.set_custom(vec![custom_item]).unwrap();
	println!("Succesfully set custom data!");
}
