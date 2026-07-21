use std::{
	borrow::Cow,
	marker::PhantomData,
	path::{Path, PathBuf},
};

use crate::common::Error;
#[cfg(feature = "image-data")]
use crate::ImageData;

pub(crate) struct Clipboard;

impl Clipboard {
	pub(crate) fn new() -> Result<Self, Error> {
		Err(Error::ClipboardNotSupported)
	}
}

pub(crate) struct Clear<'a> {
	_p: PhantomData<&'a ()>,
}

impl<'a> Clear<'a> {
	pub(crate) fn new(_clipboard: &'a mut Clipboard) -> Self {
		Self { _p: PhantomData }
	}

	pub(crate) fn clear(self) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}
}

pub(crate) struct Set<'a> {
	_p: PhantomData<&'a ()>,
}

impl<'a> Set<'a> {
	pub(crate) fn new(_clipboard: &'a mut Clipboard) -> Self {
		Self { _p: PhantomData }
	}

	pub(crate) fn text(self, _text: Cow<'_, str>) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}

	pub(crate) fn html(self, _html: Cow<'_, str>, _alt: Option<Cow<'_, str>>) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self, _image: ImageData<'_>) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}

	pub(crate) fn file_list(self, _file_list: &[impl AsRef<Path>]) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}
}

pub(crate) struct Get<'a> {
	_p: PhantomData<&'a ()>,
}

impl<'a> Get<'a> {
	pub(crate) fn new(_clipboard: &'a mut Clipboard) -> Self {
		Self { _p: PhantomData }
	}

	pub(crate) fn text(self) -> Result<String, Error> {
		Err(Error::ContentNotAvailable)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
		Err(Error::ContentNotAvailable)
	}

	pub(crate) fn html(self) -> Result<String, Error> {
		Err(Error::ContentNotAvailable)
	}

	pub(crate) fn file_list(self) -> Result<Vec<PathBuf>, Error> {
		Err(Error::ContentNotAvailable)
	}
}
