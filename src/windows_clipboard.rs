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

use clipboard_win::{get_clipboard_string, set_clipboard};

use std::error::Error;

pub struct ClipboardContext;

impl ClipboardContext {
    pub fn new() -> Result<ClipboardContext, Box<Error>> {
        Ok(ClipboardContext)
    }
    pub fn get_contents(&self) -> Result<String, Box<Error>> {
        Ok(try!(get_clipboard_string()))
    }
    pub fn set_contents(&mut self, data: String) -> Result<(), Box<Error>> {
        Ok(try!(set_clipboard(&data)))
    }
}
