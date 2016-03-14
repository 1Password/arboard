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

pub struct ClipboardContext;

impl ClipboardContext {
    pub fn new() -> Result<ClipboardContext, &'static str> {
        Ok(ClipboardContext)
    }
    pub fn get_contents(&self) -> Result<String, &str> {
        println!("Attempting to get the contents of the clipboard, which hasn't yet been implemented on this platform.");
        Ok("".to_string())
    }
    pub fn set_contents(&self, _: String) -> Result<(), &str> {
        println!("Attempting to set the contents of the clipboard, which hasn't yet been implemented on this platform.");
        Ok(())
    }
}
