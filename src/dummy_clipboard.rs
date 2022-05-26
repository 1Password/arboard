use crate::Error;
#[cfg(feature = "image-data")]
use crate::ImageData;

#[derive(Default, Debug)]
pub struct DummyClipboard {}

impl DummyClipboard {
    pub fn new() -> Result<Self, Error> {
        Ok(DummyClipboard::default())
    }

    /// Fetches utf-8 text from the clipboard and returns it.
    pub fn get_text(&mut self) -> Result<String, Error> {
        Err(Error::ContentNotAvailable)
    }

    /// Places the text onto the clipboard. Any valid utf-8 string is accepted.
    pub fn set_text(&mut self, _text: String) -> Result<(), Error> {
        Err(Error::ClipboardNotSupported)
    }

    /// Fetches image data from the clipboard, and returns the decoded pixels.
    ///
    /// Any image data placed on the clipboard with `set_image` will be possible read back, using
    /// this function. However it's of not guaranteed that an image placed on the clipboard by any
    /// other application will be of a supported format.
    #[cfg(feature = "image-data")]
    pub fn get_image(&mut self) -> Result<ImageData<'static>, Error> {
        Err(Error::ContentNotAvailable)
    }

    /// Places an image to the clipboard.
    ///
    /// The chosen output format, depending on the platform is the following:
    ///
    /// - On macOS: `NSImage` object
    /// - On Linux: PNG, under the atom `image/png`
    /// - On Windows: In order of priority `CF_DIB` and `CF_BITMAP`
    #[cfg(feature = "image-data")]
    pub fn set_image(&mut self, _image: ImageData) -> Result<(), Error> {
        Err(Error::ClipboardNotSupported)
    }
}
