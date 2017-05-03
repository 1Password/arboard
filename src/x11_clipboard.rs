/*
Copyright 2017 Avraham Weinstock

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

use std::error::Error;
use std::time::Duration;
use common::*;
use x11_clipboard_crate::Clipboard;

pub struct X11ClipboardContext(Clipboard);

impl ClipboardProvider for X11ClipboardContext {
    fn new() -> Result<X11ClipboardContext, Box<Error>> {
        Clipboard::new()
            .map(X11ClipboardContext)
            .map_err(Into::into)
    }

    fn get_contents(&mut self) -> Result<String, Box<Error>> {
        self.0.load(
            self.0.getter.atoms.clipboard,
            self.0.getter.atoms.utf8_string,
            self.0.getter.atoms.property,
            Duration::from_secs(3)
        )
            .map_err(Into::into)
            .and_then(|vec| String::from_utf8(vec).map_err(Into::into))
    }

    fn set_contents(&mut self, data: String) -> Result<(), Box<Error>> {
        self.0.store(
            self.0.setter.atoms.clipboard,
            self.0.setter.atoms.utf8_string,
            data
        )
            .map_err(Into::into)
    }
}
