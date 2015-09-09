# rust-clipboard
rust-clipboard is a cross-platform library for getting and setting the contents of the OS-level clipboard.
It has been tested on Windows, Mac OSX, and GNU/Linux.
It is used in Mozilla Servo.

## Example

```rust
fn example() {
    let mut ctx = ClipboardContext::new().unwrap();
    println!("{}", ctx.get_contents());
    ctx.set_contents(&"some string");
}
```

## API

```rust
fn new() -> Result<ClipboardContext, Box<Error>>
fn get_contents(&ClipboardContext) -> Result<String, Box<Error>>
fn set_contents(&mut ClipboardContext, String) -> Result<(), Box<Error>>
```

`ClipboardContext` is an opaque struct that is defined in different ways based on the OS via conditional compilation.

## License
Since the x11 backend contains code derived from xclip (which is GPLv2), rust-clipboard is also GPLv2.
