use std::{borrow::Cow, path::PathBuf};

use jni::{
	objects::{JObject, JString, JValue},
	AttachGuard, JavaVM,
};

use crate::{Error, ImageData};

impl From<jni::errors::Error> for Error {
	fn from(error: jni::errors::Error) -> Self {
		Error::Unknown { description: error.to_string() }
	}
}

pub(crate) struct Clipboard {
	ctx: ndk_context::AndroidContext,
}

impl Clipboard {
	pub(crate) fn new() -> Result<Self, Error> {
		Ok(Self { ctx: ndk_context::android_context() })
	}

	fn vm(&self) -> Result<JavaVM, jni::errors::Error> {
		// SAFETY: Valid pointer guaranteed by the `ndk_context` crate.
		unsafe { jni::JavaVM::from_raw(self.ctx.vm().cast()) }
	}

	fn context(&self) -> JObject {
		// SAFETY: Valid pointer guaranteed by the `ndk_context` crate.
		unsafe { JObject::from_raw(self.ctx.context().cast()) }
	}

	fn clipboard_manager<'attachment>(
		&self,
		env: &mut AttachGuard<'attachment>,
	) -> Result<JObject<'attachment>, Error> {
		let context = self.context();
		let clipboard = env.new_string("clipboard")?;

		Ok(env
			.call_method(
				context,
				"getSystemService",
				"(Ljava/lang/String;)Ljava/lang/Object;",
				&[(&clipboard).into()],
			)?
			.l()?)
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
		let vm = self.clipboard.vm()?;
		let mut env = vm.attach_current_thread()?;
		let clipboard = self.clipboard.clipboard_manager(&mut env)?;

		// Fetch the primary clip
		let clip_data = env
			.call_method(&clipboard, "getPrimaryClip", "()Landroid/content/ClipData;", &[])?
			.l()?;

		// Take the first item
		let item = env
			.call_method(
				clip_data,
				"getItemAt",
				"(I)Landroid/content/ClipData$Item;",
				&[JValue::from(0)],
			)?
			.l()?;

		// Coerce the item to text
		let char_seq = env
			.call_method(
				item,
				"coerceToText",
				"(Landroid/content/Context;)Ljava/lang/CharSequence;",
				&[JValue::from(&self.clipboard.context())],
			)?
			.l()?;

		// CharSequence.toString() -> String
		let j_str_obj = env.call_method(char_seq, "toString", "()Ljava/lang/String;", &[])?.l()?;
		let j_str = JString::from(j_str_obj); // cast
		let rust_s = env.get_string(&j_str)?.into(); // JavaStr → String

		Ok(rust_s)
	}

	pub(crate) fn html(self) -> Result<String, Error> {
		Err(Error::ClipboardNotSupported)
	}

	pub(crate) fn file_list(self) -> Result<Vec<PathBuf>, Error> {
		Err(Error::ClipboardNotSupported)
	}

	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
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
		let vm = self.clipboard.vm()?;
		let mut env = vm.attach_current_thread()?;
		let clipboard_manager = self.clipboard.clipboard_manager(&mut env)?;

		let label = env.new_string("label")?;
		let text = env.new_string(text)?;

		let clip_data = env.call_static_method(
			"android/content/ClipData",
			"newPlainText",
			"(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
			&[(&label).into(), (&text).into()],
		)?;

		env.call_method(
			clipboard_manager,
			"setPrimaryClip",
			"(Landroid/content/ClipData;)V",
			&[(&clip_data).into()],
		)?;

		Ok(())
	}

	pub(crate) fn html(self, _: Cow<'_, str>, _: Option<Cow<'_, str>>) -> Result<(), Error> {
		Err(Error::ClipboardNotSupported)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self, _: ImageData) -> Result<(), Error> {
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
		let vm = self.clipboard.vm()?;
		let mut env = vm.attach_current_thread()?;
		let clipboard_manager = self.clipboard.clipboard_manager(&mut env)?;

		env.call_method(clipboard_manager, "clearPrimaryClip", "()V", &[])?;

		Ok(())
	}
}
