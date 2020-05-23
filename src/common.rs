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
use std::borrow::Cow;

pub fn err(s: &str) -> Box<Error> {
    Box::<Error + Send + Sync>::from(s)
}

pub enum ClipboardContent {
    Utf8(String),
    Tiff(Vec<u8>),
    // TODO: extend this enum by more types
    // Url, RichText, ....
    #[doc(hidden)]
    __Nonexhaustive,
}

/// Stores pixel data of an image.
///
/// Each element in `bytes` stores the value of a channel of a single pixel.
/// This struct stores four channels (red, green, blue, alpha) so
/// a 3*3 image is going to be stored by 3*3*4 = 36 bytes of data.
///
/// The pixels are stored in row-major order meaning that the second pixel
/// in `bytes` corresponds to the pixel form the first row and second column.
/// 
/// Assigning a 2*1 image would for example look like this
/// ```
/// use std::borrow::Cow;
/// let bytes = [
///     // A red pixel
///     255, 0, 0, 255,
/// 
///     // A green pixel
///     0, 255, 0, 255,
/// ];
/// let img = ImageData {
///     width: 2,
///     height: 1,
///     bytes: Cow::from(bytes.as_ref())
/// };
/// ```
pub struct ImageData<'a> {
    pub width: usize,
    pub height: usize,
    pub bytes: Cow<'a, [u8]>,
}

/// Trait for clipboard access
pub trait ClipboardProvider: Sized {
    /// Create a context with which to access the clipboard
    // TODO: consider replacing Box<Error> with an associated type?
    fn new() -> Result<Self, Box<Error>>;
    /// Method to get the clipboard contents as a String
    fn get_text(&mut self) -> Result<String, Box<Error>>;
    /// Method to set the clipboard contents as a String
    fn set_text(&mut self, text: String) -> Result<(), Box<Error>>;
    /// Method to get clipboard contents not necessarily string
    fn get_binary_contents(&mut self) -> Result<Option<ClipboardContent>, Box<Error>>;
    fn get_image(&mut self) -> Result<ImageData, Box<Error>>;
}
