
[![Latest version](https://img.shields.io/crates/v/arboard?color=mediumvioletred)](https://crates.io/crates/arboard)
[![Documentation](https://docs.rs/arboard/badge.svg)](https://docs.rs/arboard)

## General

This is a cross-platform library for interacting with the clipboard. It can get and set both text and image data in a platform independent way on Linux, Mac, and Windows.

The Linux implementation uses the X protocol for managing the clipboard but *fear not*  because Wayland works with the X11 protocoll just as well. Furthermore this implementation uses the Clipboard selection (as opposed to the primary selection) and it sends the data to the clipboard manager when the application exits so that the data placed onto the clipboard with your application remains to be available after exiting.

This is a fork of `rust-clipboard`. The reason for forking instead of making a PR is that `rust-clipboard` is not being maintained anymore. Furthermore note that the API of this crate is considerably different from that of `rust-clipboard`. There are already a ton of clipboard crates out there which is a bit unfortunate; I don't know why this is happening but while it is, we might as well just start naming the clipboard crates after ourselves. This one is arboard which stands for Artur's clipboard.

## Prerequisites

On Linux you need the x11 library when building your application. Install it with something like:

```bash
sudo apt-get install xorg-dev
```

## Example

```rust
use arboard::Clipboard;

fn main() {
	let mut clipboard = Clipboard::new().unwrap();
	println!("Clipboard text was: {}", clipboard.get_text().unwrap());

	let the_string = "Hello, world!";
	clipboard.set_text(the_string.into()).unwrap();
	println!("But now the clipboard text should be: \"{}\"", the_string);
}
```
