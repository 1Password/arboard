use crate::Error;
use objc2::{
	msg_send,
	rc::{autoreleasepool, Retained},
	ClassType,
};
use objc2_app_kit::NSPasteboard;

pub(super) struct Backend {
	pasteboard: Retained<NSPasteboard>,
	count: isize,
}

impl Backend {
	pub(super) fn new() -> Result<Self, Error> {
		autoreleasepool(|_| {
			// Some launchd sessions have no pasteboard, despite the non-null API annotation.
			let pasteboard: Option<Retained<NSPasteboard>> =
				unsafe { msg_send![NSPasteboard::class(), generalPasteboard] };
			let pasteboard = pasteboard.ok_or(Error::ClipboardNotSupported)?;
			let count = unsafe { pasteboard.changeCount() };
			Ok(Self { pasteboard, count })
		})
	}

	pub(super) fn poll(&mut self) -> Result<bool, Error> {
		autoreleasepool(|_| {
			let count = unsafe { self.pasteboard.changeCount() };
			let changed = count != self.count;
			self.count = count;
			Ok(changed)
		})
	}
}
