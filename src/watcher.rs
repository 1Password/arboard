use std::{
	sync::{mpsc, Arc, Condvar, Mutex},
	thread::{self, JoinHandle},
	time::Duration,
};

use crate::Error;

#[cfg(target_os = "macos")]
#[path = "watcher/macos.rs"]
mod native;
#[cfg(windows)]
#[path = "watcher/windows.rs"]
mod native;
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
#[path = "watcher/linux.rs"]
mod native;

const INTERVAL: Duration = Duration::from_millis(50);

/// A notification from a [`ClipboardWatcher`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardEvent {
	/// The clipboard may have changed. Read its current contents with [`crate::Clipboard`].
	///
	/// The first event reports the initial state, even when the clipboard is empty.
	/// Events carry no payload or revision. Multiple changes can produce one event.
	Changed,
	/// The watcher stopped. All subsequent waits return this event.
	Stopped,
}

/// An independent watcher for the regular clipboard.
///
/// Construction opens a native connection and starts one worker thread. No [`crate::Clipboard`]
/// instance is required. The watcher never reads clipboard payloads or invokes user callbacks.
/// It does not retain clipboard contents or take clipboard ownership.
///
/// After successful construction, one initial [`ClipboardEvent::Changed`] event is available.
/// At most one change event remains pending: changes coalesce until a wait consumes that event.
/// A notification is a hint to read the current clipboard, not a snapshot or a count of writes.
/// A write of identical data can cause a notification. A read after an event can see newer data.
///
/// [`stop`](Self::stop) and `Drop` stop and join the worker. A stop discards pending changes and
/// wakes all waits. Share the watcher through `Arc` to stop it from another thread.
/// Concurrent waits compete for change events; they do not each receive a copy.
///
/// # Platform support
///
/// - Windows checks `GetClipboardSequenceNumber` every 50 ms without opening the clipboard.
///   Zero means that the sequence is unavailable and causes an error. Delayed rendering can
///   defer an update. Counter wraparound or several changes between checks can hide changes.
/// - macOS checks `NSPasteboard.changeCount` every 50 ms. Counter wraparound can hide changes.
/// - X11 uses XFixes selection notifications on a separate connection. XFixes 1.0 is required.
///   Changes to data without a selection ownership notification are not observable.
/// - With `wayland-data-control`, a session with `WAYLAND_DISPLAY` or `WAYLAND_SOCKET` uses
///   Wayland. The watcher requires `ext-data-control-v1` or `wlr-data-control-unstable-v1`.
///   It observes selection events for the first advertised seat, including empty selections.
///   It does not compare MIME lists. A removed seat or a revoked device terminates the watcher.
///   An unsupported compositor returns [`Error::ClipboardNotSupported`]; there is no silent
///   X11 fallback. Without this feature, the watcher observes X11/XWayland only.
///
/// Native event queues are checked every 50 ms. This interval is not a delivery deadline.
/// Startup can block while the native server responds. After startup, stop interrupts the
/// interval wait; it does not need a clipboard change. Native failures reach the next wait
/// once as an error, then waits return [`ClipboardEvent::Stopped`].
/// A terminal error takes precedence over a pending change, including the initial event.
///
/// # Example
///
/// ```no_run
/// use arboard::{ClipboardEvent, ClipboardWatcher};
/// use std::time::Duration;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let watcher = ClipboardWatcher::new()?;
/// assert_eq!(watcher.wait()?, ClipboardEvent::Changed); // Initial state.
/// if watcher.wait_timeout(Duration::from_secs(10))? == Some(ClipboardEvent::Changed) {
///     println!("The clipboard changed.");
/// }
/// watcher.stop()?;
/// assert_eq!(watcher.wait()?, ClipboardEvent::Stopped);
/// # Ok(())
/// # }
/// ```
pub struct ClipboardWatcher {
	shared: Arc<Shared>,
	worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Default)]
struct State {
	changed: bool,
	stopped: bool,
	error: Option<Error>,
}

#[derive(Default)]
struct Shared {
	state: Mutex<State>,
	wake: Condvar,
}

fn unknown(error: impl std::fmt::Display) -> Error {
	Error::Unknown { description: error.to_string() }
}

impl ClipboardWatcher {
	/// Starts a watcher and waits for native initialization.
	///
	/// Returns an error if the clipboard, notification protocol, or worker is unavailable.
	pub fn new() -> Result<Self, Error> {
		Self::spawn(|| {
			let mut backend = native::Backend::new()?;
			Ok(move || backend.poll())
		})
	}

	fn spawn<F, P>(initialize: F) -> Result<Self, Error>
	where
		F: FnOnce() -> Result<P, Error> + Send + 'static,
		P: FnMut() -> Result<bool, Error>,
	{
		let shared = Arc::new(Shared::default());
		let worker_shared = shared.clone();
		let (ready_tx, ready_rx) = mpsc::sync_channel(1);
		let worker = thread::Builder::new()
			.name("arboard-watcher".into())
			.spawn(move || {
				let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
					let mut poll = match initialize() {
						Ok(poll) => poll,
						Err(error) => {
							let _ = ready_tx.send(Err(error));
							return;
						}
					};
					{
						let mut state = worker_shared.state.lock().unwrap();
						state.changed = true;
					}
					let _ = ready_tx.send(Ok(()));
					loop {
						let state = worker_shared.state.lock().unwrap();
						let (state, _) = worker_shared
							.wake
							.wait_timeout_while(state, INTERVAL, |state| !state.stopped)
							.unwrap();
						if state.stopped {
							break;
						}
						drop(state);
						match poll() {
							Ok(false) => {}
							Ok(true) => {
								worker_shared.state.lock().unwrap().changed = true;
								worker_shared.wake.notify_all();
							}
							Err(error) => {
								worker_shared.finish(Some(error));
								break;
							}
						}
					}
				}));
				if result.is_err() {
					worker_shared.finish(Some(unknown("clipboard watcher worker panicked")));
				}
			})
			.map_err(unknown)?;
		match ready_rx.recv() {
			Ok(Ok(())) => Ok(Self { shared, worker: Mutex::new(Some(worker)) }),
			result => {
				let _ = worker.join();
				Err(match result {
					Ok(Err(error)) => error,
					_ => unknown("clipboard watcher worker failed during startup"),
				})
			}
		}
	}

	/// Waits for a change, a terminal error, or a stop.
	pub fn wait(&self) -> Result<ClipboardEvent, Error> {
		let state = self.shared.state.lock().unwrap();
		let mut state =
			self.shared.wake.wait_while(state, |state| !state.changed && !state.stopped).unwrap();
		Self::take_event(&mut state).map(Option::unwrap)
	}

	/// Waits up to `timeout`. Returns `None` only when the timeout expires without an event.
	///
	/// A zero timeout checks for a pending event without a wait.
	pub fn wait_timeout(&self, timeout: Duration) -> Result<Option<ClipboardEvent>, Error> {
		let state = self.shared.state.lock().unwrap();
		let (mut state, _) = self
			.shared
			.wake
			.wait_timeout_while(state, timeout, |state| !state.changed && !state.stopped)
			.unwrap();
		Self::take_event(&mut state)
	}

	fn take_event(state: &mut State) -> Result<Option<ClipboardEvent>, Error> {
		if let Some(error) = state.error.take() {
			return Err(error);
		}
		if state.stopped {
			Ok(Some(ClipboardEvent::Stopped))
		} else if std::mem::take(&mut state.changed) {
			Ok(Some(ClipboardEvent::Changed))
		} else {
			Ok(None)
		}
	}

	/// Stops the worker, discards pending changes, wakes waits, and joins the worker thread.
	///
	/// This method is idempotent. A pending native error remains available to the next wait.
	/// Returns an error only if the worker thread could not be joined.
	pub fn stop(&self) -> Result<(), Error> {
		self.shared.finish(None);
		// Hold the join lock so every concurrent stop waits for the worker to exit.
		if let Some(worker) = self.worker.lock().unwrap().take() {
			worker.join().map_err(|_| unknown("clipboard watcher worker panicked"))?;
		}
		Ok(())
	}
}

impl Shared {
	fn finish(&self, error: Option<Error>) {
		let mut state = self.state.lock().unwrap();
		state.stopped = true;
		state.changed = false;
		if error.is_some() {
			state.error = error;
		}
		self.wake.notify_all();
	}
}

impl Drop for ClipboardWatcher {
	fn drop(&mut self) {
		let _ = self.stop();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicBool, Ordering};

	#[test]
	fn initial_timeout_and_repeated_stop() {
		let watcher = ClipboardWatcher::spawn(|| Ok(|| Ok(false))).unwrap();
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Changed);
		assert_eq!(watcher.wait_timeout(Duration::ZERO).unwrap(), None);
		assert_eq!(watcher.wait_timeout(Duration::from_millis(10)).unwrap(), None);
		watcher.stop().unwrap();
		watcher.stop().unwrap();
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Stopped);
	}

	#[test]
	fn changes_coalesce() {
		let (tx, rx) = mpsc::channel();
		let enabled = Arc::new(AtomicBool::new(true));
		let worker_enabled = enabled.clone();
		let watcher = ClipboardWatcher::spawn(move || {
			Ok(move || {
				let changed = worker_enabled.load(Ordering::SeqCst);
				tx.send(changed).unwrap();
				Ok(changed)
			})
		})
		.unwrap();
		for _ in 0..3 {
			rx.recv_timeout(Duration::from_secs(3)).unwrap();
		}
		enabled.store(false, Ordering::SeqCst);
		while rx.recv_timeout(Duration::from_secs(3)).unwrap() {}
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Changed);
		assert_eq!(watcher.wait_timeout(INTERVAL * 2).unwrap(), None);
	}

	#[test]
	fn native_error_is_terminal_and_delivered_once() {
		let (tx, rx) = mpsc::channel();
		let watcher = ClipboardWatcher::spawn(move || {
			Ok(move || match rx.try_recv() {
				Ok(()) => Err(Error::ClipboardOccupied),
				_ => Ok(false),
			})
		})
		.unwrap();
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Changed);
		tx.send(()).unwrap();
		assert!(matches!(watcher.wait(), Err(Error::ClipboardOccupied)));
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Stopped);
		watcher.stop().unwrap();
	}

	#[test]
	fn timeout_wait_wakes_on_change() {
		let (tx, rx) = mpsc::channel();
		let watcher =
			ClipboardWatcher::spawn(move || Ok(move || Ok(rx.try_recv().is_ok()))).unwrap();
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Changed);
		tx.send(()).unwrap();
		assert_eq!(
			watcher.wait_timeout(Duration::from_secs(3)).unwrap(),
			Some(ClipboardEvent::Changed)
		);
		assert_eq!(watcher.wait_timeout(Duration::ZERO).unwrap(), None);
	}

	#[test]
	fn startup_error_and_panic_reach_caller() {
		assert!(matches!(
			ClipboardWatcher::spawn(|| Err::<fn() -> Result<bool, Error>, _>(
				Error::ClipboardNotSupported
			)),
			Err(Error::ClipboardNotSupported)
		));
		assert!(ClipboardWatcher::spawn(|| -> Result<fn() -> Result<bool, Error>, Error> {
			panic!("startup failure")
		})
		.is_err());
	}

	#[test]
	fn worker_panic_wakes_waiter() {
		let (tx, rx) = mpsc::channel();
		let watcher = ClipboardWatcher::spawn(move || {
			Ok(move || {
				assert!(rx.try_recv().is_err(), "poll failure");
				Ok(false)
			})
		})
		.unwrap();
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Changed);
		tx.send(()).unwrap();
		assert!(watcher.wait_timeout(Duration::from_secs(3)).is_err());
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Stopped);
	}

	struct Dropped(Arc<AtomicBool>);

	impl Drop for Dropped {
		fn drop(&mut self) {
			self.0.store(true, Ordering::SeqCst);
		}
	}

	fn idle_watcher(dropped: Arc<AtomicBool>) -> ClipboardWatcher {
		ClipboardWatcher::spawn(move || {
			let guard = Dropped(dropped);
			Ok(move || {
				let _ = &guard;
				Ok(false)
			})
		})
		.unwrap()
	}

	#[test]
	fn stop_wakes_waiters_and_joins_worker() {
		let dropped = Arc::new(AtomicBool::new(false));
		let watcher = Arc::new(idle_watcher(dropped.clone()));
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Changed);
		let (tx, rx) = mpsc::channel();
		let waiter = watcher.clone();
		let waiting = thread::spawn(move || tx.send(waiter.wait()).unwrap());
		watcher.stop().unwrap();
		assert_eq!(
			rx.recv_timeout(Duration::from_secs(3)).unwrap().unwrap(),
			ClipboardEvent::Stopped
		);
		assert!(dropped.load(Ordering::SeqCst));
		waiting.join().unwrap();
	}

	#[test]
	fn concurrent_stops_wait_for_worker_cleanup() {
		let (entered_tx, entered_rx) = mpsc::channel();
		let (release_tx, release_rx) = mpsc::channel();
		let watcher = Arc::new(
			ClipboardWatcher::spawn(move || {
				Ok(move || {
					entered_tx.send(()).unwrap();
					release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
					Ok(false)
				})
			})
			.unwrap(),
		);
		entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
		let (stopped_tx, stopped_rx) = mpsc::channel();
		let mut threads = Vec::new();
		for _ in 0..2 {
			let watcher = watcher.clone();
			let stopped_tx = stopped_tx.clone();
			threads.push(thread::spawn(move || {
				watcher.stop().unwrap();
				stopped_tx.send(()).unwrap();
			}));
		}
		assert!(stopped_rx.recv_timeout(INTERVAL).is_err());
		release_tx.send(()).unwrap();
		for thread in threads {
			stopped_rx.recv_timeout(Duration::from_secs(3)).unwrap();
			thread.join().unwrap();
		}
	}

	#[test]
	fn drop_joins_worker_and_stop_preserves_error() {
		let dropped = Arc::new(AtomicBool::new(false));
		drop(idle_watcher(dropped.clone()));
		assert!(dropped.load(Ordering::SeqCst));

		let watcher = idle_watcher(Arc::new(AtomicBool::new(false)));
		watcher.shared.finish(Some(Error::ClipboardOccupied));
		watcher.stop().unwrap();
		assert!(matches!(watcher.wait(), Err(Error::ClipboardOccupied)));
		assert_eq!(watcher.wait().unwrap(), ClipboardEvent::Stopped);
	}
}
