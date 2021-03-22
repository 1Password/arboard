/*
SPDX-License-Identifier: Apache-2.0 OR MIT

Copyright 2020 The arboard contributors

The project to which this file belongs is licensed under either of
the Apache 2.0 or the MIT license at the licensee's choice. The terms
and conditions of the chosen license apply to this file.
*/

//!
//!
//! This implementation is a port of https://github.com/dacap/clip to Rust
//! The structure of the original is more or less maintained.
//!
//! Disclaimer: The original C++ code is well organized and feels clean but it relies on C++
//! allowing a liberal data sharing between threads and it is painfully obvious from certain parts
//! of this port that this code was not designed for Rust. It should probably be reworked because
//! the absolute plague that the Arc<Mutex<>> objects are in this code is horrible just to look at
//! and will forever haunt me in my nightmares.
//!
//! Most changes are to conform with Rusts rules for example there are multiple overloads of
//! the `get_atom` functtion in the original but there's no function overloading in Rust so
//! those are split apart into functions with different names. (`get_atom_by_id` and the other one
//! at the time of writing I haven't needed to use)
//!
//! More noteably the `Manager` class had to be split into mutliple `structs` and some member
//! functions were made into global functions to conform Rust's aliasing rules.
//! Furthermore the signature of many functions was changed to follow a simple locking philosophy;
//! namely that the mutex gets locked at the topmost level possible and then most functions don't
//! need to attempt to lock, instead they just use the direct object references passed on as arguments.
//!
//!

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use lazy_static::lazy_static;
use x11rb::protocol::xproto;
use x11rb::{
	connection::Connection,
	protocol::{
		xproto::{
			Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, GetPropertyReply,
			PropMode, Property, PropertyNotifyEvent, SelectionClearEvent, SelectionNotifyEvent,
			SelectionRequestEvent, Time, Window, WindowClass,
		},
		Event,
	},
	rust_connection::RustConnection,
	wrapper::ConnectionExt as _,
};

use super::common::{Error, ImageData};

x11rb::atom_manager! {
	pub CommonAtoms: CommonAtomCookies {
		ATOM,
		INCR,
		TARGETS,
		CLIPBOARD,
		MIME_IMAGE_PNG: b"image/png",
		ATOM_PAIR,
		SAVE_TARGETS,
		MULTIPLE,
		CLIPBOARD_MANAGER,
	}
}

x11rb::atom_manager! {
	pub TextAtoms: TextAtomCookies {
		UTF8_STRING,
		TEXT_PLAIN_1: b"text/plain;charset=utf-8",
		TEXT_PLAIN_2: b"text/plain;charset=UTF-8",
		// ANSI C strings?
		STRING,
		TEXT,
		TEXT_PLAIN_0: b"text/plain",
	}
}

type BufferPtr = Option<Arc<Mutex<Vec<u8>>>>;
type Atoms = Vec<Atom>;
type NotifyCallback = Option<Arc<dyn (Fn(&BufferPtr) -> bool) + Send + Sync + 'static>>;

lazy_static! {
	static ref LOCKED_OBJECTS: Arc<Mutex<Option<LockedObjects>>> = Arc::new(Mutex::new(None));

	// Used to wait/notify the arrival of the SelectionNotify event when
	// we requested the clipboard content from other selection owner.
	static ref CONDVAR: Condvar = Condvar::new();
}

struct LockedObjects {
	shared: SharedState,
	manager: Manager,
}

impl LockedObjects {
	fn new() -> Result<LockedObjects, Error> {
		let (connection, screen) = RustConnection::connect(None).unwrap();
		match Manager::new(&connection, screen) {
			Ok(manager) => {
				//unsafe { libc::atexit(Manager::destruct); }
				Ok(LockedObjects {
					shared: SharedState {
						conn: Some(Arc::new(connection)),
						common_atoms: Default::default(),
						text_atoms: Default::default(),
					},
					manager,
				})
			}
			Err(e) => Err(e),
		}
	}
}

/// The name indicates that objects in this struct are shared between
/// the event processing thread and the user tread. However it's important
/// that the `Manager` itself is also shared. So the real reason for splitting these
/// apart from the `Manager` is to conform to Rust's aliasing rules but that is hard to
/// convey in a short name.
struct SharedState {
	conn: Option<Arc<RustConnection>>,

	// Cache of common used atoms by us
	common_atoms: Option<CommonAtoms>,

	// Cache of atoms related to text or image content
	text_atoms: Option<TextAtoms>,
	//image_atoms: Atoms,
}

impl SharedState {
	fn common_atoms(&mut self) -> CommonAtoms {
		self.common_atoms.unwrap_or_else(|| {
			CommonAtoms::new(self.conn.as_ref().unwrap().as_ref()).unwrap().reply().unwrap()
		})
	}

	fn text_atoms(&mut self) -> Atoms {
		let atoms = self.text_atoms.unwrap_or_else(|| {
			TextAtoms::new(self.conn.as_ref().unwrap().as_ref()).unwrap().reply().unwrap()
		});

		vec![
			atoms.UTF8_STRING,
			atoms.TEXT_PLAIN_1,
			atoms.TEXT_PLAIN_2,
			atoms.STRING,
			atoms.TEXT,
			atoms.TEXT_PLAIN_0,
		]
	}
}

struct Manager {
	// Temporal background window used to own the clipboard and process
	// all events related about the clipboard in a background thread
	window: Window,

	// Thread used to run a background message loop to wait X11 events
	// about clipboard. The X11 selection owner will be a hidden window
	// created by us just for the clipboard purpose/communication.
	thread_handle: Option<std::thread::JoinHandle<()>>,

	// WARNING: The callback must not attempt to lock the manager or the shared state.
	// (Otherwise the code needs to be restructured slightly)
	//
	// Internal callback used when a SelectionNotify is received (or the
	// whole data content is received by the INCR method). So this
	// callback can use the notification by different purposes (e.g. get
	// the data length only, or get/process the data content, etc.).
	callback: NotifyCallback,

	// Result returned by the m_callback. Used as return value in the
	// get_data_from_selection_owner() function. For example, if the
	// callback must read a "image/png" file from the clipboard data and
	// fails, the callback can return false and finally the get_image()
	// will return false (i.e. there is data, but it's not a valid image
	// format).
	callback_result: bool,

	// Actual clipboard data generated by us (when we "copy" content in
	// the clipboard, it means that we own the X11 "CLIPBOARD"
	// selection, and in case of SelectionRequest events, we've to
	// return the data stored in this "m_data" field)
	data: BTreeMap<Atom, BufferPtr>,

	// Copied image in the clipboard. As we have to transfer the image
	// in some specific format (e.g. image/png) we want to keep a copy
	// of the image and make the conversion when the clipboard data is
	// requested by other process.
	image: super::common::ImageData<'static>,

	// True if we have received an INCR notification so we're going to
	// process several PropertyNotify to concatenate all data chunks.
	incr_process: bool,

	/// Variable used to wait more time if we've received an INCR
	/// notification, which means that we're going to receive large
	/// amounts of data from the selection owner.
	///mutable bool m_incr_received;
	incr_received: bool,

	// Target/selection format used in the SelectionNotify. Used in the
	// INCR method to get data from the same property in the same format
	// (target) on each PropertyNotify.
	target_atom: Atom,

	// Each time we receive data from the selection owner, we put that
	// data in this buffer. If we get the data with the INCR method,
	// we'll concatenate chunks of data in this buffer to complete the
	// whole clipboard content.
	reply_data: BufferPtr,

	// Used to concatenate chunks of data in "m_reply_data" from several
	// PropertyNotify when we are getting the selection owner data with
	// the INCR method.
	reply_offset: usize,
	// List of user-defined formats/atoms.
	//custom_formats: Vec<xcb::xproto::Atom>,
}

impl Manager {
	fn new(connection: &RustConnection, screen: usize) -> Result<Self, Error> {
		let setup = connection.setup();
		let screen = setup.roots.get(screen).ok_or(Error::Unknown {
			description: String::from("Could not get screen from setup"),
		})?;
		let event_mask =
            // Just in case that some program reports SelectionNotify events
            // with XCB_EVENT_MASK_PROPERTY_CHANGE mask.
            EventMask::PROPERTY_CHANGE |
            // To receive DestroyNotify event and stop the message loop.
            EventMask::STRUCTURE_NOTIFY;
		let window = connection
			.generate_id()
			.map_err(|e| Error::Unknown { description: format!("{}", e) })?;
		connection
			.create_window(
				0,
				window,
				screen.root,
				0,
				0,
				1,
				1,
				0,
				WindowClass::INPUT_OUTPUT,
				screen.root_visual,
				&CreateWindowAux::new().event_mask(event_mask),
			)
			.map_err(|e| Error::Unknown { description: format!("{}", e) })?;

		let thread_handle = std::thread::spawn(process_x11_events);

		Ok(Manager {
			//mutex: Mutex::new(()),
			window,
			thread_handle: Some(thread_handle),
			callback: None,
			callback_result: false,
			data: Default::default(),
			image: super::common::ImageData {
				width: 0,
				height: 0,
				bytes: std::borrow::Cow::from(vec![]),
			},
			incr_process: false,
			incr_received: false,
			target_atom: 0,
			reply_data: Default::default(),
			reply_offset: 0,
		})
	}

	fn set_x11_selection_owner(&self, shared: &mut SharedState) -> bool {
		let clipboard_atom = shared.common_atoms().CLIPBOARD;
		let cookie = shared.conn.as_ref().unwrap().set_selection_owner(
			self.window,
			clipboard_atom,
			Time::CURRENT_TIME,
		);

		cookie.is_ok()
	}

	fn set_image(&mut self, shared: &mut SharedState, image: ImageData) -> Result<(), Error> {
		if !self.set_x11_selection_owner(shared) {
			return Err(Error::Unknown {
				description: "Failed to set x11 selection owner.".into(),
			});
		}

		self.image.width = image.width;
		self.image.height = image.height;
		self.image.bytes = image.bytes.into_owned().into();

		// Put a ~nullptr~ (None) in the m_data for image/png format and then we'll
		// encode the png data when the image is requested in this format.
		self.data.insert(shared.common_atoms().MIME_IMAGE_PNG, None);

		Ok(())
	}

	/// Rust impl: instead of this function there's a more generic `set_data` which I believe can
	/// also set user formats, but arboard doesn't support that for now.
	fn set_text(&mut self, shared: &mut SharedState, bytes: Vec<u8>) -> Result<(), Error> {
		if !self.set_x11_selection_owner(shared) {
			return Err(Error::Unknown {
				description: "Could not take ownership of the x11 selection".into(),
			});
		}

		let atoms = shared.text_atoms();
		if atoms.is_empty() {
			return Err(Error::Unknown { description:
				"Couldn't get the atoms that identify supported text formats for the x11 clipboard"
					.into(),
			});
		}

		let arc_data = Arc::new(Mutex::new(bytes));
		for atom in atoms {
			self.data.insert(atom, Some(arc_data.clone()));
		}

		Ok(())
	}

	fn clear_data(&mut self) {
		self.data.clear();
		self.image.width = 0;
		self.image.height = 0;
		self.image.bytes = Vec::new().into();
	}

	fn set_requestor_property_with_clipboard_content(
		&mut self,
		shared: &mut SharedState,
		requestor: Window,
		property: Atom,
		target: Atom,
	) -> bool {
		let item = {
			if let Some(item) = self.data.get_mut(&target) {
				item
			} else {
				// Nothing to do (unsupported target)
				return false;
			}
		};

		// This can be null if the data was set from an image but we
		// didn't encode the image yet (e.g. to image/png format).
		if item.is_none() {
			encode_data_on_demand(shared, &mut self.image, target, item);

			// Return nothing, the given "target" cannot be constructed
			// (maybe by some encoding error).
			if item.is_none() {
				return false;
			}
		}

		let item = item.as_ref().unwrap().lock().unwrap();
		// Set the "property" of "requestor" with the
		// clipboard content in the requested format ("target").
		if let Err(e) = shared.conn.as_ref().unwrap().change_property8(
			PropMode::REPLACE,
			requestor,
			property,
			target,
			item.as_slice(),
		) {
			log::error!("{}", e)
		}

		true
	}

	fn copy_reply_data(&mut self, reply: &GetPropertyReply) {
		let src = &reply.value;
		// n = length of "src" in bytes
		let n = reply.value_len;
		let req = self.reply_offset + n as usize;
		match &mut self.reply_data {
			None => {
				self.reply_offset = 0; // Rust impl: I added this just to be extra sure.
				self.reply_data = Some(Arc::new(Mutex::new(vec![0; req])));
			}
			// The "m_reply_data" size can be smaller because the size
			// specified in INCR property is just a lower bound.
			Some(reply_data) => {
				let mut reply_data = reply_data.lock().unwrap();
				if req > reply_data.len() {
					reply_data.resize(req, 0);
				}
			}
		}
		let src_slice = src.as_slice();
		let mut reply_data_locked = self.reply_data.as_mut().unwrap().lock().unwrap();
		reply_data_locked[self.reply_offset..req].copy_from_slice(src_slice);
		self.reply_offset += n as usize;
	}

	// Rust impl: It's strange, the reply attribute is also unused in the original code.
	fn call_callback(&mut self, _reply: GetPropertyReply) {
		self.callback_result = false;
		if let Some(callback) = &self.callback {
			self.callback_result = callback(&self.reply_data);
		}
		CONDVAR.notify_one();

		self.reply_data = None;
	}

	/// Rust impl: This function was added instead of the destructor because the drop
	/// does not get called on lazy static objects. This function is registered for `libc::atexit`
	/// on a successful initialization
	fn destruct() {
		let join_handle;

		// The following scope is to ensure that we release the lock
		// before attempting to join the thread.
		{
			let mut guard = LOCKED_OBJECTS.lock().unwrap();
			if guard.is_none() {
				return;
			}
			macro_rules! manager {
				() => {
					guard.as_mut().unwrap().manager
				};
			}
			macro_rules! shared {
				() => {
					guard.as_mut().unwrap().shared
				};
			}

			if !manager!().data.is_empty()
				&& manager!().window != 0
				&& manager!().window == get_x11_selection_owner(&mut shared!())
			{
				let atoms = vec![shared!().common_atoms().SAVE_TARGETS];
				let selection = shared!().common_atoms().CLIPBOARD_MANAGER;

				// Start the SAVE_TARGETS mechanism so the X11
				// CLIPBOARD_MANAGER will save our clipboard data
				// from now on.
				guard = get_data_from_selection_owner(
					guard,
					&atoms,
					Some(Arc::new(|_| true)),
					selection,
				)
				.1;
			}

			if manager!().window != 0 {
				let window = manager!().window;
				let _ = shared!().conn.as_ref().unwrap().destroy_window(window);
				let _ = shared!().conn.as_ref().unwrap().flush();
				manager!().window = 0;
			}
			join_handle = manager!().thread_handle.take();
		}

		if let Some(handle) = join_handle {
			handle.join().ok();
		}

		// This is not needed because the connection is automatically disconnected when droped
		// if (m_connection)
		//     xcb_disconnect(m_connection);
	}
}

fn process_x11_events() {
	let connection = {
		let lo = LOCKED_OBJECTS.lock().unwrap();
		lo.as_ref().unwrap().shared.conn.clone()
	};

	let mut stop = false;
	while !stop {
		let event = {
			// If this doesn't work, wrap the connection into an Arc
			std::thread::sleep(Duration::from_millis(5));
			let maybe_event = connection.as_ref().unwrap().poll_for_event();
			match maybe_event {
				Ok(Some(e)) => e,
				Ok(None) => continue,
				Err(_) => break,
			}
		};
		println!();
		match event {
			Event::DestroyNotify(_) => {
				//println!("Received destroy event, stopping");
				stop = true;
				//panic!("{}", line!());
				//break;
			}

			// Someone else has new content in the clipboard, so is
			// notifying us that we should delete our data now.
			Event::SelectionClear(event) => {
				//println!("Received selection clear,");
				handle_selection_clear_event(event);
			}

			// Someone is requesting the clipboard content from us.
			Event::SelectionRequest(event) => {
				//println!("Received selection request");
				handle_selection_request_event(event);
			}

			// We've requested the clipboard content and this is the
			// answer.
			Event::SelectionNotify(event) => {
				//println!("Received selection notify");
				handle_selection_notify_event(event);
			}

			Event::PropertyNotify(event) => {
				//println!("Received property notify");
				handle_property_notify_event(event);
			}
			_ => {}
		}
		// The event uses RAII, so it's free'd automatically
	}
}

fn handle_selection_clear_event(event: SelectionClearEvent) {
	println!("SelectionClearEvent");
	let selection = event.selection;
	let mut guard = LOCKED_OBJECTS.lock().unwrap();
	let locked = guard.as_mut().unwrap();
	let clipboard_atom = { locked.shared.common_atoms().CLIPBOARD };
	if selection == clipboard_atom {
		locked.manager.clear_data();
	}
}

fn handle_selection_request_event(event: SelectionRequestEvent) {
	let target = event.target;
	let requestor = event.requestor;
	let property = event.property;
	let time = event.time;
	let selection = event.selection;
	let targets_atom;
	let save_targets_atom;
	let multiple_atom;
	let atom_atom;
	{
		let mut guard = LOCKED_OBJECTS.lock().unwrap();
		let locked = guard.as_mut().unwrap();
		let shared = &mut locked.shared;

		let stuff = shared.conn.as_ref().unwrap().get_atom_name(target).unwrap().reply().unwrap();
		let target_name = String::from_utf8_lossy(&stuff.name);
		println!("SelectionRequestEvent: '{:?}'", target_name);

		targets_atom = shared.common_atoms().TARGETS;
		save_targets_atom = shared.common_atoms().SAVE_TARGETS;
		multiple_atom = shared.common_atoms().MULTIPLE;
		atom_atom = shared.common_atoms().ATOM;
	}
	if target == targets_atom {
		let mut targets = Atoms::with_capacity(4);
		targets.push(targets_atom);
		targets.push(save_targets_atom);
		targets.push(multiple_atom);
		let mut guard = LOCKED_OBJECTS.lock().unwrap();
		let locked = guard.as_mut().unwrap();
		let manager = &locked.manager;
		for atom in manager.data.keys() {
			targets.push(*atom);
		}

		let shared = &locked.shared;
		// Set the "property" of "requestor" with the clipboard
		// formats ("targets", atoms) that we provide.
		if let Err(e) = shared.conn.as_ref().unwrap().change_property32(
			PropMode::REPLACE,
			requestor,
			property,
			atom_atom,
			targets.as_slice(),
		) {
			log::error!("{}", e);
		};
	} else if target == save_targets_atom {
		// Do nothing
	} else if target == multiple_atom {
		let mut guard = LOCKED_OBJECTS.lock().unwrap();
		let locked = guard.as_mut().unwrap();
		let reply = {
			let atom_pair_atom = locked.shared.common_atoms().ATOM_PAIR;
			get_and_delete_property(
				locked.shared.conn.as_ref().unwrap(),
				requestor,
				property,
				atom_pair_atom,
				false,
			)
		};
		if let Some(reply) = reply {
			let atoms = reply.value32();
			for atom in atoms.into_iter().flatten() {
				let target = atom;
				let property = atom;
				let property_set = locked.manager.set_requestor_property_with_clipboard_content(
					&mut locked.shared,
					requestor,
					property,
					target,
				);
				if !property_set {
					if let Err(e) = locked.shared.conn.as_ref().unwrap().change_property(
						PropMode::REPLACE,
						requestor,
						property,
						AtomEnum::NONE,
						0,
						0,
						&[],
					) {
						log::error!("{}", e)
					}
				}
			}
		}
	} else {
		let mut guard = LOCKED_OBJECTS.lock().unwrap();
		let locked = guard.as_mut().unwrap();
		let property_set = locked.manager.set_requestor_property_with_clipboard_content(
			&mut locked.shared,
			requestor,
			property,
			target,
		);
		if !property_set {
			return;
		}
	}

	let mut guard = LOCKED_OBJECTS.lock().unwrap();
	let locked = guard.as_mut().unwrap();
	let shared = &mut locked.shared;

	// Notify the "requestor" that we've already updated the property.
	let notify = SelectionNotifyEvent {
		response_type: xproto::SELECTION_NOTIFY_EVENT,
		sequence: 0,
		time,
		requestor,
		selection,
		target,
		property,
	};
	if let Err(e) =
		shared.conn.as_ref().unwrap().send_event(false, requestor, EventMask::NO_EVENT, notify)
	{
		log::error!("{}", e)
	}
	if let Err(e) = shared.conn.as_ref().unwrap().flush() {
		log::error!("{}", e)
	}
}

fn handle_selection_notify_event(event: SelectionNotifyEvent) {
	println!("SelectionNotifyEvent");
	let target = event.target;
	let requestor = event.requestor;
	let property = event.property;
	let mut guard = LOCKED_OBJECTS.lock().unwrap();
	let mut locked = guard.as_mut().unwrap();
	assert_eq!(requestor, locked.manager.window);

	if target == locked.shared.common_atoms().TARGETS {
		locked.manager.target_atom = locked.shared.common_atoms().ATOM;
	} else {
		locked.manager.target_atom = target;
	}

	let target_atom = locked.manager.target_atom;
	let reply = get_and_delete_property(
		locked.shared.conn.as_ref().unwrap(),
		requestor,
		property,
		target_atom,
		true,
	);
	if let Some(reply) = reply {
		let reply_type = reply.type_;
		// In this case, We're going to receive the clipboard content in
		// chunks of data with several PropertyNotify events.
		let incr_atom = locked.shared.common_atoms().INCR;
		if reply_type == incr_atom {
			let reply = get_and_delete_property(
				locked.shared.conn.as_ref().unwrap(),
				requestor,
				property,
				incr_atom,
				true,
			);
			if let Some(reply) = reply {
				if reply.value_len == 4 {
					let n = reply.value32().and_then(|mut values| values.next()).unwrap_or(0);
					locked.manager.reply_data = Some(Arc::new(Mutex::new(vec![0u8; n as usize])));
					locked.manager.reply_offset = 0;
					locked.manager.incr_process = true;
					locked.manager.incr_received = true;
				}
			}
		} else {
			// Simple case, the whole clipboard content in just one reply
			// (without the INCR method).
			locked.manager.reply_data = None;
			locked.manager.reply_offset = 0;
			locked.manager.copy_reply_data(&reply);
			locked.manager.call_callback(reply);
		}
	}
}

fn handle_property_notify_event(event: PropertyNotifyEvent) {
	println!("PropertyNotifyEvent");
	let state = event.state;
	let atom = event.atom;
	let window = event.window;
	let mut guard = LOCKED_OBJECTS.lock().unwrap();
	let mut locked = guard.as_mut().unwrap();
	if locked.manager.incr_process
		&& state == Property::NEW_VALUE
		&& atom == locked.shared.common_atoms().CLIPBOARD
	{
		let target_atom = locked.manager.target_atom;
		let reply = get_and_delete_property(
			locked.shared.conn.as_ref().unwrap(),
			window,
			atom,
			target_atom,
			true,
		);
		if let Some(reply) = reply {
			locked.manager.incr_received = true;

			// When the length is 0 it means that the content was
			// completely sent by the selection owner.
			if reply.value_len > 0 {
				locked.manager.copy_reply_data(&reply);
			} else {
				// Now that m_reply_data has the complete clipboard content,
				// we can call the m_callback.
				locked.manager.call_callback(reply);
				locked.manager.incr_process = false;
			}
		}
	}
}

fn get_and_delete_property(
	conn: &RustConnection,
	window: Window,
	property: Atom,
	atom: Atom,
	delete_prop: bool,
) -> Option<GetPropertyReply> {
	conn.get_property(
		delete_prop,
		window,
		property,
		atom,
		0,
		0x1fffffff, // 0x1fffffff = INT32_MAX / 4
	)
	.ok()
	.and_then(|cookie| cookie.reply().ok())
}

fn get_data_from_selection_owner<'a>(
	mut guard: MutexGuard<'a, Option<LockedObjects>>,
	atoms: &[Atom],
	callback: NotifyCallback,
	mut selection: xproto::Atom,
) -> (bool, MutexGuard<'a, Option<LockedObjects>>) {
	// Wait a response for 100 milliseconds
	const CV_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);
	{
		let locked = guard.as_mut().unwrap();
		if selection == 0 {
			selection = locked.shared.common_atoms().CLIPBOARD;
		}
		locked.manager.callback = callback;

		// Clear data if we are not the selection owner.
		if locked.manager.window != get_x11_selection_owner(&mut locked.shared) {
			locked.manager.data.clear();
		}
	}

	// Ask to the selection owner for its content on each known
	// text format/atom.
	for atom in atoms.iter() {
		{
			let locked = guard.as_mut().unwrap();
			let clipboard_atom = locked.shared.common_atoms().CLIPBOARD;
			if let Err(e) = locked.shared.conn.as_ref().unwrap().convert_selection(
				locked.manager.window,
				selection,
				*atom,
				clipboard_atom,
				Time::CURRENT_TIME,
			) {
				log::error!("{}", e)
			}
			if let Err(e) = locked.shared.conn.as_ref().unwrap().flush() {
				log::error!("{}", e)
			}
		}

		// We use the "m_incr_received" to wait several timeouts in case
		// that we've received the INCR SelectionNotify or
		// PropertyNotify events.
		'incr_loop: loop {
			guard.as_mut().unwrap().manager.incr_received = false;
			match CONDVAR.wait_timeout(guard, CV_TIMEOUT) {
				Ok((new_guard, status)) => {
					guard = new_guard;
					if !status.timed_out() {
						// If the condition variable was notified, it means that the
						// callback was called correctly.
						return (guard.as_ref().unwrap().manager.callback_result, guard);
					}

					if !guard.as_ref().unwrap().manager.incr_received {
						break 'incr_loop;
					}
				}
				Err(err) => {
					panic!(
						"A critical error occured while working with the x11 clipboard. {}",
						err
					);
				}
			}
		}
	}

	guard.as_mut().unwrap().manager.callback = None;
	(false, guard)
}

fn get_x11_selection_owner(shared: &mut SharedState) -> Window {
	let mut result = 0;

	let clipboard_atom = shared.common_atoms().CLIPBOARD;
	let cookie = shared.conn.as_ref().unwrap().get_selection_owner(clipboard_atom);
	let reply = cookie.ok().and_then(|cookie| cookie.reply().ok());
	if let Some(reply) = reply {
		result = reply.owner;
	}

	result
}

fn get_text(mut guard: MutexGuard<Option<LockedObjects>>) -> Result<String, Error> {
	// Rust impl: This function is probably the ugliest Rust code I've ever written
	// Make no mistake, the original, C++ code was perfectly fine (which I didn't write)
	let owner = get_x11_selection_owner(&mut guard.as_mut().unwrap().shared);
	if owner == guard.as_mut().unwrap().manager.window {
		let atoms = guard.as_mut().unwrap().shared.text_atoms();
		for atom in atoms.iter() {
			let mut item = None;
			if let Some(Some(i)) = guard.as_mut().unwrap().manager.data.get(atom) {
				item = Some(i.clone());
			}
			if let Some(item) = item {
				// Unwrapping the item because we always initialize text with `Some`
				let locked = item.lock().unwrap();
				let result = String::from_utf8(locked.clone());
				return result.map_err(|_| Error::ConversionFailure);
			}
		}
	} else if owner != 0 {
		let atoms = guard.as_mut().unwrap().shared.text_atoms();
		let result = Arc::new(Mutex::new(Ok(String::new())));
		let callback = {
			let result = result.clone();
			Arc::new(move |data: &BufferPtr| {
				if let Some(reply_data) = data {
					let locked_data = reply_data.lock().unwrap();
					let mut locked_result = result.lock().unwrap();
					*locked_result = String::from_utf8(locked_data.clone());
				}
				true
			})
		};

		let (success, _) = get_data_from_selection_owner(guard, &atoms, Some(callback as _), 0);
		if success {
			let mut taken = Ok(String::new());
			let mut locked = result.lock().unwrap();
			std::mem::swap(&mut taken, &mut locked);
			return taken.map_err(|_| Error::ConversionFailure);
		}
	}
	Err(Error::ContentNotAvailable)
}

fn get_image(mut guard: MutexGuard<Option<LockedObjects>>) -> Result<ImageData, Error> {
	let owner = get_x11_selection_owner(&mut guard.as_mut().unwrap().shared);
	//let mut result_img;
	if owner == guard.as_ref().unwrap().manager.window {
		let image = &guard.as_ref().unwrap().manager.image;
		if image.width > 0 && image.height > 0 && !image.bytes.is_empty() {
			return Ok(image.to_owned_img());
		}
	} else if owner != 0 {
		let atoms = vec![guard.as_mut().unwrap().shared.common_atoms().MIME_IMAGE_PNG];
		let result: Arc<Mutex<Result<ImageData, Error>>> =
			Arc::new(Mutex::new(Err(Error::ContentNotAvailable)));
		let callback = {
			let result = result.clone();
			Arc::new(move |data: &BufferPtr| {
				if let Some(reply_data) = data {
					let locked_data = reply_data.lock().unwrap();
					let cursor = std::io::Cursor::new(&*locked_data);
					let mut reader = image::io::Reader::new(cursor);
					reader.set_format(image::ImageFormat::Png);
					let image;
					match reader.decode() {
						Ok(img) => image = img.into_rgba8(),
						Err(_e) => {
							let mut locked_result = result.lock().unwrap();
							*locked_result = Err(Error::ConversionFailure);
							return false;
						}
					}
					let (w, h) = image.dimensions();
					let mut locked_result = result.lock().unwrap();
					let image_data = ImageData {
						width: w as usize,
						height: h as usize,
						bytes: image.into_raw().into(),
					};
					*locked_result = Ok(image_data);
				}
				true
			})
		};
		let _success = get_data_from_selection_owner(guard, &atoms, Some(callback as _), 0).0;
		// Rust impl: We return the result here no matter if it succeeded, because the result will
		// tell us if it hasn't
		let mut taken = Err(Error::Unknown {
			description: format!("Implementation error at {}:{}", file!(), line!()),
		});
		let mut locked = result.lock().unwrap();
		std::mem::swap(&mut taken, &mut locked);
		return taken;
	}
	Err(Error::ContentNotAvailable)
}

fn encode_data_on_demand(
	shared: &mut SharedState,
	image: &mut ImageData,
	atom: xproto::Atom,
	buffer: &mut Option<Arc<Mutex<Vec<u8>>>>,
) {
	/// This is a workaround for the PNGEncoder not having a `into_inner` like function
	/// which would allow us to take back our Vec after the encoder finished encoding.
	/// So instead we create this wrapper around an Rc Vec which implements `io::Write`
	#[derive(Clone)]
	struct RcBuffer {
		inner: Rc<RefCell<Vec<u8>>>,
	}
	impl std::io::Write for RcBuffer {
		fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
			self.inner.borrow_mut().extend_from_slice(buf);
			Ok(buf.len())
		}
		fn flush(&mut self) -> std::io::Result<()> {
			// Noop
			Ok(())
		}
	}

	if atom == shared.common_atoms().MIME_IMAGE_PNG {
		if image.bytes.is_empty() || image.width == 0 || image.height == 0 {
			return;
		}

		let output = RcBuffer { inner: Rc::new(RefCell::new(Vec::new())) };
		let encoding_result;
		{
			let encoder = image::png::PngEncoder::new(output.clone());
			encoding_result = encoder.encode(
				image.bytes.as_ref(),
				image.width as u32,
				image.height as u32,
				image::ColorType::Rgba8,
			);
		}
		// Rust impl: The encoder must be destroyed so that it lets go of its reference to the
		// `output` before we `try_unwrap()`
		if encoding_result.is_ok() {
			*buffer =
				Some(Arc::new(Mutex::new(Rc::try_unwrap(output.inner).unwrap().into_inner())));
		}
	}
}

fn ensure_lo_initialized() -> Result<MutexGuard<'static, Option<LockedObjects>>, Error> {
	let mut locked = LOCKED_OBJECTS.lock().unwrap();
	if locked.is_none() {
		*locked = Some(LockedObjects::new().map_err(|e| Error::Unknown {
			description: format!(
				"Could not initialize the x11 clipboard handling facilities. Cause: {}",
				e
			),
		})?);
	}
	Ok(locked)
}

fn with_locked_objects<F, T>(action: F) -> Result<T, Error>
where
	F: FnOnce(&mut LockedObjects) -> Result<T, Error>,
{
	// The gobal may not have been initialized yet or may have been destroyed previously.
	//
	// Note: the global objects gets destroyed (replaced with None) when the last
	// clipboard context is dropped (goes out of scope).
	let mut locked = ensure_lo_initialized()?;
	let lo = locked.as_mut().unwrap();
	action(lo)
}

pub struct X11ClipboardContext {
	_owned: Arc<Mutex<Option<LockedObjects>>>,
}

impl Drop for X11ClipboardContext {
	fn drop(&mut self) {
		// If there's no other owner than us and the global,
		// then destruct the manager
		if Arc::strong_count(&LOCKED_OBJECTS) == 2 {
			Manager::destruct();
			let mut locked = LOCKED_OBJECTS.lock().unwrap();
			*locked = None;
		}
	}
}

impl X11ClipboardContext {
	pub(crate) fn new() -> Result<Self, Error> {
		Ok(X11ClipboardContext { _owned: LOCKED_OBJECTS.clone() })
	}

	pub(crate) fn get_text(&mut self) -> Result<String, Error> {
		let locked = ensure_lo_initialized()?;
		get_text(locked)
	}

	pub(crate) fn set_text(&mut self, text: String) -> Result<(), Error> {
		with_locked_objects(|locked| {
			let manager = &mut locked.manager;
			let shared = &mut locked.shared;
			manager.set_text(shared, text.into_bytes())
		})
	}

	pub(crate) fn get_image(&mut self) -> Result<ImageData, Error> {
		let locked = ensure_lo_initialized()?;
		get_image(locked)
	}

	pub(crate) fn set_image(&mut self, image: ImageData) -> Result<(), Error> {
		with_locked_objects(|locked| {
			let manager = &mut locked.manager;
			let shared = &mut locked.shared;
			manager.set_image(shared, image)
		})
	}
}
