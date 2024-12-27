use std::borrow::Cow;

use jni::objects::{JObject, JString};

use crate::{Error, ImageData};

impl From<jni::errors::Error> for Error {
	fn from(error: jni::errors::Error) -> Self {
		Error::Unknown { description: error.to_string() }
	}
}

pub(crate) struct Clipboard {}

impl Clipboard {
	pub(crate) fn new() -> Result<Self, Error> {
		Ok(Self {})
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
		let ctx = ndk_context::android_context();
		let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.unwrap();
		let context = unsafe { JObject::from_raw(ctx.context().cast()) };
		let mut env = vm.attach_current_thread().unwrap();

		let clipboard = env.new_string("clipboard")?;

		let clipboard_manager = env
			.call_method(
				context,
				"getSystemService",
				"(Ljava/lang/String;)Ljava/lang/Object;",
				&[(&clipboard).into()],
			)?
			.l()?;

		let clip = env
			.call_method(clipboard_manager, "getPrimaryClip", "()Landroid/content/ClipData;", &[])?
			.l()?;

		let item = env
			.call_method(clip, "getItemAt", "(I)Landroid/content/ClipData$Item;", &[0.into()])?
			.l()?;

		let text = env.call_method(item, "getText", "()Ljava/lang/CharSequence;", &[])?.l()?;

		let text = JString::from(text);
		let text = env.get_string(&text)?;

		Ok(text.into())
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
		let ctx = ndk_context::android_context();
		let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.unwrap();
		let context = unsafe { JObject::from_raw(ctx.context().cast()) };
		let mut env = vm.attach_current_thread().unwrap();

		let clipboard = env.new_string("clipboard")?;

		let clipboard_manager = env
			.call_method(
				context,
				"getSystemService",
				"(Ljava/lang/String;)Ljava/lang/Object;",
				&[(&clipboard).into()],
			)?
			.l()?;

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
		Err(Error::ClipboardNotSupported)
	}
}
