use std::{
	borrow::Cow,
	path::{Path, PathBuf},
};

use jni::{
	objects::{JObject, JString},
	Env, JavaVM,
};

#[cfg(feature = "image-data")]
use crate::common::ImageData;
use crate::Error;

impl From<jni::errors::Error> for Error {
	fn from(error: jni::errors::Error) -> Self {
		Error::Unknown { description: error.to_string() }
	}
}

fn with_clipboard_access<F, T>(callback: F) -> Result<T, Error>
where
	F: FnOnce(&mut Env, JObject) -> Result<T, Error>,
{
	let ctx = ndk_context::android_context();

	let jvm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };

	jvm.attach_current_thread(|env| {
		let context = unsafe { JObject::from_raw(env, ctx.context().cast()) };
		let clipboard = env.new_string("clipboard")?;

		let clipboard_manager = env
			.call_method(
				context,
				c"getSystemService",
				c"(Ljava/lang/String;)Ljava/lang/Object;",
				&[(&clipboard).into()],
			)?
			.l()?;

		callback(env, clipboard_manager)
	})
}

pub(crate) struct Clipboard(());

impl Clipboard {
	pub(crate) fn new() -> Result<Self, Error> {
		Ok(Self(()))
	}
}

pub(crate) struct Get<'clipboard> {
	clipboard: &'clipboard Clipboard,
}

impl<'clipboard> Get<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn text(self) -> Result<String, Error> {
		with_clipboard_access(|env, clipboard_manager| {
			if !env.call_method(&clipboard_manager, c"hasPrimaryClip", c"()Z", &[])?.z()? {
				return Err(Error::ContentNotAvailable);
			}

			let clip = env
				.call_method(
					clipboard_manager,
					c"getPrimaryClip",
					c"()Landroid/content/ClipData;",
					&[],
				)?
				.l()?;

			if env.call_method(&clip, c"getItemCount", c"()I", &[])?.i()? == 0 {
				return Err(Error::ContentNotAvailable);
			}

			let item = env
				.call_method(
					&clip,
					c"getItemAt",
					c"(I)Landroid/content/ClipData$Item;",
					&[0.into()],
				)?
				.l()?;

			let char_sequence =
				env.call_method(item, c"getText", c"()Ljava/lang/CharSequence;", &[])?.l()?;
			let text = env.cast_local::<JString>(char_sequence)?.to_string();

			Ok(text)
		})
	}

	pub(crate) fn html(self) -> Result<String, Error> {
		Err(Error::ClipboardNotSupported)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
		Err(Error::ClipboardNotSupported)
	}

	pub(crate) fn file_list(self) -> Result<Vec<PathBuf>, Error> {
		Err(Error::ClipboardNotSupported)
	}
}

pub(crate) struct Set<'clipboard> {
	clipboard: &'clipboard mut Clipboard,
}

impl<'clipboard> Set<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn text(self, text: Cow<'_, str>) -> Result<(), Error> {
		with_clipboard_access(|env, clipboard_manager| {
			let label = env.new_string("label")?;
			let text = env.new_string(text)?;

			let clip_data = env.call_static_method(
				c"android/content/ClipData",
				c"newPlainText",
				c"(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
				&[(&label).into(), (&text).into()],
			)?;

			env.call_method(
				clipboard_manager,
				c"setPrimaryClip",
				c"(Landroid/content/ClipData;)V",
				&[(&clip_data).into()],
			)?;

			Ok(())
		})
	}

	pub(crate) fn html(self, _: Cow<'_, str>, _: Option<Cow<'_, str>>) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self, _: ImageData) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}

	pub(crate) fn file_list(self, _: &[impl AsRef<Path>]) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}
}

pub(crate) struct Clear<'clipboard> {
	clipboard: &'clipboard mut Clipboard,
}

impl<'clipboard> Clear<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn clear(self) -> Result<(), Error> {
		with_clipboard_access(|env, clipboard_manager| {
			env.call_method(clipboard_manager, c"clearPrimaryClip", c"()V", &[])?;
			Ok(())
		})
	}
}
