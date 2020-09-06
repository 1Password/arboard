
use arboard::Clipboard;

fn main() {
	let mut ctx = Clipboard::new().unwrap();
	println!("Clipboard text was: {}", ctx.get_text().unwrap());

	let the_string = "Hello, world!";
	ctx.set_text(the_string.into()).unwrap();
	println!("But now the clipboard text should be: \"{}\"", the_string);
}
