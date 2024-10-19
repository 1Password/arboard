# Arboard (Arthur's Clipboard)

[![Latest version](https://img.shields.io/crates/v/arboard?color=mediumvioletred)](https://crates.io/crates/arboard)
[![Documentation](https://docs.rs/arboard/badge.svg)](https://docs.rs/arboard)
![MSRV](https://img.shields.io/badge/rustc-1.67.1+-blue.svg)

## General

This is a cross-platform library for interacting with the clipboard. It allows
to copy and paste both text and image data in a platform independent way on
Linux, Mac, and Windows.

### GNU/Linux

The GNU/Linux implementation uses the X protocol by default for managing the
clipboard but because Wayland works with the X11 protocol, when `xwayland` is
available and enabled. Furthermore this implementation uses the Clipboard
X11 selection (as opposed to the primary selection). It also sends the data to the 
clipboard manager when the application exits so that the data placed onto the clipboard 
by your application remains available after exiting.

There's also an optional wayland data control backend through the
`wl-clipboard-rs` crate. This can be enabled using the `wayland-data-control`
feature. When enabled this will be prioritized over the X11 backend, but if Wayland
initialization fails, the implementation falls back to using the X11 protocol
automatically. Note that clipboard contents remaining after application exit
may be dependent on your compositor.

## Example

```rust
use arboard::Clipboard;

fn main() {
    let mut clipboard = Clipboard::new().unwrap();
    println!("Clipboard text was: {}", clipboard.get_text().unwrap());

    let the_string = "Hello, world!";
    clipboard.set_text(the_string).unwrap();
    println!("But now the clipboard text should be: \"{}\"", the_string);
}
```

## FAQ

This section outlines some very frequently asked questions about the crate and its behavior. If your question isn't answered
by these, please feel free to open an issue.

- Q: On Linux, the data copied to the clipboard disappears too fast.
    - A: X11 and Wayland put the responsibility for answering paste requests and serving data on the application
    which originally copied it onto the clipboard. This usually means the app using `arboard`. In order to keep the
    contents around longer, make sure that you don't Drop your `Clipboard` object right away.
- Q: Why does adding `sleep`s or other timing changes to my code improve results?
    - A: The handling of other app's clipboard requests, including clipboard managers, is handled by a background
    worker in `arboard`. If your active thread is sleeping, it gives the worker more time to listen and finish the final data handoff.
- Q: What are ways to keep clipboard contents around longer on Linux?
    - A1: If your application is exiting, you must make sure there is a clipboard manager running on the system.
    If nothing is listening for the clipboard ownership transfer, the data will be lost. Note that this isn't a
    complete guarantee as races are possible if your program is exiting. We welcome suggestions to improve on this.
    - A2: If your application is longer-running, it is highly recommended that you either store the `Clipboard` object in
    some long-lived data structure (like app context, etc) or utilize the [wait](https://docs.rs/arboard/latest/arboard/trait.SetExtLinux.html#tymethod.wait)
    method and/or threading to make sure another app can request the clipboard data later.

## Yet another clipboard crate

This is a fork of `rust-clipboard`. The reason for forking instead of making a
PR is that `rust-clipboard` is not being maintained any more. Furthermore note
that the API of this crate is considerably different from that of
`rust-clipboard`. There are already a ton of clipboard crates out there which
is a bit unfortunate; I don't know why this is happening but while it is, we
might as well just start naming the clipboard crates after ourselves. This one
is arboard which stands for Artur's clipboard.
