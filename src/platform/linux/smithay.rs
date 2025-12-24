use std::{borrow::Cow, ffi::c_void, time::Duration, time::Instant};

use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::common::Error;
use super::{into_unknown, LinuxClipboardKind, WaitConfig};

// smithay_clipboard provides a clipboard for Wayland clients using core protocols
use smithay_clipboard::Clipboard as SmithayClipboard;

// Store the raw display pointer. We keep this in a `OnceCell` and construct a per-`arboard::Clipboard`
// smithay worker from it.
static GLOBAL_DISPLAY: OnceCell<AtomicPtr<c_void>> = OnceCell::new();

/// Safety: `display` must be a valid `*mut wl_display` pointer which remains valid
/// for as long as the clipboard is used.
pub(super) unsafe fn init_from_display(display: *mut c_void) -> Result<(), Error> {
    if display.is_null() {
        return Err(Error::ClipboardNotSupported);
    }

    GLOBAL_DISPLAY
        .set(AtomicPtr::new(display))
        .map_err(|_| Error::ClipboardNotSupported)
}

pub(super) fn is_available() -> bool {
    GLOBAL_DISPLAY.get().is_some()
}

fn display_ptr() -> Result<*mut c_void, Error> {
    GLOBAL_DISPLAY
        .get()
    .map(|p| p.load(Ordering::SeqCst))
        .ok_or(Error::ClipboardNotSupported)
}

pub(crate) struct Clipboard {
    inner: SmithayClipboard,
}

impl Clipboard {
    pub(crate) fn new() -> Result<Self, Error> {
        let display = display_ptr()?;
        // Safety: `display` is set through the public `init_wayland_display`, which requires the
        // pointer to remain valid for the lifetime of the application.
        let inner = unsafe { SmithayClipboard::new(display) };
        Ok(Self { inner })
    }

    pub(crate) fn get_text(&mut self, selection: LinuxClipboardKind) -> Result<String, Error> {
        // smithay-clipboard uses the core `wl_data_device` clipboard selection only.
        match selection {
            LinuxClipboardKind::Clipboard => self.inner.load().map_err(into_unknown),
            LinuxClipboardKind::Primary | LinuxClipboardKind::Secondary => Err(Error::ClipboardNotSupported),
        }
    }

    pub(crate) fn set_text(
        &mut self,
        text: Cow<'_, str>,
        selection: LinuxClipboardKind,
        wait: WaitConfig,
        _exclude_from_history: bool,
    ) -> Result<(), Error> {
        let owned = text.into_owned();

        // smithay-clipboard uses the core `wl_data_device` clipboard selection only.
        match selection {
            LinuxClipboardKind::Clipboard => {
                self.inner.store(owned.clone());

                // `SetExtLinux::wait()` is expected to *block* the calling thread while continuing
                // to serve clipboard requests. Keeping `self` alive in this call achieves that.
                let poll = Duration::from_millis(250);
                match wait {
                    WaitConfig::None => Ok(()),
                    WaitConfig::Forever => {
                        loop {
                            match self.inner.load() {
                                Ok(current) => {
                                    if current != owned {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                            std::thread::sleep(poll);
                        }
                        Ok(())
                    }
                    WaitConfig::Until(deadline) => {
                        while Instant::now() < deadline {
                            match self.inner.load() {
                                Ok(current) => {
                                    if current != owned {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                            std::thread::sleep(poll);
                        }
                        Ok(())
                    }
                }
            }
            LinuxClipboardKind::Primary | LinuxClipboardKind::Secondary => Err(Error::ClipboardNotSupported),
        }
    }

    pub(crate) fn clear(&mut self, selection: LinuxClipboardKind) -> Result<(), Error> {
        // There is no explicit "clear" in the core Wayland clipboard protocol.
        // Storing an empty string is a best-effort approximation.
        match selection {
            LinuxClipboardKind::Clipboard => {
                self.inner.store(String::new());
                Ok(())
            }
            LinuxClipboardKind::Primary | LinuxClipboardKind::Secondary => Err(Error::ClipboardNotSupported),
        }
    }
}

// TODO: add image/file_list support in future iterations using the smithay worker APIs
