/*
Copyright 2016 Avraham Weinstock

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

#![crate_name = "clipboard"]
#![crate_type = "lib"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]

#[cfg(target_os="linux")]
extern crate x11;

#[cfg(windows)]
extern crate clipboard_win;

#[cfg(target_os="macos")]
#[macro_use]
extern crate objc;
#[cfg(target_os="macos")]
extern crate objc_id;
#[cfg(target_os="macos")]
extern crate objc_foundation;

mod util;

#[cfg(target_os="linux")]
mod x11_clipboard;
#[cfg(target_os="linux")]
pub use x11_clipboard::*;

#[cfg(windows)]
mod windows_clipboard;
#[cfg(windows)]
pub use windows_clipboard::*;

#[cfg(target_os="macos")]
mod osx_clipboard;
#[cfg(target_os="macos")]
pub use osx_clipboard::*;


#[cfg(not(any(target_os="linux", windows, target_os="macos")))]
mod nop_clipboard;
#[cfg(not(any(target_os="linux", windows, target_os="macos")))]
pub use nop_clipboard::*;

#[test]
fn test_clipboard() {
    let mut ctx = ClipboardContext::new().unwrap();
    ctx.set_contents("some string".to_owned()).unwrap();
    assert!(ctx.get_contents().unwrap() == "some string");
}
