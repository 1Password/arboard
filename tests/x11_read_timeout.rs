#![cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]

use std::{
	sync::{
		atomic::{AtomicBool, AtomicUsize, Ordering},
		Arc,
	},
	time::{Duration, Instant},
};

use arboard::{Clipboard, Error, GetExtLinux};
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
	COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
};

#[test]
fn bounds_reads_from_an_unresponsive_owner() {
	assert!(std::env::var_os("WAYLAND_DISPLAY").is_none(), "run this test under X11");
	let (owner, screen) = x11rb::connect(None).unwrap();
	let window = owner.generate_id().unwrap();
	owner
		.create_window(
			COPY_DEPTH_FROM_PARENT,
			window,
			owner.setup().roots[screen].root,
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
	let selection = owner.intern_atom(false, b"CLIPBOARD").unwrap().reply().unwrap().atom;
	owner.set_selection_owner(window, selection, CURRENT_TIME).unwrap().check().unwrap();

	let mut clipboard = Clipboard::new().unwrap();
	let timeout = Duration::from_millis(80);
	let start = Instant::now();
	assert!(matches!(clipboard.get().read_timeout(timeout).text(), Err(Error::Timeout)));
	let elapsed = start.elapsed();
	assert!(elapsed >= timeout, "returned before the deadline: {elapsed:?}");
	assert!(elapsed < Duration::from_millis(400), "reset the deadline: {elapsed:?}");
	assert!(matches!(clipboard.get().read_timeout(timeout).html(), Err(Error::Timeout)));
	assert!(matches!(clipboard.get().read_timeout(timeout).file_list(), Err(Error::Timeout)));
	#[cfg(feature = "image-data")]
	assert!(matches!(clipboard.get().read_timeout(timeout).image(), Err(Error::Timeout)));

	owner
		.set_selection_owner(u32::from(AtomEnum::NONE), selection, CURRENT_TIME)
		.unwrap()
		.check()
		.unwrap();
	clipboard.set_text("local value").unwrap();
	assert!(matches!(clipboard.get().read_timeout(Duration::ZERO).text(), Err(Error::Timeout)));
	assert_eq!(clipboard.get_text().unwrap(), "local value");
	assert_eq!(clipboard.get().read_timeout(timeout).text().unwrap(), "local value");

	owner.set_selection_owner(window, selection, CURRENT_TIME).unwrap().check().unwrap();
	let incr = owner.intern_atom(false, b"INCR").unwrap().reply().unwrap().atom;
	while owner.poll_for_event().unwrap().is_some() {}
	let stop = Arc::new(AtomicBool::new(false));
	let incremental = Arc::new(AtomicBool::new(false));
	let requests = Arc::new(AtomicUsize::new(0));
	std::thread::scope(|scope| {
		let stop_worker = Arc::clone(&stop);
		let incremental_worker = Arc::clone(&incremental);
		let requests_worker = Arc::clone(&requests);
		let worker = scope.spawn(move || {
			let mut transfer = None;
			while !stop_worker.load(Ordering::Relaxed) {
				match owner.poll_for_event().unwrap() {
					Some(Event::SelectionRequest(request)) => {
						requests_worker.fetch_add(1, Ordering::Relaxed);
						let property = if incremental_worker.load(Ordering::Relaxed) {
							owner
								.change_window_attributes(
									request.requestor,
									&ChangeWindowAttributesAux::new()
										.event_mask(EventMask::PROPERTY_CHANGE),
								)
								.unwrap();
							owner
								.change_property32(
									PropMode::REPLACE,
									request.requestor,
									request.property,
									incr,
									&[1024],
								)
								.unwrap();
							transfer = Some((request.requestor, request.property, request.target));
							request.property
						} else {
							std::thread::sleep(Duration::from_millis(50));
							x11rb::NONE
						};
						owner
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
									property,
								},
							)
							.unwrap();
						owner.flush().unwrap();
					}
					Some(Event::PropertyNotify(event)) if event.state == Property::DELETE => {
						if let Some((requestor, property, target)) = transfer {
							if event.window == requestor && event.atom == property {
								std::thread::sleep(Duration::from_millis(30));
								owner
									.change_property8(
										PropMode::REPLACE,
										requestor,
										property,
										target,
										b"chunk",
									)
									.unwrap();
								owner.flush().unwrap();
							}
						}
					}
					_ => std::thread::sleep(Duration::from_millis(1)),
				}
			}
		});
		let start = Instant::now();
		let result = clipboard.get().read_timeout(timeout).text();
		let elapsed = start.elapsed();
		let request_count = requests.load(Ordering::Relaxed);

		// Finish a pending rejection before changing how the owner answers requests.
		std::thread::sleep(Duration::from_millis(70));
		incremental.store(true, Ordering::Relaxed);
		let incr_start = Instant::now();
		let incr_result = clipboard.get().read_timeout(timeout).text();
		let incr_elapsed = incr_start.elapsed();
		// Stop the fixture before assertions so a failure cannot leave its worker running.
		stop.store(true, Ordering::Relaxed);
		worker.join().unwrap();

		assert!(matches!(result, Err(Error::Timeout)));
		assert!(request_count >= 2, "did not exercise format fallback");
		assert!(elapsed < Duration::from_millis(200), "reset on format fallback: {elapsed:?}");
		assert!(matches!(incr_result, Err(Error::Timeout)), "{incr_result:?}");
		assert!(incr_elapsed < Duration::from_millis(200), "reset on INCR: {incr_elapsed:?}");
	});
	println!("All read operations honor the explicit deadline; default reads still work.");
}
