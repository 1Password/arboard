# arboard

This crate is a fork of rust-clipboard and a cross-platform library for getting and setting the contents of the OS-level clipboard. Most notably this crate allows setting and getting image data to/from the clipboard.

## Prerequisites

On Linux you need the x11 library, install it with something like:

```bash
sudo apt-get install xorg-dev
```

## Example

```rust
extern crate arboard;

use arboard::{ClipboardContext, ClipboardProvider};

fn example() {
    let mut ctx = ClipboardContext::new().unwrap();
    println!("{:?}", ctx.get_text());
    ctx.set_text("some string".to_owned()).unwrap();
}
```

## API

The `ClipboardProvider` trait has the following functions:

```rust
fn new() -> Result<Self, Box<Error>>;
fn get_text(&mut self) -> Result<String, Box<Error>>;
fn set_text(&mut self, String) -> Result<(), Box<Error>>;
```

`ClipboardContext` is a type alias for one of {`WindowsClipboardContext`, `OSXClipboardContext`, `X11ClipboardContext`}, all of which implement `ClipboardProvider`. Which concrete type is chosen for `ClipboardContext` depends on the OS (via conditional compilation).

## License

`rust-clipboard` is dual-licensed under MIT and Apache2.
