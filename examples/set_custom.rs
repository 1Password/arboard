extern crate arboard;

use arboard::{Clipboard, CustomItem};

fn main() {
	let mut ctx = Clipboard::new().unwrap();

	#[rustfmt::skip]
	let data = "
        <svg height='300' width='300'>
        <path
            d='M 100 100 L 200 200 H 10 V 40 H 70'
            fill='#59fa81'
            stroke='#d85b49'
            stroke-width='3'
        />
        </svg>
    ";
	//let custom_item = CustomItem::ImageSvg(data.into());
    let custom_item = CustomItem::TextPlain(data.into());
	ctx.set_custom(vec![custom_item]).unwrap();
    println!("Succesfully set custom data!");
}
