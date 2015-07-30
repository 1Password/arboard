#![crate_name = "clipboard"]
#![crate_type = "lib"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]

#![cfg_attr(target_os="linux", feature(vec_push_all))]

#[cfg(target_os="linux")]
extern crate libc;
#[cfg(target_os="linux")]
extern crate x11;

#[cfg(windows)]
extern crate clipboard_win;


#[cfg(target_os="linux")]
mod x11_clipboard;
#[cfg(target_os="linux")]
pub use x11_clipboard::*;

#[cfg(windows)]
mod windows_clipboard;
#[cfg(windows)]
pub use windows_clipboard::*;


#[cfg(not(any(target_os="linux", windows)))]
mod nop_clipboard;
#[cfg(not(any(target_os="linux", windows)))]
pub use nop_clipboard::*;
