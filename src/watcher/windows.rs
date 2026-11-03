use crate::Error;
use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;

pub(super) struct Backend {
	sequence: u32,
}

fn sequence() -> Result<u32, Error> {
	let sequence = unsafe { GetClipboardSequenceNumber() };
	if sequence == 0 {
		// This API does not document a GetLastError value.
		Err(super::unknown("the clipboard sequence number is unavailable"))
	} else {
		Ok(sequence)
	}
}

impl Backend {
	pub(super) fn new() -> Result<Self, Error> {
		Ok(Self { sequence: sequence()? })
	}

	pub(super) fn poll(&mut self) -> Result<bool, Error> {
		let sequence = sequence()?;
		let changed = sequence != self.sequence;
		self.sequence = sequence;
		Ok(changed)
	}
}
