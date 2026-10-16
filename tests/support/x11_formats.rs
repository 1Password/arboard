use arboard::{Clipboard, ClipboardFormat, GetExtLinux, LinuxClipboardKind};
use x11rb::{
	connection::Connection,
	protocol::{
		xproto::{
			AtomEnum, ChangeWindowAttributesAux, ConnectionExt, CreateWindowAux, EventMask,
			PropMode, Property, SelectionNotifyEvent, WindowClass, SELECTION_NOTIFY_EVENT,
		},
		Event,
	},
	wrapper::ConnectionExt as _,
	CURRENT_TIME,
};

pub fn actual_type_is_preserved(clipboard: &mut Clipboard) {
	let (ready_tx, ready_rx) = std::sync::mpsc::channel();
	let owner = std::thread::spawn(move || {
		let (connection, screen) = x11rb::connect(None).unwrap();
		let window = connection.generate_id().unwrap();
		connection
			.create_window(
				0,
				window,
				connection.setup().roots[screen].root,
				0,
				0,
				1,
				1,
				0,
				WindowClass::INPUT_OUTPUT,
				0,
				&CreateWindowAux::new(),
			)
			.unwrap()
			.check()
			.unwrap();
		let incr = connection.intern_atom(false, b"INCR").unwrap().reply().unwrap().atom;
		connection
			.set_selection_owner(window, AtomEnum::SECONDARY.into(), CURRENT_TIME)
			.unwrap()
			.check()
			.unwrap();
		ready_tx.send(()).unwrap();
		let mut requests = 0;
		let mut transfer = None;
		loop {
			match connection.wait_for_event().unwrap() {
				Event::SelectionRequest(request) => {
					if requests == 0 {
						connection
							.change_property8(
								PropMode::REPLACE,
								request.requestor,
								request.property,
								AtomEnum::STRING,
								b"hello",
							)
							.unwrap();
					} else {
						connection
							.change_window_attributes(
								request.requestor,
								&ChangeWindowAttributesAux::new()
									.event_mask(EventMask::PROPERTY_CHANGE),
							)
							.unwrap()
							.check()
							.unwrap();
						connection
							.change_property32(
								PropMode::REPLACE,
								request.requestor,
								request.property,
								incr,
								&[11],
							)
							.unwrap();
						transfer = Some((request.requestor, request.property, 0));
					}
					requests += 1;
					connection
						.send_event(
							false,
							request.requestor,
							EventMask::NO_EVENT,
							SelectionNotifyEvent {
								response_type: SELECTION_NOTIFY_EVENT,
								sequence: 0,
								time: request.time,
								requestor: request.requestor,
								selection: request.selection,
								target: request.target,
								property: request.property,
							},
						)
						.unwrap();
					connection.flush().unwrap();
				}
				Event::PropertyNotify(event) if event.state == Property::DELETE => {
					if let Some((requestor, property, ref mut index)) = transfer {
						if event.window != requestor || event.atom != property {
							continue;
						}
						let bytes: &[u8] = match *index {
							0 => b"hello ",
							1 => b"world",
							2 => b"",
							_ => break,
						};
						connection
							.change_property8(
								PropMode::REPLACE,
								requestor,
								property,
								AtomEnum::STRING,
								bytes,
							)
							.unwrap();
						connection.flush().unwrap();
						*index += 1;
					}
				}
				_ => {}
			}
		}
	});
	ready_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
	let text = ClipboardFormat::register("TEXT").unwrap();
	assert_eq!(
		clipboard.get().clipboard(LinuxClipboardKind::Secondary).raw(&text).unwrap(),
		b"hello"
	);
	assert_eq!(
		clipboard.get().clipboard(LinuxClipboardKind::Secondary).raw(&text).unwrap(),
		b"hello world"
	);
	owner.join().unwrap();
}
