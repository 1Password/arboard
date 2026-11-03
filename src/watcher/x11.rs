use crate::{watcher::unknown, Error};
use x11rb::{
	connection::{Connection, RequestConnection},
	protocol::{
		xfixes::{ConnectionExt as _, SelectionEventMask},
		xproto::{ConnectionExt as _, CreateWindowAux, WindowClass},
		Event,
	},
	rust_connection::RustConnection,
	COPY_DEPTH_FROM_PARENT,
};

pub(in crate::watcher) struct Backend {
	connection: RustConnection,
	window: u32,
	selection: u32,
}

impl Backend {
	pub(super) fn new() -> Result<Self, Error> {
		let (connection, screen) = x11rb::connect(None).map_err(unknown)?;
		if connection.extension_information("XFIXES").map_err(unknown)?.is_none() {
			return Err(Error::ClipboardNotSupported);
		}
		let version =
			connection.xfixes_query_version(1, 0).map_err(unknown)?.reply().map_err(unknown)?;
		if version.major_version < 1 {
			return Err(Error::ClipboardNotSupported);
		}
		let selection = connection
			.intern_atom(false, b"CLIPBOARD")
			.map_err(unknown)?
			.reply()
			.map_err(unknown)?
			.atom;
		let window = connection.generate_id().map_err(unknown)?;
		connection
			.create_window(
				COPY_DEPTH_FROM_PARENT,
				window,
				connection.setup().roots[screen].root,
				0,
				0,
				1,
				1,
				0,
				WindowClass::INPUT_ONLY,
				0,
				&CreateWindowAux::new(),
			)
			.map_err(unknown)?
			.check()
			.map_err(unknown)?;
		connection
			.xfixes_select_selection_input(
				window,
				selection,
				SelectionEventMask::SET_SELECTION_OWNER
					| SelectionEventMask::SELECTION_WINDOW_DESTROY
					| SelectionEventMask::SELECTION_CLIENT_CLOSE,
			)
			.map_err(unknown)?
			.check()
			.map_err(unknown)?;
		Ok(Self { connection, window, selection })
	}

	pub(super) fn poll(&mut self) -> Result<bool, Error> {
		let mut changed = false;
		// Bound each pass so a busy native event stream cannot starve stop.
		for _ in 0..256 {
			match self.connection.poll_for_event().map_err(unknown)? {
				Some(Event::XfixesSelectionNotify(event))
					if event.window == self.window && event.selection == self.selection =>
				{
					changed = true;
				}
				Some(Event::Error(error)) => return Err(unknown(format!("{error:?}"))),
				Some(_) => {}
				None => break,
			}
		}
		Ok(changed)
	}
}
