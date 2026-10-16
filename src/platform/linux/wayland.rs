use std::{
	borrow::Cow,
	io::Read,
	path::{Path, PathBuf},
};

use wl_clipboard_rs::{
	copy::{self, Error as CopyError, MimeSource, MimeType, Options, ServeRequests, Source},
	paste::{self, get_contents, Error as PasteError, Seat},
	utils::is_primary_selection_supported,
};

#[cfg(feature = "image-data")]
use super::encode_as_png;
use super::{
	gnome_copied_files_from_uri_list, into_unknown, nautilus_clipboard_from_uri_list,
	paths_from_uri_list, paths_to_uri_list, LinuxClipboardKind, WaitConfig,
	GNOME_COPIED_FILES_MIME, KDE_EXCLUSION_HINT, KDE_EXCLUSION_MIME, NAUTILUS_CLIPBOARD_MIME,
	URI_LIST_MIME,
};
use crate::common::Error;
#[cfg(feature = "image-data")]
use crate::common::ImageData;
use crate::ClipboardFormat;

#[cfg(feature = "image-data")]
const MIME_PNG: &str = "image/png";

pub(crate) struct Clipboard {}

impl TryInto<copy::ClipboardType> for LinuxClipboardKind {
	type Error = Error;

	fn try_into(self) -> Result<copy::ClipboardType, Self::Error> {
		match self {
			LinuxClipboardKind::Clipboard => Ok(copy::ClipboardType::Regular),
			LinuxClipboardKind::Primary => Ok(copy::ClipboardType::Primary),
			LinuxClipboardKind::Secondary => Err(Error::ClipboardNotSupported),
		}
	}
}

impl TryInto<paste::ClipboardType> for LinuxClipboardKind {
	type Error = Error;

	fn try_into(self) -> Result<paste::ClipboardType, Self::Error> {
		match self {
			LinuxClipboardKind::Clipboard => Ok(paste::ClipboardType::Regular),
			LinuxClipboardKind::Primary => Ok(paste::ClipboardType::Primary),
			LinuxClipboardKind::Secondary => Err(Error::ClipboardNotSupported),
		}
	}
}

fn add_clipboard_exclusions(exclude_from_history: bool, sources: &mut Vec<MimeSource>) {
	if exclude_from_history {
		sources.push(MimeSource {
			source: Source::Bytes(Box::from(KDE_EXCLUSION_HINT)),
			mime_type: MimeType::Specific(String::from(KDE_EXCLUSION_MIME)),
		});
	}
}

fn options(wait: WaitConfig, selection: LinuxClipboardKind) -> Result<Options, Error> {
	let mut opts = Options::new();
	opts.foreground(matches!(wait, WaitConfig::Forever));
	if matches!(wait, WaitConfig::Once) {
		opts.serve_requests(ServeRequests::Only(1));
	}
	opts.clipboard(selection.try_into()?);
	Ok(opts)
}

fn handle_copy_error(e: copy::Error) -> Error {
	match e {
		CopyError::PrimarySelectionUnsupported => Error::ClipboardNotSupported,
		other => into_unknown(other),
	}
}

fn handle_paste_error(e: paste::Error) -> Error {
	match e {
		PasteError::PrimarySelectionUnsupported => Error::ClipboardNotSupported,
		other => into_unknown(other),
	}
}

fn handle_clipboard_read<T, F: FnOnce(Vec<u8>) -> Result<T, Error>>(
	selection: LinuxClipboardKind,
	mime: paste::MimeType,
	into_requested_data: F,
) -> Result<T, Error> {
	let result = get_contents(selection.try_into()?, Seat::Unspecified, mime);
	match result {
		Ok((mut pipe, _)) => {
			let mut buffer = vec![];
			pipe.read_to_end(&mut buffer).map_err(into_unknown)?;
			into_requested_data(buffer)
		}
		Err(PasteError::ClipboardEmpty) | Err(PasteError::NoMimeType) => {
			Err(Error::ContentNotAvailable)
		}
		Err(err) => Err(handle_paste_error(err)),
	}
}

impl Clipboard {
	pub(crate) fn formats(
		&self,
		selection: LinuxClipboardKind,
	) -> Result<Vec<ClipboardFormat>, Error> {
		match paste::get_mime_types(selection.try_into()?, Seat::Unspecified) {
			Ok(names) => Ok(names.into_iter().map(|native| ClipboardFormat { native }).collect()),
			Err(PasteError::ClipboardEmpty) => Ok(Vec::new()),
			Err(error) => Err(handle_paste_error(error)),
		}
	}

	pub(crate) fn raw(
		&self,
		format: &ClipboardFormat,
		selection: LinuxClipboardKind,
	) -> Result<Vec<u8>, Error> {
		handle_clipboard_read(selection, paste::MimeType::Specific(&format.native), Ok)
	}

	pub(crate) fn set_formats(
		&self,
		data: Vec<(ClipboardFormat, Vec<u8>)>,
		selection: LinuxClipboardKind,
		wait: WaitConfig,
	) -> Result<(), Error> {
		let deadline = match wait {
			WaitConfig::Until(deadline) => Some(deadline),
			_ => None,
		};
		let wait_once = matches!(wait, WaitConfig::Once);
		let sources = data
			.into_iter()
			.map(|(format, bytes)| MimeSource {
				source: Source::Bytes(bytes.into_boxed_slice()),
				mime_type: MimeType::Specific(format.native),
			})
			.collect();
		let mut opts = options(wait, selection)?;
		// Raw HTML or RTF must not become an implicit plain-text offer.
		opts.omit_additional_text_mime_types(true);
		if wait_once {
			opts.foreground(true);
		}
		if let Some(deadline) = deadline {
			use std::sync::mpsc;
			opts.foreground(true);
			let (ready_tx, ready_rx) = mpsc::sync_channel(1);
			let (done_tx, done_rx) = mpsc::sync_channel(1);
			std::thread::Builder::new()
				.spawn(move || match opts.prepare_copy_multi(sources) {
					Ok(copy) => {
						if ready_tx.send(Ok(())).is_err() {
							return;
						}
						let result = copy.serve().map_err(handle_copy_error);
						if let Err(mpsc::SendError(Err(error))) = done_tx.send(result) {
							log::warn!(
								"Clipboard transfer failed after the wait deadline: {error}"
							);
						}
					}
					Err(error) => {
						let _ = ready_tx.send(Err(handle_copy_error(error)));
					}
				})
				.map_err(into_unknown)?;
			ready_rx.recv().map_err(into_unknown)??;
			return match done_rx
				.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
			{
				Ok(result) => result,
				Err(mpsc::RecvTimeoutError::Timeout) => Ok(()),
				Err(error) => Err(into_unknown(error)),
			};
		}
		opts.copy_multi(sources).map_err(handle_copy_error)
	}

	pub(crate) fn new() -> Result<Self, Error> {
		// Check if it's possible to communicate with the wayland compositor
		match is_primary_selection_supported() {
			// We don't care if the primary clipboard is supported or not, `wl-clipboard-rs` will fail
			// if not and we don't want to duplicate more of their logic.
			Ok(_) => Ok(Self {}),
			Err(e) => Err(into_unknown(e)),
		}
	}

	pub(crate) fn clear(&mut self, selection: LinuxClipboardKind) -> Result<(), Error> {
		let selection = selection.try_into()?;
		copy::clear(selection, copy::Seat::All).map_err(handle_copy_error)
	}

	pub(crate) fn get_text(&mut self, selection: LinuxClipboardKind) -> Result<String, Error> {
		handle_clipboard_read(selection, paste::MimeType::Text, |contents| {
			String::from_utf8(contents).map_err(|_| Error::ConversionFailure)
		})
	}

	pub(crate) fn set_text(
		&self,
		text: Cow<'_, str>,
		selection: LinuxClipboardKind,
		wait: WaitConfig,
		exclude_from_history: bool,
	) -> Result<(), Error> {
		let opts = options(wait, selection)?;
		let mut sources = Vec::with_capacity(if exclude_from_history { 2 } else { 1 });

		sources.push(MimeSource {
			source: Source::Bytes(text.into_owned().into_bytes().into_boxed_slice()),
			mime_type: MimeType::Text,
		});

		add_clipboard_exclusions(exclude_from_history, &mut sources);

		opts.copy_multi(sources).map_err(handle_copy_error)
	}

	pub(crate) fn get_html(&mut self, selection: LinuxClipboardKind) -> Result<String, Error> {
		handle_clipboard_read(selection, paste::MimeType::Specific("text/html"), |contents| {
			String::from_utf8(contents).map_err(|_| Error::ConversionFailure)
		})
	}

	pub(crate) fn set_html(
		&self,
		html: Cow<'_, str>,
		alt: Option<Cow<'_, str>>,
		selection: LinuxClipboardKind,
		wait: WaitConfig,
		exclude_from_history: bool,
	) -> Result<(), Error> {
		let opts = options(wait, selection)?;
		let mut sources = {
			let cap = [true, alt.is_some(), exclude_from_history]
				.map(|v| usize::from(v as u8))
				.iter()
				.sum();
			Vec::with_capacity(cap)
		};

		if let Some(alt) = alt {
			sources.push(MimeSource {
				source: Source::Bytes(alt.into_owned().into_bytes().into_boxed_slice()),
				mime_type: MimeType::Text,
			});
		}

		sources.push(MimeSource {
			source: Source::Bytes(html.into_owned().into_bytes().into_boxed_slice()),
			mime_type: MimeType::Specific(String::from("text/html")),
		});

		add_clipboard_exclusions(exclude_from_history, &mut sources);

		opts.copy_multi(sources).map_err(handle_copy_error)
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn get_image(
		&mut self,
		selection: LinuxClipboardKind,
	) -> Result<ImageData<'static>, Error> {
		use std::io::Cursor;

		handle_clipboard_read(selection, paste::MimeType::Specific(MIME_PNG), |buffer| {
			let image = image::io::Reader::new(Cursor::new(buffer))
				.with_guessed_format()
				.map_err(|_| Error::ConversionFailure)?
				.decode()
				.map_err(|_| Error::ConversionFailure)?;
			let image = image.into_rgba8();

			Ok(ImageData {
				width: image.width() as usize,
				height: image.height() as usize,
				bytes: image.into_raw().into(),
			})
		})
	}

	#[cfg(feature = "image-data")]
	pub(crate) fn set_image(
		&mut self,
		image: ImageData,
		selection: LinuxClipboardKind,
		wait: WaitConfig,
		exclude_from_history: bool,
	) -> Result<(), Error> {
		let opts = options(wait, selection)?;
		let image = encode_as_png(&image)?;

		let mut sources = Vec::with_capacity(if exclude_from_history { 2 } else { 1 });

		sources.push(MimeSource {
			source: Source::Bytes(image.into()),
			mime_type: MimeType::Specific(String::from(MIME_PNG)),
		});

		add_clipboard_exclusions(exclude_from_history, &mut sources);

		opts.copy_multi(sources).map_err(handle_copy_error)
	}

	pub(crate) fn get_file_list(
		&mut self,
		selection: LinuxClipboardKind,
	) -> Result<Vec<PathBuf>, Error> {
		for mime in [URI_LIST_MIME, GNOME_COPIED_FILES_MIME, NAUTILUS_CLIPBOARD_MIME] {
			match handle_clipboard_read(
				selection,
				paste::MimeType::Specific(mime),
				paths_from_uri_list,
			) {
				Err(Error::ContentNotAvailable) => continue,
				result => return result,
			}
		}
		Err(Error::ContentNotAvailable)
	}

	pub(crate) fn set_file_list(
		&self,
		file_list: &[impl AsRef<Path>],
		selection: LinuxClipboardKind,
		wait: WaitConfig,
		exclude_from_history: bool,
	) -> Result<(), Error> {
		let opts = options(wait, selection)?;
		let files = paths_to_uri_list(file_list)?;
		let gnome_files = gnome_copied_files_from_uri_list(&files);
		let nautilus_files = nautilus_clipboard_from_uri_list(&files);

		let mut sources = Vec::with_capacity(if exclude_from_history { 4 } else { 3 });
		sources.push(MimeSource {
			source: Source::Bytes(files.into_bytes().into_boxed_slice()),
			mime_type: MimeType::Specific(String::from(URI_LIST_MIME)),
		});
		sources.push(MimeSource {
			source: Source::Bytes(gnome_files.into_boxed_slice()),
			mime_type: MimeType::Specific(String::from(GNOME_COPIED_FILES_MIME)),
		});
		sources.push(MimeSource {
			source: Source::Bytes(nautilus_files.into_boxed_slice()),
			mime_type: MimeType::Specific(String::from(NAUTILUS_CLIPBOARD_MIME)),
		});

		add_clipboard_exclusions(exclude_from_history, &mut sources);

		opts.copy_multi(sources).map_err(handle_copy_error)
	}
}
