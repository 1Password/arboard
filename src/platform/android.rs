use std::{
	borrow::Cow,
	path::{Path, PathBuf},
};

use jni::{
	jni_sig, jni_str,
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
				jni_str!("getSystemService"),
				jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"),
				&[(&clipboard).into()],
			)?
			.l()?;

		callback(env, clipboard_manager)
	})
}

pub(crate) struct Clipboard(());

impl Clipboard {
	pub(crate) fn new() -> Result<Self, Error> {
		with_clipboard_access(|env, _| {
			let version_class = env.load_class(jni_str!("android/os/Build$VERSION"))?;
			let build_sdk =
				env.get_static_field(version_class, jni_str!("SDK_INT"), jni_sig!("I"))?.i()?;

			// clearPrimaryClip was introduced in this version
			if build_sdk >= 28 {
				Ok(Self(()))
			} else {
				Err(Error::ClipboardNotSupported)
			}
		})
	}
}

pub(crate) struct Get<'clipboard> {
	_clipboard: &'clipboard Clipboard,
}

impl<'clipboard> Get<'clipboard> {
	pub(crate) fn new(_clipboard: &'clipboard mut Clipboard) -> Self {
		Self { _clipboard }
	}

	pub(crate) fn text(self) -> Result<String, Error> {
		with_clipboard_access(|env, clipboard_manager| {
			if !env
				.call_method(&clipboard_manager, jni_str!("hasPrimaryClip"), jni_sig!("()Z"), &[])?
				.z()?
			{
				return Err(Error::ContentNotAvailable);
			}

			let clip = env
				.call_method(
					clipboard_manager,
					jni_str!("getPrimaryClip"),
					jni_sig!("()Landroid/content/ClipData;"),
					&[],
				)?
				.l()?;

			if env.call_method(&clip, jni_str!("getItemCount"), jni_sig!("()I"), &[])?.i()? == 0 {
				return Err(Error::ContentNotAvailable);
			}

			let item = env
				.call_method(
					&clip,
					jni_str!("getItemAt"),
					jni_sig!("(I)Landroid/content/ClipData$Item;"),
					&[0.into()],
				)?
				.l()?;

			let char_sequence = env
				.call_method(
					item,
					jni_str!("getText"),
					jni_sig!("()Ljava/lang/CharSequence;"),
					&[],
				)?
				.l()?;
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
	_clipboard: &'clipboard mut Clipboard,
}

impl<'clipboard> Set<'clipboard> {
	pub(crate) fn new(_clipboard: &'clipboard mut Clipboard) -> Self {
		Self { _clipboard }
	}

	pub(crate) fn text(self, text: Cow<'_, str>) -> Result<(), Error> {
		with_clipboard_access(|env, clipboard_manager| {
			let label = env.new_string("label")?;
			let text = env.new_string(text)?;

			let clip_data = env.call_static_method(
				jni_str!("android/content/ClipData"),
				jni_str!("newPlainText"),
				jni_sig!(
					"(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;"
				),
				&[(&label).into(), (&text).into()],
			)?;

			env.call_method(
				clipboard_manager,
				jni_str!("setPrimaryClip"),
				jni_sig!("(Landroid/content/ClipData;)V"),
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
	_clipboard: &'clipboard mut Clipboard,
}

impl<'clipboard> Clear<'clipboard> {
	pub(crate) fn new(_clipboard: &'clipboard mut Clipboard) -> Self {
		Self { _clipboard }
	}

	pub(crate) fn clear(self) -> Result<(), Error> {
		with_clipboard_access(|env, clipboard_manager| {
			env.call_method(clipboard_manager, jni_str!("clearPrimaryClip"), jni_sig!("()V"), &[])?;
			Ok(())
		})
	}
}
