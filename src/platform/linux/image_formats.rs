use crate::{Error, ImageData, ImageLimits};
use ::image::ImageFormat;
#[cfg(any(feature = "wayland-data-control", test))]
use std::io::Read;

const FORMATS: &[(&str, Option<ImageFormat>)] = &[
	("image/png", Some(ImageFormat::Png)),
	("image/jpeg", Some(ImageFormat::Jpeg)),
	("image/webp", Some(ImageFormat::WebP)),
	("image/bmp", Some(ImageFormat::Bmp)),
	("image/gif", Some(ImageFormat::Gif)),
	("image/jpg", Some(ImageFormat::Jpeg)),
	("image/x-ms-bmp", Some(ImageFormat::Bmp)),
	("image/x-bmp", Some(ImageFormat::Bmp)),
	("application/x-qt-image", None),
];

pub(super) fn read_image(
	limits: ImageLimits,
	mut read: impl FnMut(&str) -> Result<Vec<u8>, Error>,
) -> Result<ImageData<'static>, Error> {
	let mut invalid = false;
	for &(mime, format) in FORMATS {
		let bytes = match read(mime) {
			Ok(bytes) => bytes,
			Err(Error::ContentNotAvailable) => continue,
			Err(error) => return Err(error),
		};
		limits.check_encoded(bytes.len())?;
		let result = decode_payload(&bytes, format, limits);
		match result {
			Ok(image) => return Ok(image),
			Err(Error::ConversionFailure) => {
				log::debug!("Clipboard image conversion failed for {mime}; trying the next format");
				invalid = true;
			}
			Err(error) => return Err(error),
		}
	}
	Err(if invalid { Error::ConversionFailure } else { Error::ContentNotAvailable })
}

fn decode_payload(
	bytes: &[u8],
	format: Option<ImageFormat>,
	limits: ImageLimits,
) -> Result<ImageData<'static>, Error> {
	let (bytes, format) = match format {
		Some(format) => (bytes, format),
		None => {
			// Qt also serializes QImage with a big-endian QDataStream non-null marker.
			let bytes = bytes.strip_prefix(&[0, 0, 0, 1]).unwrap_or(bytes);
			let format = ::image::guess_format(bytes).map_err(|_| Error::ConversionFailure)?;
			if !FORMATS.iter().any(|(_, supported)| *supported == Some(format)) {
				return Err(Error::ConversionFailure);
			}
			(bytes, format)
		}
	};
	crate::image_read::decode(bytes, format, limits)
}

#[cfg(any(feature = "wayland-data-control", test))]
pub(super) fn read_bounded(reader: impl Read, limits: ImageLimits) -> Result<Vec<u8>, Error> {
	let mut bytes = Vec::new();
	let count = limits.max_encoded_bytes.map_or(u64::MAX, |limit| (limit as u64).saturating_add(1));
	reader.take(count).read_to_end(&mut bytes).map_err(super::into_unknown)?;
	limits.check_encoded(bytes.len())?;
	Ok(bytes)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{self, Cursor};

	const PNG: &[u8] = include_bytes!("../../../tests/fixtures/red.png");
	const JPEG: &[u8] = include_bytes!("../../../tests/fixtures/red.jpeg");
	const WEBP: &[u8] = include_bytes!("../../../tests/fixtures/red.webp");
	const BMP: &[u8] = include_bytes!("../../../tests/fixtures/red.bmp");
	const GIF: &[u8] = include_bytes!("../../../tests/fixtures/red.gif");
	const QT: &[u8] = include_bytes!("../../../tests/fixtures/qt-image.bin");

	#[test]
	fn fixture_formats_and_aliases() {
		for (mime, bytes) in [
			("image/png", PNG),
			("image/jpeg", JPEG),
			("image/webp", WEBP),
			("image/bmp", BMP),
			("image/gif", GIF),
			("image/gif", include_bytes!("../../../tests/fixtures/animated.gif")),
			("image/webp", include_bytes!("../../../tests/fixtures/animated.webp")),
			("image/jpg", JPEG),
			("image/x-ms-bmp", BMP),
			("image/x-bmp", BMP),
			("application/x-qt-image", QT),
			("application/x-qt-image", PNG),
		] {
			let limits = ImageLimits {
				max_encoded_bytes: Some(bytes.len()),
				max_width: Some(3),
				max_height: Some(2),
				max_decoded_bytes: Some(24),
			};
			let read = |requested: &str| {
				if requested == mime {
					Ok(bytes.to_vec())
				} else {
					Err(Error::ContentNotAvailable)
				}
			};
			let image = read_image(limits, read).unwrap();
			assert_eq!((image.width, image.height, image.bytes.len()), (3, 2, 24), "{mime}");
			let red = if mime.contains("jp") { 254 } else { 255 };
			assert!(image.bytes.chunks_exact(4).all(|pixel| pixel == [red, 0, 0, 255]), "{mime}");
			for smaller in [
				ImageLimits { max_encoded_bytes: Some(bytes.len() - 1), ..limits },
				ImageLimits { max_width: Some(2), ..limits },
				ImageLimits { max_height: Some(1), ..limits },
				ImageLimits { max_decoded_bytes: Some(23), ..limits },
			] {
				assert!(matches!(read_image(smaller, read), Err(Error::ImageTooLarge)), "{mime}");
			}
		}
	}

	#[test]
	fn canonical_preference_and_conversion_fallback() {
		let mut requested = Vec::new();
		let image = read_image(ImageLimits::default(), |mime| {
			requested.push(mime.to_owned());
			match mime {
				"image/png" => Ok(b"invalid PNG".to_vec()),
				"image/jpeg" => Ok(JPEG.to_vec()),
				_ => panic!("must stop at JPEG"),
			}
		})
		.unwrap();
		assert_eq!(&image.bytes[..4], &[254, 0, 0, 255]);
		assert_eq!(requested, ["image/png", "image/jpeg"]);
		let image = read_image(ImageLimits::default(), |mime| {
			assert_eq!(mime, "image/png");
			Ok(PNG.to_vec())
		})
		.unwrap();
		assert_eq!(&image.bytes[..4], &[255, 0, 0, 255]);
	}

	#[test]
	fn gif_frame_allocation_respects_decoded_limit() {
		let mut bytes = GIF.to_vec();
		bytes[6..10].copy_from_slice(&[1, 0, 1, 0]);
		let limits = ImageLimits {
			max_width: Some(1),
			max_height: Some(1),
			max_decoded_bytes: Some(4),
			..ImageLimits::default()
		};
		for limit in [4, 23] {
			assert!(matches!(
				decode_payload(
					&bytes,
					Some(ImageFormat::Gif),
					ImageLimits { max_decoded_bytes: Some(limit), ..limits },
				),
				Err(Error::ImageTooLarge)
			));
		}
		for limit in [None, Some(24)] {
			let image = decode_payload(
				&bytes,
				Some(ImageFormat::Gif),
				ImageLimits { max_decoded_bytes: limit, ..limits },
			)
			.unwrap();
			assert_eq!((image.width, image.height, image.bytes.len()), (1, 1, 4));
		}
		let mut requested = Vec::new();
		assert!(matches!(
			read_image(limits, |mime| {
				requested.push(mime.to_owned());
				if mime == "image/gif" {
					Ok(bytes.clone())
				} else {
					Err(Error::ContentNotAvailable)
				}
			}),
			Err(Error::ImageTooLarge)
		));
		assert_eq!(requested.last().unwrap(), "image/gif");
	}

	#[test]
	fn absence_conversion_and_transfer_errors_stay_distinct() {
		assert!(matches!(
			read_image(ImageLimits::default(), |_| Err(Error::ContentNotAvailable)),
			Err(Error::ContentNotAvailable)
		));
		assert!(matches!(
			read_image(ImageLimits::default(), |_| Ok(b"invalid".to_vec())),
			Err(Error::ConversionFailure)
		));
		let mut calls = 0;
		let result = read_image(ImageLimits::default(), |_| {
			calls += 1;
			Err(Error::ClipboardOccupied)
		});
		assert!(matches!(result, Err(Error::ClipboardOccupied)));
		assert_eq!(calls, 1);
		let result = read_image(
			ImageLimits { max_encoded_bytes: Some(0), ..ImageLimits::default() },
			|mime| {
				assert_eq!(mime, "image/png");
				Ok(PNG.to_vec())
			},
		);
		assert!(matches!(result, Err(Error::ImageTooLarge)));
	}

	#[test]
	fn mime_types_do_not_override_encoded_format() {
		assert!(matches!(
			decode_payload(JPEG, Some(ImageFormat::Png), ImageLimits::default()),
			Err(Error::ConversionFailure)
		));
		assert!(matches!(
			decode_payload(b"\0\0\0\0", None, ImageLimits::default()),
			Err(Error::ConversionFailure)
		));
	}

	#[test]
	fn bounded_streams_stop_at_one_extra_byte() {
		for size in [0, 1, 4, 5, 8] {
			let data = vec![42; size];
			let limits = ImageLimits { max_encoded_bytes: Some(size), ..ImageLimits::default() };
			assert_eq!(read_bounded(Cursor::new(&data), limits).unwrap(), data);
			if size > 0 {
				assert!(matches!(
					read_bounded(
						Cursor::new(&data),
						ImageLimits { max_encoded_bytes: Some(size - 1), ..limits }
					),
					Err(Error::ImageTooLarge)
				));
			}
		}
		let mut cursor = Cursor::new(vec![0; 100]);
		assert!(matches!(
			read_bounded(
				&mut cursor,
				ImageLimits { max_encoded_bytes: Some(4), ..ImageLimits::default() }
			),
			Err(Error::ImageTooLarge)
		));
		assert_eq!(cursor.position(), 5);
		struct Broken;
		impl Read for Broken {
			fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
				Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture read failure"))
			}
		}
		assert!(matches!(read_bounded(Broken, ImageLimits::default()), Err(Error::Unknown { .. })));
	}
}
