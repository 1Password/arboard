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
    pub fn set_contents(&self, data: String) -> Result<(), Box<Error>> {
        Ok(try!(set_clipboard(&data)))
    }
}
