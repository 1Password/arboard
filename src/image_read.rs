use crate::{Error, ImageData};
#[cfg(any(not(windows), test))]
use ::image::{io::Reader, ImageFormat};
use ::image::{DynamicImage, ImageDecoder};
#[cfg(any(not(windows), test))]
use std::io::Cursor;

/// Optional bounds for one clipboard image read.
///
/// All fields default to `None`, which preserves the existing decoder defaults.
/// Bounds are inclusive: a value equal to a bound is allowed.
///
/// These limits do not cap total process memory. Encoded data, decoded pixels,
/// conversion buffers, codec metadata, and operating system buffers can coexist.
/// Codec setup can allocate memory before image dimensions become available.
///
/// On X11, each property reply is bounded before transfer, with up to three
/// padding bytes. Incremental transfers check the cumulative size before append.
/// Image negotiation has a separate 64 KiB bound for the TARGETS response.
/// An oversized TARGETS response also returns [`Error::ImageTooLarge`].
/// Wayland reads at most one byte past the encoded limit to detect excess data.
/// On Windows, the encoded bound includes the native allocation's padding and
/// DIB header. The bound is checked before arboard copies that allocation.
/// On macOS, the bound applies to the TIFF representation returned by AppKit.
/// AppKit can create that representation before arboard can check its size.
/// Neither native backend can bound allocations made by the operating system.
///
/// ```no_run
/// use arboard::{Clipboard, ImageLimits};
///
/// # fn main() -> Result<(), arboard::Error> {
/// let mut limits = ImageLimits::default();
/// limits.max_encoded_bytes = Some(1024 * 1024);
/// limits.max_width = Some(4096);
/// limits.max_height = Some(4096);
/// limits.max_decoded_bytes = Some(64 * 1024 * 1024);
/// let image = Clipboard::new()?.get().image_limits(limits).image()?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageLimits {
	/// Maximum encoded payload size, in bytes.
	pub max_encoded_bytes: Option<usize>,
	/// Maximum width, in pixels.
	pub max_width: Option<u32>,
	/// Maximum height, in pixels.
	pub max_height: Option<u32>,
	/// Maximum size of each decoded pixel buffer, in bytes.
	///
	/// Before pixel decode, arboard checks both the decoder's output size and
	/// `width * height * 4` for the returned RGBA8 pixels.
	/// The bound also applies to a GIF frame buffer before cropping to its canvas.
	/// This is not a sum of live allocations or a bound on codec scratch space.
	pub max_decoded_bytes: Option<u64>,
}

impl ImageLimits {
	pub(crate) fn decoder_limits(self, mut defaults: ::image::io::Limits) -> ::image::io::Limits {
		defaults.max_image_width = self.max_width;
		defaults.max_image_height = self.max_height;
		defaults
	}

	pub(crate) fn check_encoded(self, size: usize) -> Result<(), Error> {
		if self.max_encoded_bytes.is_some_and(|limit| size > limit) {
			return Err(Error::ImageTooLarge);
		}
		Ok(())
	}

	fn check_dimensions(self, width: u32, height: u32, decoded: u64) -> Result<(), Error> {
		if self.max_width.is_some_and(|limit| width > limit)
			|| self.max_height.is_some_and(|limit| height > limit)
		{
			return Err(Error::ImageTooLarge);
		}
		let rgba = u64::from(width)
			.checked_mul(u64::from(height))
			.and_then(|size| size.checked_mul(4))
			.ok_or(Error::ImageTooLarge)?;
		if self.max_decoded_bytes.is_some_and(|limit| decoded > limit || rgba > limit) {
			return Err(Error::ImageTooLarge);
		}
		Ok(())
	}
}

pub(crate) fn decode_error(error: ::image::ImageError) -> Error {
	match error {
		::image::ImageError::Limits(error)
			if matches!(error.kind(), ::image::error::LimitErrorKind::DimensionError) =>
		{
			Error::ImageTooLarge
		}
		_ => Error::ConversionFailure,
	}
}

#[cfg(any(not(windows), test))]
pub(crate) fn decode(
	data: &[u8],
	format: ImageFormat,
	limits: ImageLimits,
) -> Result<ImageData<'static>, Error> {
	limits.check_encoded(data.len())?;
	let mut reader = Reader::with_format(Cursor::new(data), format);
	reader.limits(limits.decoder_limits(::image::io::Limits::default()));
	let mut decoder = reader.into_decoder().map_err(decode_error)?;
	let (width, height) = decoder.dimensions();
	limits.check_dimensions(width, height, decoder.total_bytes())?;
	// Preserve ImageReader::decode's existing allocation budget.
	let mut decoder_limits = ::image::io::Limits::default();
	decoder_limits.reserve(decoder.total_bytes()).map_err(|_| Error::ConversionFailure)?;
	// GIF frames can exceed the logical canvas. Bound the extra frame allocation
	// inside the decoder before it crops the frame into the output buffer.
	let frame_allocation_limit = if format == ImageFormat::Gif {
		limits
			.max_decoded_bytes
			.filter(|&limit| decoder_limits.max_alloc.map_or(true, |default| limit <= default))
	} else {
		None
	};
	if let Some(limit) = frame_allocation_limit {
		decoder_limits.max_alloc = Some(limit);
	}
	decoder.set_limits(decoder_limits).map_err(|_| Error::ConversionFailure)?;
	decode_pixels_with_error(decoder, limits, |error| match error {
		::image::ImageError::Limits(error)
			if frame_allocation_limit.is_some()
				&& matches!(error.kind(), ::image::error::LimitErrorKind::InsufficientMemory) =>
		{
			Error::ImageTooLarge
		}
		_ => Error::ConversionFailure,
	})
}

#[cfg(windows)]
pub(crate) fn decode_pixels(
	decoder: impl ImageDecoder,
	limits: ImageLimits,
) -> Result<ImageData<'static>, Error> {
	decode_pixels_with_error(decoder, limits, |_| Error::ConversionFailure)
}

fn decode_pixels_with_error(
	decoder: impl ImageDecoder,
	limits: ImageLimits,
	map_error: impl FnOnce(::image::ImageError) -> Error,
) -> Result<ImageData<'static>, Error> {
	let (width, height) = decoder.dimensions();
	limits.check_dimensions(width, height, decoder.total_bytes())?;
	let rgba = DynamicImage::from_decoder(decoder).map_err(map_error)?.into_rgba8();
	Ok(ImageData { width: width as usize, height: height as usize, bytes: rgba.into_raw().into() })
}

#[cfg(test)]
mod tests {
	use super::*;

	#[cfg(target_os = "macos")]
	const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/red.tiff");
	#[cfg(target_os = "macos")]
	const FORMAT: ImageFormat = ImageFormat::Tiff;
	#[cfg(not(target_os = "macos"))]
	const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/red.png");
	#[cfg(not(target_os = "macos"))]
	const FORMAT: ImageFormat = ImageFormat::Png;

	#[test]
	fn inclusive_limits() {
		let mut limits = ImageLimits::default();
		assert_eq!(limits.max_encoded_bytes, None);
		assert_eq!(limits.max_width, None);
		assert_eq!(limits.max_height, None);
		assert_eq!(limits.max_decoded_bytes, None);
		limits.max_encoded_bytes = Some(FIXTURE.len());
		limits.max_width = Some(3);
		limits.max_height = Some(2);
		limits.max_decoded_bytes = Some(24);
		let before = limits;
		let image = decode(FIXTURE, FORMAT, limits).unwrap();
		assert_eq!(limits, before);
		assert_eq!((image.width, image.height, image.bytes.len()), (3, 2, 24));
		assert!(image.bytes.chunks_exact(4).all(|pixel| pixel == [255, 0, 0, 255]));
		for smaller in [
			ImageLimits { max_encoded_bytes: Some(FIXTURE.len() - 1), ..limits },
			ImageLimits { max_width: Some(2), ..limits },
			ImageLimits { max_height: Some(1), ..limits },
			ImageLimits { max_decoded_bytes: Some(23), ..limits },
			ImageLimits { max_encoded_bytes: Some(0), ..limits },
			ImageLimits { max_width: Some(0), ..limits },
			ImageLimits { max_height: Some(0), ..limits },
			ImageLimits { max_decoded_bytes: Some(0), ..limits },
		] {
			assert!(matches!(decode(FIXTURE, FORMAT, smaller), Err(Error::ImageTooLarge)));
		}
	}

	#[test]
	fn checked_pixel_sizes() {
		let limits = ImageLimits { max_decoded_bytes: Some(24), ..ImageLimits::default() };
		assert!(limits.check_dimensions(3, 2, 18).is_ok());
		assert!(matches!(limits.check_dimensions(3, 2, 36), Err(Error::ImageTooLarge)));
		assert!(matches!(
			ImageLimits::default().check_dimensions(u32::MAX, u32::MAX, 0),
			Err(Error::ImageTooLarge)
		));
	}

	#[test]
	fn malformed_data_is_not_a_limit_error() {
		assert!(matches!(
			decode(b"not an image", FORMAT, ImageLimits::default()),
			Err(Error::ConversionFailure)
		));
		let limits = ImageLimits { max_encoded_bytes: Some(2), ..ImageLimits::default() };
		assert!(matches!(decode(b"not an image", FORMAT, limits), Err(Error::ImageTooLarge)));
	}

	#[cfg(not(target_os = "macos"))]
	#[test]
	fn sixteen_bit_decoder_buffer_has_its_own_bound() {
		let data = include_bytes!("../tests/fixtures/red16.png");
		let limits = ImageLimits { max_decoded_bytes: Some(36), ..ImageLimits::default() };
		assert_eq!(decode(data, ImageFormat::Png, limits).unwrap().bytes.len(), 24);
		assert!(matches!(
			decode(data, ImageFormat::Png, ImageLimits { max_decoded_bytes: Some(35), ..limits }),
			Err(Error::ImageTooLarge)
		));
	}

	#[cfg(not(target_os = "macos"))]
	#[test]
	fn png_dimensions_are_checked_before_pixel_data() {
		let header = &FIXTURE[..33];
		assert!(matches!(
			decode(header, FORMAT, ImageLimits { max_width: Some(2), ..ImageLimits::default() }),
			Err(Error::ImageTooLarge)
		));
		assert!(matches!(
			decode(header, FORMAT, ImageLimits::default()),
			Err(Error::ConversionFailure)
		));
	}
}
