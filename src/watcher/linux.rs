use crate::Error;

#[cfg(feature = "wayland-data-control")]
#[path = "wayland.rs"]
mod wayland;
#[path = "x11.rs"]
mod x11;

pub(super) enum Backend {
	X11(Box<x11::Backend>),
	#[cfg(feature = "wayland-data-control")]
	Wayland(wayland::Backend),
}

impl Backend {
	pub(super) fn new() -> Result<Self, Error> {
		#[cfg(feature = "wayland-data-control")]
		if std::env::var_os("WAYLAND_DISPLAY").is_some()
			|| std::env::var_os("WAYLAND_SOCKET").is_some()
		{
			return wayland::Backend::new().map(Self::Wayland);
		}
		x11::Backend::new().map(|backend| Self::X11(Box::new(backend)))
	}

	pub(super) fn poll(&mut self) -> Result<bool, Error> {
		match self {
			Self::X11(backend) => backend.poll(),
			#[cfg(feature = "wayland-data-control")]
			Self::Wayland(backend) => backend.poll(),
		}
	}
}
