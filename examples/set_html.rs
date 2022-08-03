use arboard::Clipboard;
use simple_logger::SimpleLogger;
use std::{thread, time::Duration};

fn main() {
	SimpleLogger::new().init().unwrap();

	let mut ctx = Clipboard::new().unwrap();

	let text = "Hello, World!\n\
        Lorem ipsum dolor sit amet,\n\
        consectetur adipiscing elit.";
	let html = "<h1>Hello, World!</h1>\
        <b>Lorem ipsum</b> dolor sit amet,<br>\
        <i>consectetur adipiscing elit</i>.";
	ctx.set_html(html, Some(text)).unwrap();
	thread::sleep(Duration::from_secs(5));
}
