use crate::common::Error;
#[cfg(feature = "image-data")]
use crate::common::ImageData;
use js_sys::wasm_bindgen::JsCast;
use std::borrow::Cow;
use web_sys::wasm_bindgen::closure::Closure;

pub(crate) struct Clipboard {
	inner: web_sys::Clipboard,
	window: web_sys::Window,
}

impl Clipboard {
	const GLOBAL_CLIPBOARD_OBJECT: &str = "__arboard_global_clipboard";
	const GLOBAL_CALLBACK_OBJECT: &str = "__arboard_global_callback";

	pub(crate) fn new() -> Result<Self, Error> {
		let window = web_sys::window().ok_or(Error::ClipboardNotSupported)?;
		let inner = window.navigator().clipboard();

		// If the clipboard is being opened for the first time, add a paste callback
		if js_sys::Reflect::get(&window, &Self::GLOBAL_CALLBACK_OBJECT.into())
			.map_err(|_| Error::ClipboardNotSupported)?
			.is_falsy()
		{
			let window_clone = window.clone();

			let paste_callback = Closure::wrap(Box::new(move |e: web_sys::ClipboardEvent| {
				if let Some(data_transfer) = e.clipboard_data() {
					let object_to_set = if let Ok(text_data) = data_transfer.get_data("text") {
						text_data.into()
					} else {
						web_sys::wasm_bindgen::JsValue::NULL.clone()
					};

					js_sys::Reflect::set(
						&window_clone,
						&Self::GLOBAL_CLIPBOARD_OBJECT.into(),
						&object_to_set,
					)
					.expect("Failed to set global clipboard object.");
				}
			}) as Box<dyn FnMut(_)>);

			// Set this event handler to execute before any child elements (third argument `true`) so that it is subsequently observed by other events.
			window
				.document()
				.ok_or(Error::ClipboardNotSupported)?
				.add_event_listener_with_callback_and_bool(
					"paste",
					&paste_callback.as_ref().unchecked_ref(),
					true,
				)
				.map_err(|_| Error::unknown("Could not add paste event listener."))?;

			js_sys::Reflect::set(
				&window,
				&Self::GLOBAL_CALLBACK_OBJECT.into(),
				&web_sys::wasm_bindgen::JsValue::TRUE,
			)
			.expect("Failed to set global callback flag.");

			paste_callback.forget();
		}

		Ok(Self { inner, window })
	}

	fn get_last_clipboard(&self) -> Option<String> {
		js_sys::Reflect::get(&self.window, &Self::GLOBAL_CLIPBOARD_OBJECT.into())
			.ok()
			.and_then(|x| x.as_string())
	}

	fn set_last_clipboard(&self, value: &str) {
		js_sys::Reflect::set(&self.window, &Self::GLOBAL_CLIPBOARD_OBJECT.into(), &value.into())
			.expect("Failed to set global clipboard object.");
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
		let _ = self.clipboard.inner.write_text("");
		self.clipboard.set_last_clipboard("");
		Ok(())
	}
}

pub(crate) struct Get<'clipboard> {
	clipboard: &'clipboard mut Clipboard,
}

impl<'clipboard> Get<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn text(self) -> Result<String, Error> {
		self.clipboard.get_last_clipboard().ok_or_else(|| Error::ContentNotAvailable)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self) -> Result<ImageData<'static>, Error> {
		Err(Error::ConversionFailure)
	}
}

pub(crate) struct Set<'clipboard> {
	clipboard: &'clipboard mut Clipboard,
}

impl<'clipboard> Set<'clipboard> {
	pub(crate) fn new(clipboard: &'clipboard mut Clipboard) -> Self {
		Self { clipboard }
	}

	pub(crate) fn text(self, data: Cow<'_, str>) -> Result<(), Error> {
		let _ = self.clipboard.inner.write_text(&data);
		self.clipboard.set_last_clipboard(&data);
		Ok(())
	}

	pub(crate) fn html(self, html: Cow<'_, str>, alt: Option<Cow<'_, str>>) -> Result<(), Error> {
		let alt = match alt {
			Some(s) => s.into(),
			None => String::new(),
		};

		let html_item = js_sys::Object::new();
		js_sys::Reflect::set(&html_item, &"text/html".into(), &(&*html).into())
			.expect("Failed to set HTML item text.");

		let alt_item = js_sys::Object::new();
		js_sys::Reflect::set(&alt_item, &"text/plain".into(), &alt.into())
			.expect("Failed to set alt item text.");

		let mut clipboard_items = js_sys::Array::default();
		clipboard_items.extend([
			web_sys::ClipboardItem::new_with_record_from_str_to_str_promise(&html_item)
				.map_err(|_| Error::unknown("Failed to create HTML clipboard item."))?,
			web_sys::ClipboardItem::new_with_record_from_str_to_str_promise(&alt_item)
				.map_err(|_| Error::unknown("Failed to create alt clipboard item."))?,
		]);

		let _ = self.clipboard.inner.write(&clipboard_items);
		self.clipboard.set_last_clipboard(&html);
		Ok(())
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self, _: ImageData) -> Result<(), Error> {
		Err(Error::ConversionFailure)
	}
}
