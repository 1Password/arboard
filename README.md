# rust-clipboard

rust-clipboard is a cross-platform library for getting and setting the contents of the OS-level clipboard.  
It has been tested on Windows, Mac OSX, and GNU/Linux.  
It is used in Mozilla Servo.

[![](http://meritbadge.herokuapp.com/clipboard)](https://crates.io/crates/clipboard)
[![Appveyor Build Status](https://ci.appveyor.com/api/projects/status/github/aweinstock314/rust-clipboard)](https://ci.appveyor.com/project/aweinstock314/rust-clipboard)
[![Travis Build Status](https://travis-ci.org/aweinstock314/rust-clipboard.svg?branch=master)](https://travis-ci.org/aweinstock314/rust-clipboard)

## Prerequisites

On Linux you need the x11 library, install it with something like:

```bash
sudo apt-get install xorg-dev
```

## Example

```rust
extern crate clipboard;

use clipboard::ClipboardProvider;
use clipboard::ClipboardContext;

fn example() {
    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
    println!("{}", ctx.get_contents());
    ctx.set_contents("some string".to_owned());
}
```

## API

The `ClipboardProvider` trait has the following functions:

```rust
fn new() -> Result<Self, Box<Error>>;
fn get_contents(&mut self) -> Result<String, Box<Error>>;
fn set_contents(&mut self, String) -> Result<(), Box<Error>>;
```

`ClipboardContext` is a type alias for one of {`WindowsClipboardContext`, `OSXClipboardContext`, `X11ClipboardContext`, `NopClipboardContext`}, all of which implement `ClipboardProvider`. Which concrete type is chosen for `ClipboardContext` depends on the OS (via conditional compilation).

## License

`rust-clipboard` is licensed under Apache2.
