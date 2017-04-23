/*
Copyright (C) 2016 Avraham Weinstock

This program is free software; you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation; either version 2 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License along
with this program; if not, write to the Free Software Foundation, Inc.,
51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
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
