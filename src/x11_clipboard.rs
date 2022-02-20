/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

// More info about using the clipboard on X11:
// https://tronche.com/gui/x/icccm/sec-2.html#s-2.6
// https://freedesktop.org/wiki/ClipboardManager/

use std::{
	cell::RefCell,
	collections::{hash_map::Entry, HashMap},
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	thread::JoinHandle,
	thread_local,
	time::{Duration, Instant},
	usize,
};

use log::{error, trace, warn};
use once_cell::sync::Lazy;
use parking_lot::{Condvar, Mutex, MutexGuard, RwLock};
use x11rb::{
	connection::Connection,
	protocol::{
		xproto::{
			Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, Property,
			PropertyNotifyEvent, SelectionNotifyEvent, SelectionRequestEvent, Time, WindowClass,
			SELECTION_NOTIFY_EVENT,
		},
		Event,
	},
	rust_connection::RustConnection,
	wrapper::ConnectionExt as _,
	COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, NONE,
};

#[cfg(feature = "image-data")]
use crate::{common_linux::encode_as_png, ImageData};
use crate::{common_linux::into_unknown, Error, LinuxClipboardKind};

type Result<T, E = Error> = std::result::Result<T, E>;

static CLIPBOARD: Lazy<Mutex<Option<GlobalClipboard>>> = Lazy::new(|| Mutex::new(None));

x11rb::atom_manager! {
	pub Atoms: AtomCookies {
		CLIPBOARD,
		PRIMARY,
		SECONDARY,

		CLIPBOARD_MANAGER,
		SAVE_TARGETS,
		TARGETS,
		ATOM,
		INCR,

		UTF8_STRING,
		UTF8_MIME_0: b"text/plain;charset=utf-8",
		UTF8_MIME_1: b"text/plain;charset=UTF-8",
		// Text in ISO Latin-1 encoding
		// See: https://tronche.com/gui/x/icccm/sec-2.html#s-2.6.2
		STRING,
		// Text in unknown encoding
		// See: https://tronche.com/gui/x/icccm/sec-2.html#s-2.6.2
		TEXT,
		TEXT_MIME_UNKNOWN: b"text/plain",

		PNG_MIME: b"image/png",

		// This is just some random name for the property on our window, into which
		// the clipboard owner writes the data we requested.
		ARBOARD_CLIPBOARD,
	}
}

thread_local! {
	static ATOM_NAME_CACHE: RefCell<HashMap<Atom, &'static str>> = Default::default();
}

// Some clipboard items, like images, may take a very long time to produce a
// `SelectionNotify`. Multiple seconds long.
const LONG_TIMEOUT_DUR: Duration = Duration::from_millis(4000);
const SHORT_TIMEOUT_DUR: Duration = Duration::from_millis(10);

#[derive(Debug, PartialEq, Eq)]
enum ManagerHandoverState {
	Idle,
	InProgress,
	Finished,
}

struct GlobalClipboard {
	context: Arc<ClipboardContext>,

	/// Join handle to the thread which serves selection requests.
	server_handle: JoinHandle<()>,
}

struct XContext {
	conn: RustConnection,
	win_id: u32,
}

struct ClipboardContext {
	/// The context for the thread which serves clipboard read
	/// requests coming to us.
	server: XContext,
	atoms: Atoms,
	clipboard_data: RwLock<Option<ClipboardData>>,
	primary_data: RwLock<Option<ClipboardData>>,
	secondary_data: RwLock<Option<ClipboardData>>,

	handover_state: Mutex<ManagerHandoverState>,
	handover_cv: Condvar,

	serve_stopped: AtomicBool,
}

impl XContext {
	fn new() -> Result<Self> {
		// create a new connection to an X11 server
		let (conn, screen_num): (RustConnection, _) =
			RustConnection::connect(None).map_err(into_unknown)?;
		let screen = conn
			.setup()
			.roots
			.get(screen_num)
			.ok_or(Error::Unknown { description: String::from("no screen found") })?;
		let win_id = conn.generate_id().map_err(into_unknown)?;

		let event_mask =
            // Just in case that some program reports SelectionNotify events
            // with XCB_EVENT_MASK_PROPERTY_CHANGE mask.
            EventMask::PROPERTY_CHANGE |
            // To receive DestroyNotify event and stop the message loop.
            EventMask::STRUCTURE_NOTIFY;
		// create the window
		conn.create_window(
			// copy as much as possible from the parent, because no other specific input is needed
			COPY_DEPTH_FROM_PARENT,
			win_id,
			screen.root,
			0,
			0,
			1,
			1,
			0,
			WindowClass::COPY_FROM_PARENT,
			COPY_FROM_PARENT,
			// don't subscribe to any special events because we are requesting everything we need ourselves
			&CreateWindowAux::new().event_mask(event_mask),
		)
		.map_err(into_unknown)?;
		conn.flush().map_err(into_unknown)?;

		Ok(Self { conn, win_id })
	}
}

#[derive(Debug, Clone)]
struct ClipboardData {
	bytes: Vec<u8>,

	/// The atom represeting the format in which the data is encoded.
	format: Atom,
}

enum ReadSelNotifyResult {
	GotData(Vec<u8>),
	IncrStarted,
	EventNotRecognized,
}

impl ClipboardContext {
	fn new() -> Result<Self> {
		let server = XContext::new()?;
		let atoms =
			Atoms::new(&server.conn).map_err(into_unknown)?.reply().map_err(into_unknown)?;

		Ok(Self {
			server,
			atoms,
			clipboard_data: RwLock::default(),
			primary_data: RwLock::default(),
			secondary_data: RwLock::default(),
			handover_state: Mutex::new(ManagerHandoverState::Idle),
			handover_cv: Condvar::new(),
			serve_stopped: AtomicBool::new(false),
		})
	}

	fn write(&self, data: ClipboardData, selection: LinuxClipboardKind) -> Result<()> {
		if self.serve_stopped.load(Ordering::Relaxed) {
			return Err(Error::Unknown {
                description: "The clipboard handler thread seems to have stopped. Logging messages may reveal the cause. (See the `log` crate.)".into()
            });
		}

		let server_win = self.server.win_id;

		// ICCCM version 2, section 2.6.1.3 states that we should re-assert ownership whenever data
		// changes.
		self.server
			.conn
			.set_selection_owner(server_win, self.atom_of(selection), Time::CURRENT_TIME)
			.map_err(|_| Error::ClipboardOccupied)?;

		self.server.conn.flush().map_err(into_unknown)?;

		// Just setting the data, and the `serve_requests` will take care of the rest.
		*self.data_of(selection).write() = Some(data);

		Ok(())
	}

	/// `formats` must be a slice of atoms, where each atom represents a target format.
	/// The first format from `formats`, which the clipboard owner supports will be the
	/// format of the return value.
	fn read(&self, formats: &[Atom], selection: LinuxClipboardKind) -> Result<ClipboardData> {
		// if we are the current owner, we can get the current clipboard ourselves
		if self.is_owner(selection)? {
			let data = self.data_of(selection).read();
			if let Some(data) = &*data {
				for format in formats {
					if *format == data.format {
						return Ok(data.clone());
					}
				}
			}
			return Err(Error::ContentNotAvailable);
		}
		// if let Some(data) = self.data.read().clone() {
		//     return Ok(data)
		// }
		let reader = XContext::new()?;

		trace!("Trying to get the clipboard data.");
		for format in formats {
			match self.read_single(&reader, selection, *format) {
				Ok(bytes) => {
					return Ok(ClipboardData { bytes, format: *format });
				}
				Err(Error::ContentNotAvailable) => {
					continue;
				}
				Err(e) => return Err(e),
			}
		}
		Err(Error::ContentNotAvailable)
	}

	fn read_single(
		&self,
		reader: &XContext,
		selection: LinuxClipboardKind,
		target_format: Atom,
	) -> Result<Vec<u8>> {
		// Delete the property so that we can detect (using property notify)
		// when the selection owner receives our request.
		reader
			.conn
			.delete_property(reader.win_id, self.atoms.ARBOARD_CLIPBOARD)
			.map_err(into_unknown)?;

		// request to convert the clipboard selection to our data type(s)
		reader
			.conn
			.convert_selection(
				reader.win_id,
				self.atom_of(selection),
				target_format,
				self.atoms.ARBOARD_CLIPBOARD,
				Time::CURRENT_TIME,
			)
			.map_err(into_unknown)?;
		reader.conn.sync().map_err(into_unknown)?;

		trace!("Finished `convert_selection`");

		let mut incr_data: Vec<u8> = Vec::new();
		let mut using_incr = false;

		let mut timeout_end = Instant::now() + LONG_TIMEOUT_DUR;

		while Instant::now() < timeout_end {
			let event = reader.conn.poll_for_event().map_err(into_unknown)?;
			let event = match event {
				Some(e) => e,
				None => {
					std::thread::sleep(Duration::from_millis(1));
					continue;
				}
			};
			match event {
				// The first response after requesting a selection.
				Event::SelectionNotify(event) => {
					trace!("Read SelectionNotify");
					let result = self.handle_read_selection_notify(
						&reader,
						target_format,
						&mut using_incr,
						&mut incr_data,
						event,
					)?;
					match result {
						ReadSelNotifyResult::GotData(data) => return Ok(data),
						ReadSelNotifyResult::IncrStarted => {
							// This means we received an indication that an the
							// data is going to be sent INCRementally. Let's
							// reset our timeout.
							timeout_end += SHORT_TIMEOUT_DUR;
						}
						ReadSelNotifyResult::EventNotRecognized => (),
					}
				}
				// If the previous SelectionNotify event specified that the data
				// will be sent in INCR segments, each segment is transferred in
				// a PropertyNotify event.
				Event::PropertyNotify(event) => {
					let result = self.handle_read_property_notify(
						&reader,
						target_format,
						&mut using_incr,
						&mut incr_data,
						&mut timeout_end,
						event,
					)?;
					if result {
						return Ok(incr_data);
					}
				}
				_ => log::trace!("An unexpected event arrived while reading the clipboard."),
			}
		}
		log::info!("Time-out hit while reading the clipboard.");
		Err(Error::ContentNotAvailable)
	}

	fn atom_of(&self, selection: LinuxClipboardKind) -> Atom {
		match selection {
			LinuxClipboardKind::Clipboard => self.atoms.CLIPBOARD,
			LinuxClipboardKind::Primary => self.atoms.PRIMARY,
			LinuxClipboardKind::Secondary => self.atoms.SECONDARY,
		}
	}

	fn data_of(&self, selection: LinuxClipboardKind) -> &RwLock<Option<ClipboardData>> {
		match selection {
			LinuxClipboardKind::Clipboard => &self.clipboard_data,
			LinuxClipboardKind::Primary => &self.primary_data,
			LinuxClipboardKind::Secondary => &self.secondary_data,
		}
	}

	fn kind_of(&self, atom: Atom) -> Option<LinuxClipboardKind> {
		match atom {
			a if a == self.atoms.CLIPBOARD => Some(LinuxClipboardKind::Clipboard),
			a if a == self.atoms.PRIMARY => Some(LinuxClipboardKind::Primary),
			a if a == self.atoms.SECONDARY => Some(LinuxClipboardKind::Secondary),
			_ => None,
		}
	}

	fn is_owner(&self, selection: LinuxClipboardKind) -> Result<bool> {
		let current = self
			.server
			.conn
			.get_selection_owner(self.atom_of(selection))
			.map_err(into_unknown)?
			.reply()
			.map_err(into_unknown)?
			.owner;

		Ok(current == self.server.win_id)
	}

	fn atom_name(&self, atom: x11rb::protocol::xproto::Atom) -> Result<String> {
		String::from_utf8(
			self.server
				.conn
				.get_atom_name(atom)
				.map_err(into_unknown)?
				.reply()
				.map_err(into_unknown)?
				.name,
		)
		.map_err(into_unknown)
	}
	fn atom_name_dbg(&self, atom: x11rb::protocol::xproto::Atom) -> &'static str {
		ATOM_NAME_CACHE.with(|cache| {
			let mut cache = cache.borrow_mut();
			match cache.entry(atom) {
				Entry::Occupied(entry) => *entry.get(),
				Entry::Vacant(entry) => {
					let s = self
						.atom_name(atom)
						.map(|s| Box::leak(s.into_boxed_str()) as &str)
						.unwrap_or("FAILED-TO-GET-THE-ATOM-NAME");
					entry.insert(s);
					s
				}
			}
		})
	}

	fn handle_read_selection_notify(
		&self,
		reader: &XContext,
		target_format: u32,
		using_incr: &mut bool,
		incr_data: &mut Vec<u8>,
		event: SelectionNotifyEvent,
	) -> Result<ReadSelNotifyResult> {
		// The property being set to NONE means that the `convert_selection`
		// failed.

		// According to: https://tronche.com/gui/x/icccm/sec-2.html#s-2.4
		// the target must be set to the same as what we requested.
		if event.property == NONE || event.target != target_format {
			return Err(Error::ContentNotAvailable);
		}
		if self.kind_of(event.selection).is_none() {
			log::info!("Received a SelectionNotify for a selection other than CLIPBOARD, PRIMARY or SECONDARY. This is unexpected.");
			return Ok(ReadSelNotifyResult::EventNotRecognized);
		}
		if *using_incr {
			log::warn!("Received a SelectionNotify while already expecting INCR segments.");
			return Ok(ReadSelNotifyResult::EventNotRecognized);
		}
		// request the selection
		let mut reply = reader
			.conn
			.get_property(true, event.requestor, event.property, event.target, 0, u32::MAX / 4)
			.map_err(into_unknown)?
			.reply()
			.map_err(into_unknown)?;

		// trace!("Property.type: {:?}", self.atom_name(reply.type_));

		// we found something
		if reply.type_ == target_format {
			Ok(ReadSelNotifyResult::GotData(reply.value))
		} else if reply.type_ == self.atoms.INCR {
			// Note that we call the get_property again because we are
			// indicating that we are ready to receive the data by deleting the
			// property, however deleting only works if the type matches the
			// property type. But the type didn't match in the previous call.
			reply = reader
				.conn
				.get_property(
					true,
					event.requestor,
					event.property,
					self.atoms.INCR,
					0,
					u32::MAX / 4,
				)
				.map_err(into_unknown)?
				.reply()
				.map_err(into_unknown)?;
			log::trace!("Receiving INCR segments");
			*using_incr = true;
			if reply.value_len == 4 {
				let min_data_len = reply.value32().and_then(|mut vals| vals.next()).unwrap_or(0);
				incr_data.reserve(min_data_len as usize);
			}
			Ok(ReadSelNotifyResult::IncrStarted)
		} else {
			// this should never happen, we have sent a request only for supported types
			Err(Error::Unknown {
				description: String::from("incorrect type received from clipboard"),
			})
		}
	}

	/// Returns Ok(true) when the incr_data is ready
	fn handle_read_property_notify(
		&self,
		reader: &XContext,
		target_format: u32,
		using_incr: &mut bool,
		incr_data: &mut Vec<u8>,
		timeout_end: &mut Instant,
		event: PropertyNotifyEvent,
	) -> Result<bool> {
		if event.atom != self.atoms.ARBOARD_CLIPBOARD || event.state != Property::NEW_VALUE {
			return Ok(false);
		}
		if !*using_incr {
			// This must mean the selection owner received our request, and is
			// now preparing the data
			return Ok(false);
		}
		let reply = reader
			.conn
			.get_property(true, event.window, event.atom, target_format, 0, u32::MAX / 4)
			.map_err(into_unknown)?
			.reply()
			.map_err(into_unknown)?;

		// log::trace!("Received segment. value_len {}", reply.value_len,);
		if reply.value_len == 0 {
			// This indicates that all the data has been sent.
			return Ok(true);
		}
		incr_data.extend(reply.value);

		// Let's reset our timeout, since we received a valid chunk.
		*timeout_end = Instant::now() + SHORT_TIMEOUT_DUR;

		// Not yet complete
		Ok(false)
	}

	fn handle_selection_request(&self, event: SelectionRequestEvent) -> Result<()> {
		let selection = match self.kind_of(event.selection) {
			Some(kind) => kind,
			None => {
				warn!("Received a selection request to a selection other than the CLIPBOARD, PRIMARY or SECONDARY. This is unexpected.");
				return Ok(());
			}
		};

		let success;
		// we are asked for a list of supported conversion targets
		if event.target == self.atoms.TARGETS {
			trace!("Handling TARGETS, dst property is {}", self.atom_name_dbg(event.property));
			let mut targets = Vec::with_capacity(10);
			targets.push(self.atoms.TARGETS);
			targets.push(self.atoms.SAVE_TARGETS);
			let data = self.data_of(selection).read();
			if let Some(data) = &*data {
				targets.push(data.format);
				if data.format == self.atoms.UTF8_STRING {
					// When we are storing a UTF8 string,
					// add all equivalent formats to the supported targets
					targets.push(self.atoms.UTF8_MIME_0);
					targets.push(self.atoms.UTF8_MIME_1);
				}
			}
			self.server
				.conn
				.change_property32(
					PropMode::REPLACE,
					event.requestor,
					event.property,
					// TODO: change to `AtomEnum::ATOM`
					self.atoms.ATOM,
					&targets,
				)
				.map_err(into_unknown)?;
			self.server.conn.flush().map_err(into_unknown)?;
			success = true;
		} else {
			trace!("Handling request for (probably) the clipboard contents.");
			let data = self.data_of(selection).read();
			if let Some(data) = &*data {
				if data.format == event.target {
					self.server
						.conn
						.change_property8(
							PropMode::REPLACE,
							event.requestor,
							event.property,
							event.target,
							&data.bytes,
						)
						.map_err(into_unknown)?;
					self.server.conn.flush().map_err(into_unknown)?;
					success = true;
				} else {
					success = false
				}
			} else {
				// This must mean that we lost ownership of the data
				// since the other side requested the selection.
				// Let's respond with the property set to none.
				success = false;
			}
		}
		// on failure we notify the requester of it
		let property = if success { event.property } else { AtomEnum::NONE.into() };
		// tell the requestor that we finished sending data
		self.server
			.conn
			.send_event(
				false,
				event.requestor,
				EventMask::NO_EVENT,
				SelectionNotifyEvent {
					response_type: SELECTION_NOTIFY_EVENT,
					sequence: event.sequence,
					time: event.time,
					requestor: event.requestor,
					selection: event.selection,
					target: event.target,
					property,
				},
			)
			.map_err(into_unknown)?;

		self.server.conn.flush().map_err(into_unknown)
	}

	fn ask_clipboard_manager_to_request_our_data(&self) -> Result<()> {
		if self.server.win_id == 0 {
			// This shouldn't really ever happen but let's just check.
			error!("The server's window id was 0. This is unexpected");
			return Ok(());
		}

		if !self.is_owner(LinuxClipboardKind::Clipboard)? {
			// We are not owning the clipboard, nothing to do.
			return Ok(());
		}
		if self.data_of(LinuxClipboardKind::Clipboard).read().is_none() {
			// If we don't have any data, there's nothing to do.
			return Ok(());
		}

		// It's important that we lock the state before sending the request
		// because we don't want the request server thread to lock the state
		// after the request but before we can lock it here.
		let mut handover_state = self.handover_state.lock();

		trace!("Sending the data to the clipboard manager");
		self.server
			.conn
			.convert_selection(
				self.server.win_id,
				self.atoms.CLIPBOARD_MANAGER,
				self.atoms.SAVE_TARGETS,
				self.atoms.ARBOARD_CLIPBOARD,
				Time::CURRENT_TIME,
			)
			.map_err(into_unknown)?;
		self.server.conn.flush().map_err(into_unknown)?;

		*handover_state = ManagerHandoverState::InProgress;
		let max_handover_duration = Duration::from_millis(100);

		// Note that we are using a parking_lot condvar here, which doesn't wake up
		// spouriously
		let result = self.handover_cv.wait_for(&mut handover_state, max_handover_duration);

		if *handover_state == ManagerHandoverState::Finished {
			return Ok(());
		}
		if result.timed_out() {
			warn!("Could not hand the clipboard contents over to the clipboard manager. The request timed out.");
			return Ok(());
		}

		Err(Error::Unknown {
			description: "The handover was not finished and the condvar didn't time out, yet the condvar wait ended. This should be unreachable.".into()
		})
	}
}

struct ScopeGuard<F: FnOnce()> {
	callback: Option<F>,
}
impl<F: FnOnce()> ScopeGuard<F> {
	fn new(callback: F) -> Self {
		ScopeGuard { callback: Some(callback) }
	}
}
impl<F: FnOnce()> Drop for ScopeGuard<F> {
	fn drop(&mut self) {
		if let Some(callback) = self.callback.take() {
			(callback)();
		}
	}
}

fn serve_requests(clipboard: Arc<ClipboardContext>) -> Result<(), Box<dyn std::error::Error>> {
	fn handover_finished(
		clip: &Arc<ClipboardContext>,
		mut handover_state: MutexGuard<ManagerHandoverState>,
	) {
		log::trace!("Finishing clipboard manager handover.");
		*handover_state = ManagerHandoverState::Finished;

		// Not sure if unlocking the mutext is necessary here but better safe than sorry.
		drop(handover_state);

		clip.handover_cv.notify_all();
	}

	trace!("Started serve reqests thread.");

	let _guard = ScopeGuard::new(|| {
		clipboard.serve_stopped.store(true, Ordering::Relaxed);
	});

	let mut written = false;
	let mut notified = false;

	loop {
		match clipboard.server.conn.wait_for_event().map_err(into_unknown)? {
			Event::DestroyNotify(_) => {
				// This window is being destroyed.
				trace!("Clipboard server window is being destroyed x_x");
				return Ok(());
			}
			Event::SelectionClear(event) => {
				// TODO: check if this works
				// Someone else has new content in the clipboard, so it is
				// notifying us that we should delete our data now.
				trace!("Somebody else owns the clipboard now");

				if let Some(selection) = clipboard.kind_of(event.selection) {
					let mut data = clipboard.data_of(selection).write();
					*data = None
				}
			}
			Event::SelectionRequest(event) => {
				trace!(
					"SelectionRequest - selection is: {}, target is {}",
					clipboard.atom_name_dbg(event.selection),
					clipboard.atom_name_dbg(event.target),
				);
				// Someone is requesting the clipboard content from us.
				clipboard.handle_selection_request(event).map_err(into_unknown)?;

				// if we are in the progress of saving to the clipboard manager
				// make sure we save that we have finished writing
				let handover_state = clipboard.handover_state.lock();
				if *handover_state == ManagerHandoverState::InProgress {
					// Only set written, when the actual contents were written,
					// not just a response to what TARGETS we have.
					if event.target != clipboard.atoms.TARGETS {
						trace!("The contents were written to the clipboard manager.");
						written = true;
						// if we have written and notified, make sure to notify that we are done
						if notified {
							handover_finished(&clipboard, handover_state);
						}
					}
				}
			}
			Event::SelectionNotify(event) => {
				// We've requested the clipboard content and this is the answer.
				// Considering that this thread is not responsible for reading
				// clipboard contents, this must come from the clipboard manager
				// signaling that the data was handed over successfully.
				if event.selection != clipboard.atoms.CLIPBOARD_MANAGER {
					error!("Received a `SelectionNotify` from a selection other than the CLIPBOARD_MANAGER. This is unexpected in this thread.");
					continue;
				}
				let handover_state = clipboard.handover_state.lock();
				if *handover_state == ManagerHandoverState::InProgress {
					// Note that some clipboard managers send a selection notify
					// before even sending a request for the actual contents.
					// (That's why we use the "notified" & "written" flags)
					trace!("The clipboard manager indicated that it's done requesting the contents from us.");
					notified = true;

					// One would think that we could also finish if the property
					// here is set 0, because that indicates failure. However
					// this is not the case; for example on KDE plasma 5.18, we
					// immediately get a SelectionNotify with property set to 0,
					// but following that, we also get a valid SelectionRequest
					// from the clipboard manager.
					if written {
						handover_finished(&clipboard, handover_state);
					}
				}
			}
			_event => {
				// May be useful for debbuging but nothing else really.
				// trace!("Received unwanted event: {:?}", event);
			}
		}
	}
}

pub struct X11ClipboardContext {
	inner: Arc<ClipboardContext>,
}

impl X11ClipboardContext {
	pub fn new() -> Result<Self> {
		let mut global_cb = CLIPBOARD.lock();
		if let Some(global_cb) = &*global_cb {
			return Ok(Self { inner: Arc::clone(&global_cb.context) });
		}
		// At this point we know that the clipboard does not exists.
		let ctx = Arc::new(ClipboardContext::new()?);
		let join_handle;
		{
			let ctx = Arc::clone(&ctx);
			join_handle = std::thread::spawn(move || {
				if let Err(error) = serve_requests(ctx) {
					error!("Worker thread errored with: {}", error);
				}
			});
		}
		*global_cb =
			Some(GlobalClipboard { context: Arc::clone(&ctx), server_handle: join_handle });
		Ok(Self { inner: ctx })
	}

	pub fn get_text(&self) -> Result<String> {
		self.get_text_with_clipboard(LinuxClipboardKind::Clipboard)
	}

	pub(crate) fn get_text_with_clipboard(&self, selection: LinuxClipboardKind) -> Result<String> {
		let formats = [
			self.inner.atoms.UTF8_STRING,
			self.inner.atoms.UTF8_MIME_0,
			self.inner.atoms.UTF8_MIME_1,
			self.inner.atoms.STRING,
			self.inner.atoms.TEXT,
			self.inner.atoms.TEXT_MIME_UNKNOWN,
		];
		let result = self.inner.read(&formats, selection)?;
		if result.format == self.inner.atoms.STRING {
			// ISO Latin-1
			// See: https://stackoverflow.com/questions/28169745/what-are-the-options-to-convert-iso-8859-1-latin-1-to-a-string-utf-8
			Ok(result.bytes.into_iter().map(|c| c as char).collect())
		} else {
			String::from_utf8(result.bytes).map_err(|_| Error::ConversionFailure)
		}
	}

	pub fn set_text(&self, message: String) -> Result<()> {
		self.set_text_with_clipboard(message, LinuxClipboardKind::Clipboard)
	}

	pub(crate) fn set_text_with_clipboard(
		&self,
		message: String,
		selection: LinuxClipboardKind,
	) -> Result<()> {
		let data =
			ClipboardData { bytes: message.into_bytes(), format: self.inner.atoms.UTF8_STRING };
		self.inner.write(data, selection)
	}

	#[cfg(feature = "image-data")]
	pub fn get_image(&self) -> Result<ImageData<'static>> {
		let formats = [self.inner.atoms.PNG_MIME];
		let bytes = self.inner.read(&formats, LinuxClipboardKind::Clipboard)?.bytes;

		let cursor = std::io::Cursor::new(&bytes);
		let mut reader = image::io::Reader::new(cursor);
		reader.set_format(image::ImageFormat::Png);
		let image = match reader.decode() {
			Ok(img) => img.into_rgba8(),
			Err(_e) => return Err(Error::ConversionFailure),
		};
		let (w, h) = image.dimensions();
		let image_data =
			ImageData { width: w as usize, height: h as usize, bytes: image.into_raw().into() };
		Ok(image_data)
	}

	#[cfg(feature = "image-data")]
	pub fn set_image(&self, image: ImageData) -> Result<()> {
		let encoded = encode_as_png(&image)?;
		let data = ClipboardData { bytes: encoded, format: self.inner.atoms.PNG_MIME };
		self.inner.write(data, LinuxClipboardKind::Clipboard)
	}
}

impl Drop for X11ClipboardContext {
	fn drop(&mut self) {
		// There are always at least 3 owners:
		// the global, the server thread, and one `Clipboard::inner`
		const MIN_OWNERS: usize = 3;

		// We start with locking the global guard to prevent race
		// conditions below.
		let mut global_cb = CLIPBOARD.lock();
		if Arc::strong_count(&self.inner) == MIN_OWNERS {
			// If the are the only owers of the clipboard are ourselves and
			// the global object, then we should destroy the global object,
			// and send the data to the clipboard manager

			if let Err(e) = self.inner.ask_clipboard_manager_to_request_our_data() {
				error!("Could not hand the clipboard data over to the clipboard manager: {}", e);
			}
			let global_cb = global_cb.take();
			if let Err(e) = self.inner.server.conn.destroy_window(self.inner.server.win_id) {
				error!("Failed to destroy the clipboard window. Error: {}", e);
				return;
			}
			if let Err(e) = self.inner.server.conn.flush() {
				error!("Failed to flush the clipboard window. Error: {}", e);
				return;
			}
			if let Some(global_cb) = global_cb {
				if let Err(e) = global_cb.server_handle.join() {
					// Let's try extracting the error message
					let message;
					if let Some(msg) = e.downcast_ref::<&'static str>() {
						message = Some((*msg).to_string());
					} else if let Some(msg) = e.downcast_ref::<String>() {
						message = Some(msg.clone());
					} else {
						message = None;
					}
					if let Some(message) = message {
						error!("The clipboard server thread paniced. Panic message: '{}'", message);
					} else {
						error!("The clipboard server thread paniced.");
					}
				}
			}
		}
	}
}
