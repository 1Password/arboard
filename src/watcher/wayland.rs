use crate::{watcher::unknown, Error};
use wayland_client::{
	backend::WaylandError,
	delegate_noop, event_created_child,
	globals::{registry_queue_init, GlobalListContents},
	protocol::{wl_registry, wl_seat},
	Connection, Dispatch, EventQueue, QueueHandle,
};
use wayland_protocols::ext::data_control::v1::client::{
	ext_data_control_device_v1 as ext_device, ext_data_control_manager_v1 as ext_manager,
	ext_data_control_offer_v1 as ext_offer,
};
use wayland_protocols_wlr::data_control::v1::client::{
	zwlr_data_control_device_v1 as wlr_device, zwlr_data_control_manager_v1 as wlr_manager,
	zwlr_data_control_offer_v1 as wlr_offer,
};

#[derive(Default)]
struct State {
	seat_name: u32,
	changed: bool,
	finished: bool,
}

pub(in crate::watcher) struct Backend {
	connection: Connection,
	queue: EventQueue<State>,
	state: State,
}

fn allow_would_block<T>(result: Result<T, WaylandError>) -> Result<(), Error> {
	match result {
		Ok(_) => Ok(()),
		Err(WaylandError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
		Err(error) => Err(unknown(error)),
	}
}

impl Backend {
	pub(super) fn new() -> Result<Self, Error> {
		let connection = Connection::connect_to_env().map_err(unknown)?;
		Self::from_connection(connection)
	}

	fn from_connection(connection: Connection) -> Result<Self, Error> {
		let (globals, mut queue) = registry_queue_init::<State>(&connection).map_err(unknown)?;
		let handle = queue.handle();
		let (seat_name, seat_version, has_ext, has_wlr) = globals.contents().with_list(|list| {
			let seat = list.iter().find(|global| global.interface == "wl_seat");
			(
				seat.map(|seat| seat.name),
				seat.map(|seat| seat.version).unwrap_or(1),
				list.iter().any(|global| global.interface == "ext_data_control_manager_v1"),
				list.iter().any(|global| global.interface == "zwlr_data_control_manager_v1"),
			)
		});
		let seat_name = seat_name.ok_or(Error::ClipboardNotSupported)?;
		let seat: wl_seat::WlSeat =
			globals.registry().bind(seat_name, seat_version.min(1), &handle, ());
		if has_ext {
			let manager: ext_manager::ExtDataControlManagerV1 =
				globals.bind(&handle, 1..=1, ()).map_err(unknown)?;
			manager.get_data_device(&seat, &handle, ());
			manager.destroy();
		} else if has_wlr {
			let manager: wlr_manager::ZwlrDataControlManagerV1 =
				globals.bind(&handle, 1..=2, ()).map_err(unknown)?;
			manager.get_data_device(&seat, &handle, ());
			manager.destroy();
		} else {
			return Err(Error::ClipboardNotSupported);
		}
		let mut state = State { seat_name, ..State::default() };
		queue.roundtrip(&mut state).map_err(unknown)?;
		if state.finished {
			return Err(Error::ClipboardNotSupported);
		}
		state.changed = false;
		Ok(Self { connection, queue, state })
	}

	pub(super) fn poll(&mut self) -> Result<bool, Error> {
		allow_would_block(self.connection.flush())?;
		self.queue.dispatch_pending(&mut self.state).map_err(unknown)?;
		if let Some(read) = self.queue.prepare_read() {
			// This connection has one reader. read() returns WouldBlock if no data is available.
			allow_would_block(read.read())?;
		}
		self.queue.dispatch_pending(&mut self.state).map_err(unknown)?;
		if self.state.finished {
			return Err(unknown("the Wayland clipboard device or seat is no longer available"));
		}
		Ok(std::mem::take(&mut self.state.changed))
	}
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
	fn event(
		state: &mut Self,
		_registry: &wl_registry::WlRegistry,
		event: wl_registry::Event,
		_data: &GlobalListContents,
		_connection: &Connection,
		_handle: &QueueHandle<Self>,
	) {
		if let wl_registry::Event::GlobalRemove { name } = event {
			if name == state.seat_name {
				state.finished = true;
			}
		}
	}
}

delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore ext_manager::ExtDataControlManagerV1);
delegate_noop!(State: ignore wlr_manager::ZwlrDataControlManagerV1);
delegate_noop!(State: ignore ext_offer::ExtDataControlOfferV1);
delegate_noop!(State: ignore wlr_offer::ZwlrDataControlOfferV1);

// Both protocols have the same event semantics. No offer data is requested.
macro_rules! device_dispatch {
	($device:ident, $device_type:ident, $offer:ident, $offer_type:ident) => {
		impl Dispatch<$device::$device_type, ()> for State {
			fn event(
				state: &mut Self,
				_device: &$device::$device_type,
				event: $device::Event,
				_data: &(),
				_connection: &Connection,
				_handle: &QueueHandle<Self>,
			) {
				match event {
					$device::Event::Selection { id } => {
						state.changed = true;
						if let Some(offer) = id {
							offer.destroy();
						}
					}
					$device::Event::PrimarySelection { id: Some(offer) } => {
						offer.destroy();
					}
					$device::Event::Finished => state.finished = true,
					_ => {}
				}
			}

			event_created_child!(State, $device::$device_type, [
				$device::EVT_DATA_OFFER_OPCODE => ($offer::$offer_type, ()),
			]);
		}
	};
}

device_dispatch!(ext_device, ExtDataControlDeviceV1, ext_offer, ExtDataControlOfferV1);
device_dispatch!(wlr_device, ZwlrDataControlDeviceV1, wlr_offer, ZwlrDataControlOfferV1);

#[cfg(test)]
mod tests {
	use super::*;
	use std::{
		io::{Read, Write},
		os::unix::net::UnixStream,
		sync::mpsc,
		thread,
		time::Duration,
	};

	enum Notification {
		Selection(u32),
		Primary(u32),
		Finished,
		SeatRemoved,
		Stop,
	}

	struct Server {
		send: mpsc::Sender<Notification>,
		applied: mpsc::Receiver<()>,
		thread: Option<thread::JoinHandle<()>>,
	}

	fn word(bytes: &[u8]) -> u32 {
		u32::from_ne_bytes(bytes[..4].try_into().unwrap())
	}

	fn string(value: &str) -> Vec<u8> {
		let mut bytes = ((value.len() + 1) as u32).to_ne_bytes().to_vec();
		bytes.extend_from_slice(value.as_bytes());
		bytes.push(0);
		while bytes.len() % 4 != 0 {
			bytes.push(0);
		}
		bytes
	}

	fn event(socket: &mut UnixStream, object: u32, opcode: u32, payload: &[u8]) {
		let header = [object, (((payload.len() + 8) as u32) << 16) | opcode];
		for word in header {
			socket.write_all(&word.to_ne_bytes()).unwrap();
		}
		socket.write_all(payload).unwrap();
	}

	fn global(socket: &mut UnixStream, registry: u32, name: u32, interface: &str, version: u32) {
		let mut payload = name.to_ne_bytes().to_vec();
		payload.extend(string(interface));
		payload.extend(version.to_ne_bytes());
		event(socket, registry, 0, &payload);
	}

	impl Server {
		fn start(protocol: Option<&'static str>) -> (Connection, Self) {
			let (client, mut socket) = UnixStream::pair().unwrap();
			socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
			let (send, receive) = mpsc::channel();
			let (applied_tx, applied) = mpsc::channel();
			let thread = thread::spawn(move || {
				let (mut registry, mut manager, mut device, mut syncs) = (0, 0, 0, 0);
				// Serve only the startup handshake. No clipboard payload protocol is implemented.
				loop {
					let mut header = [0u8; 8];
					if socket.read_exact(&mut header).is_err() {
						return;
					}
					let object = word(&header);
					let opcode = word(&header[4..]) & 0xffff;
					let size = (word(&header[4..]) >> 16) as usize;
					let mut payload = vec![0; size - 8];
					socket.read_exact(&mut payload).unwrap();
					match (object, opcode) {
						(1, 1) => {
							registry = word(&payload);
							global(&mut socket, registry, 1, "wl_seat", 1);
							if let Some(protocol) = protocol {
								let version = if protocol.starts_with("zwlr") { 2 } else { 1 };
								global(&mut socket, registry, 2, protocol, version);
							}
						}
						(1, 0) => {
							let callback = word(&payload);
							event(&mut socket, callback, 0, &0u32.to_ne_bytes());
							event(&mut socket, 1, 1, &callback.to_ne_bytes());
							syncs += 1;
							if syncs == 2 {
								break;
							}
						}
						(id, 0) if id == registry && word(&payload) == 2 => {
							manager = word(&payload[payload.len() - 4..]);
						}
						(id, 1) if id == manager => {
							device = word(&payload);
							event(&mut socket, device, 1, &0u32.to_ne_bytes());
						}
						_ => {}
					}
				}
				while let Ok(notification) = receive.recv() {
					match notification {
						Notification::Selection(offer) | Notification::Primary(offer) => {
							if offer != 0 {
								event(&mut socket, device, 0, &offer.to_ne_bytes());
								event(&mut socket, offer, 0, &string("text/plain"));
							}
							let opcode = match notification {
								Notification::Selection(_) => 1,
								_ => 3,
							};
							event(&mut socket, device, opcode, &offer.to_ne_bytes());
						}
						Notification::Finished => event(&mut socket, device, 2, &[]),
						Notification::SeatRemoved => {
							event(&mut socket, registry, 1, &1u32.to_ne_bytes());
						}
						Notification::Stop => break,
					}
					applied_tx.send(()).unwrap();
				}
			});
			(Connection::from_socket(client).unwrap(), Self { send, applied, thread: Some(thread) })
		}

		fn notify(&self, notification: Notification) {
			self.send.send(notification).unwrap();
			self.applied.recv_timeout(Duration::from_secs(3)).unwrap();
		}
	}

	impl Drop for Server {
		fn drop(&mut self) {
			let _ = self.send.send(Notification::Stop);
			self.thread.take().unwrap().join().unwrap();
		}
	}

	fn changes(protocol: &'static str) {
		let (connection, server) = Server::start(Some(protocol));
		let mut backend = Backend::from_connection(connection).unwrap();
		assert!(!backend.poll().unwrap());
		// Identical MIME lists still carry distinct selection events.
		for offer in [0xff00_0000, 0xff00_0001, 0] {
			server.notify(Notification::Selection(offer));
			assert!(backend.poll().unwrap());
			assert!(!backend.poll().unwrap());
		}
		server.notify(Notification::Primary(0xff00_0002));
		assert!(!backend.poll().unwrap());
		server.notify(Notification::Finished);
		assert!(backend.poll().is_err());
	}

	#[test]
	fn ext_selection_events() {
		changes("ext_data_control_manager_v1");
	}

	#[test]
	fn wlr_selection_events() {
		changes("zwlr_data_control_manager_v1");
	}

	#[test]
	fn missing_data_control_is_unsupported() {
		let (connection, _server) = Server::start(None);
		assert!(matches!(Backend::from_connection(connection), Err(Error::ClipboardNotSupported)));
	}

	#[test]
	fn removed_seat_is_terminal() {
		let (connection, server) = Server::start(Some("ext_data_control_manager_v1"));
		let mut backend = Backend::from_connection(connection).unwrap();
		server.notify(Notification::SeatRemoved);
		assert!(backend.poll().is_err());
	}
}
