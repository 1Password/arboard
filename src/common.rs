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

use std::error::Error;

pub fn err(s: &str) -> Box<Error> {
    Box::<Error + Send + Sync>::from(s)
}

/// Trait for clipboard access
pub trait ClipboardProvider: Sized {
    /// Create a context with which to access the clipboard
    // TODO: consider replacing Box<Error> with an associated type?
    fn new() -> Result<Self, Box<Error>>;
    /// Method to get the clipboard contents as a String
    fn get_contents(&mut self) -> Result<String, Box<Error>>;
    /// Method to set the clipboard contents as a String
    fn set_contents(&mut self, String) -> Result<(), Box<Error>>;
    // TODO: come up with some platform-agnostic API for richer types
    // than just strings (c.f. issue #31)
}
