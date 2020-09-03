# arboard

This is a cross-platform library for interacting with the clipboard. It can get and set both text and image data in a platform independent way on Linux, Mac, and Windows.

The Linux implementation uses the X protocol for managing the clipboard but *fear not*  because Wayland works with the X11 protocoll just as well. Furthermore this implementation uses the Clipboard selection (as opposed to the primary selection) and it sends the data to the clipboard manager when the application exits so that the data placed onto the clipboard with your application remains to be available after exiting.

It is a fork of `rust-clipboard`. The reason for forking instead of making a PR is that `rust-clipboard` is not being maintained anymore. There are already a ton of clipboard crates out there which is a bit unfortunate; I don't know why this is happening but while it is, we might as well just start naming the clipboard crates after ourselves. This one is arboard which stands for Artur's clipboard.

## Prerequisites

On Linux you need the x11 library when building your application. Install it with something like:

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
