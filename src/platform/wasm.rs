#[cfg(feature = "image-data")]
use crate::common::ImageData;
use crate::common::Error;
use js_sys::wasm_bindgen::JsCast;
use std::borrow::Cow;

pub(crate) struct Clipboard {
    inner: web_sys::Clipboard,
    window: web_sys::Window,
    _paste_callback: web_sys::wasm_bindgen::closure::Closure<dyn FnMut(web_sys::ClipboardEvent)>
}

impl Clipboard {
    const GLOBAL_CLIPBOARD_OBJECT: &str = "__arboard_global_clipboard";

    pub(crate) fn new() -> Result<Self, Error> {
        let window = web_sys::window().ok_or(Error::ClipboardNotSupported)?;
        let inner = window.navigator().clipboard();
        
        let window_clone = window.clone();
        let paste_callback = web_sys::wasm_bindgen::closure::Closure::wrap(Box::new(move |e: web_sys::ClipboardEvent| {
            if let Some(data_transfer) = e.clipboard_data() {
                js_sys::Reflect::set(&window_clone, &Self::GLOBAL_CLIPBOARD_OBJECT.into(), &data_transfer.get_data("text").unwrap_or_default().into())
                    .expect("Failed to set global clipboard object.");
            }
        }) as Box<dyn FnMut(_)>);
        
        // Set this event handler to execute before any child elements (third argument `true`) so that it is subsequently observed by other events.
        window.document().ok_or(Error::ClipboardNotSupported)?.add_event_listener_with_callback_and_bool("paste", &paste_callback.as_ref().unchecked_ref(), true)
            .map_err(|_| Error::unknown("Could not add paste event listener."))?;

        Ok(Self {
            inner,
            _paste_callback: paste_callback,
            window
        })
    }

    fn get_last_clipboard(&self) -> String {
        js_sys::Reflect::get(&self.window, &Self::GLOBAL_CLIPBOARD_OBJECT.into())
            .ok().and_then(|x| x.as_string()).unwrap_or_default()
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
        let _ = self.clipboard.inner.write(&js_sys::Array::default());
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
        Ok(self.clipboard.get_last_clipboard())
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
		Self {
			clipboard
		}
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
        
        self.clipboard.set_last_clipboard(&html);
        let html_item = js_sys::Object::new();
        js_sys::Reflect::set(&html_item, &"text/html".into(), &html.into_owned().into())
            .expect("Failed to set HTML item text.");

        let alt_item = js_sys::Object::new();
        js_sys::Reflect::set(&alt_item, &"text/plain".into(), &alt.into())
            .expect("Failed to set alt item text.");

        let mut clipboard_items = js_sys::Array::default();
        clipboard_items.extend([
            web_sys::ClipboardItem::new_with_record_from_str_to_str_promise(&html_item)
                .map_err(|_| Error::unknown("Failed to create HTML clipboard item."))?,
            web_sys::ClipboardItem::new_with_record_from_str_to_str_promise(&alt_item)
                .map_err(|_| Error::unknown("Failed to create alt clipboard item."))?
        ]);
        
        let _ = self.clipboard.inner.write(&clipboard_items);
        Ok(())
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn image(self, _: ImageData) -> Result<(), Error> {
        Err(Error::ConversionFailure)
	}
}