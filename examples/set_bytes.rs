use arboard::Clipboard;
use simple_logger::SimpleLogger;
use std::{fs, thread, time::Duration};

fn main() {
	SimpleLogger::new().init().unwrap();
	let mut ctx = Clipboard::new().unwrap();
	ctx.set_bytes(fs::read("./examples/ferris.png").unwrap(), &b"image/png".to_owned()).unwrap();
	println!("Copied rust logo(staying for 5s)");
	thread::sleep(Duration::from_secs(2));
}
