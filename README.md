# Arboard (Arthur's Clipboard)

[![Latest version](https://img.shields.io/crates/v/arboard?color=mediumvioletred)](https://crates.io/crates/arboard)
[![Documentation](https://docs.rs/arboard/badge.svg)](https://docs.rs/arboard)
![MSRV](https://img.shields.io/badge/rustc-1.71.0+-blue.svg)

## General

This is a cross-platform library for interacting with the clipboard. It allows
to copy and paste both text and image data in a platform independent way on
Linux, Mac, and Windows.

Please note that this is not an official 1Password product. Feature requests will be considered like any other volunteer-based crate.

## GNU/Linux

### Backend Support

By default, `arboard`'s backend on Linux supports X11 (or XWayland implementations) and uses
that for managing the various Linux clipboard variants. This supports the majority of desktop 
environments that exist in the wild today. `arboard` will use the `Clipboard` selection by default,
but the [LinuxClipboardKind](https://docs.rs/arboard/latest/arboard/enum.LinuxClipboardKind.html)
selector lets you operate on the `Primary` or `Secondary` clipboard selections (if supported).

However, Wayland is becoming the majority default as of 2025. Some distributions are
even considering the removal of X by default. To support Wayland correctly, `arboard` users
should enable the `wayland-data-control` feature. If enabled, it will be prioritized over the X clipboard.

Wayland support is not enabled by default because it may be counterintuitive 
to some users: it relies on the data-control protocol extension(s), which _are not_
supported by all Wayland compositors. You can check compositor support on `wayland.app`:
- [ext-data-control-v1](https://wayland.app/protocols/ext-data-control-v1)
- [wlr-data-control-unstable-v1](https://wayland.app/protocols/wlr-data-control-unstable-v1)

If you or a user's desktop doesn't support these protocols, `arboard` won't function in a pure
Wayland environment. It is recommended to enable `XWayland` for these cases. If your app runs inside
an isolated sandbox, such as Flatpak or Snap, you'll need to expose the X11 socket to the application
_in addition_ to the Wayland communication interface.

### Clipboard Ownership

Some apps and users may notice that sometimes values copied to a Linux clipboard with this crate vanish
before anyone gets the chance to paste, or just aren't available when you expect them to be. The root behind
these problems is _selection ownership_.

X11 and Wayland put the responsibility for answering paste requests and serving data on the application
which originally copied it onto the clipboard. This usually means the app using `arboard`. Nothing you copy
to the clipboard is sent anywhere to start. It stays inside `arboard` until something else on the system requests it,
which is very different to how the clipboard works on other platforms like macOS or Windows.

Note that `arboard` may attempt to warn you about these conditions when compiled in debug mode, to improve the debugging
experience. Even if you don't see these warnings, you should double check the lifetime of the `Clipboard` in your code.

In some cases, an environment may have a clipboard manager installed. These services monitor the clipboard contents and
do their best to retain a copy when needed to smooth over clipboard ownership changes. A clipboard manager can make contents
available even after a process previously owning it exits.

In order to keep the contents around longer, make sure that you don't `Drop` your `Clipboard` object right away or
terminate the copying process too fast. This is why, at times, adding a call to `sleep()` near the set operation
makes it behave more reliabily: the background thread `arboard` uses for serving clipboard contents has more time to run
and let other apps (including clipboard managers) make requests for the contents. However `sleep` isn't the recommended approach.

If your application is exiting, you must make sure there is a clipboard manager running on the system. If nothing is listening for 
the clipboard ownership transfer, or made a copy previously, the data will be lost. Note that this isn't a complete 
guarantee as races are possible if your program's main thread is exiting. If you would like to fully synchronize the clipboard "paste"
before exiting, you can use the [wait](https://docs.rs/arboard/latest/arboard/trait.SetExtLinux.html#tymethod.wait) method when setting
contents on the clipboard. This will block the calling thread until another app has requested, and then received, the data.

If your application is longer-running (ie a GUI, TUI, etc), it is highly recommended that you either store the `Clipboard` object in some 
long-lived data structure (like app context, etc) or utilize `wait` method mentioned above, and/or threading to make sure another 
app can request the clipboard data later.

We welcome suggestions to improve on the above issues in ways that don't degrade other use cases.

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

## Native formats and combined writes

Run `cargo run --example formats` to publish text, HTML, RTF, and custom bytes together.
The example also shows format discovery, presence checks, native names, and standalone byte writes.
It replaces the system clipboard contents.

Use `ClipboardFormat::register` for native names. Windows standard formats use numeric identifiers,
not MIME strings. Use `ClipboardFormat::from_windows_id` for these identifiers.
Format identities remain valid only within the native OS session.

Use `clipboard.get().formats()` to list formats without data transfer.
Use `contains` for a presence check and `raw` for native bytes.
Windows raw access supports documented global-memory formats, not bitmap, palette, or metafile handles.
X11 raw writes larger than one native property request fail before the clipboard changes.
Raw reads preserve native terminators and can include zero-filled Windows allocation padding.
The OS can supply synthesized formats. Native macOS byte reads can combine multiple text items.
The existing typed text getter still returns the first text item.
Run `cargo run --example get_raw -- NATIVE_FORMAT` to write native bytes to standard output.

Use `clipboard.set().formats(...)` for one write with several representations.
The typed HTML variant adds native headers. The RTF variant accepts non-UTF-8 bytes.
Raw variants require the complete native encoding.
`ClipboardContent::Image` accepts RGBA pixels with the existing `image-data` feature.
It offers PNG on Linux, TIFF on macOS, and PNG plus `CF_DIBV5` on Windows.
`ClipboardContent::Files` accepts file paths and prepares all paths before the write.
On macOS, each file occupies one item; the first item also holds the other representations.
Privacy markers apply to every file item.
Run the `formats` example to verify mixed text/image and text/file writes.

The setter prepares data before it clears or replaces the clipboard once.
It rejects duplicate native identities, including conflicts with privacy markers.
An OS failure can leave partial contents. The API does not promise rollback.
All privacy, Linux selection, and Linux wait options apply to combined writes.
`wait_once` blocks until a reader retrieves data. `wait_until` blocks until replacement or its deadline.
After the deadline, the process can continue to serve the data while it remains alive.
Discovery and presence checks are snapshots; another application can change the clipboard before a read.

Linux file lists support URI lists, GNOME copied files, and Nautilus clipboard data.
Local file URIs accept an empty host or `localhost`.
Remote hosts, malformed escapes, and unescaped query or fragment separators return a conversion error.
Percent escapes preserve non-UTF-8 filename bytes. Other URI schemes and header lines are ignored.
File writes fail if any path cannot resolve; they do not silently omit that path.
Run `cargo run --example file_list -- Cargo.toml` to show these native file representations.

The integration tests require a native clipboard. They fail when no backend is available.
On Linux, run `xvfb-run -a cargo test --test formats --all-features` for X11.
With Python 3 and `xclip`, run `xvfb-run -a python3 tools/test-raw-x11.py` for an incremental raw transfer.
For Wayland, install Sway, then run
`bash tools/with-wayland.sh cargo test --test formats --all-features`.
The helper creates a private headless compositor and removes it after the command.

## Credits

This crate is a combined effort by 1Password staff and `@ArturKovacs`, the crate's past
maintainer.

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE.txt">Apache License, Version
2.0</a> or <a href="LICENSE-MIT.txt">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>

#### History: Yet another clipboard crate

This crate started out as a fork of `rust-clipboard`. The reason for forking is due to the former
crate not being maintained any longer. At this point, `arboard`'s backends and public APIs have diverged
a lot.

`arboard`'s original maintainer noted that "I don't know why this is happening but while it is, we might 
as well just start naming the clipboard crates after ourselves. This one is arboard which stands for Artur's clipboard.".
