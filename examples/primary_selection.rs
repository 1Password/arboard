extern crate clipboard;

use clipboard::ClipboardProvider;
#[cfg(target_os = "linux")]
use clipboard::x11_clipboard::{X11ClipboardContext, Primary};

fn main() {
    if cfg!(not(target_os = "linux")) {
        println!("Primary selection is only available under linux!");
        return;
    }
    let mut ctx: X11ClipboardContext<Primary> = ClipboardProvider::new().unwrap();

    let the_string = "Hello, world!";

    ctx.set_contents(the_string.to_owned()).unwrap();
}
