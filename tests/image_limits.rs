#![cfg(feature = "image-data")]

use arboard::{Clipboard, Error, ImageData, ImageLimits};

#[test]
fn native_image_limits() {
	let mut clipboard = Clipboard::new().unwrap();
	let bytes = [255, 0, 0, 255].repeat(6);
	clipboard.set_image(ImageData { width: 3, height: 2, bytes: bytes.as_slice().into() }).unwrap();
	let mut limits = ImageLimits::default();
	limits.max_width = Some(3);
	limits.max_height = Some(2);
	limits.max_decoded_bytes = Some(24);
	assert_eq!(clipboard.get().image_limits(limits).image().unwrap().bytes.as_ref(), bytes);
	let original = limits;
	for (width, height, decoded) in [(2, 2, 24), (3, 1, 24), (3, 2, 23), (0, 0, 0)] {
		let mut smaller = limits;
		smaller.max_width = Some(width);
		smaller.max_height = Some(height);
		smaller.max_decoded_bytes = Some(decoded);
		assert!(matches!(clipboard.get().image_limits(smaller).image(), Err(Error::ImageTooLarge)));
	}
	assert_eq!(limits, original);

	// Find the native representation's size without assuming a particular encoder or padding.
	let mut lower = 0;
	let mut upper = 1024 * 1024;
	limits.max_encoded_bytes = Some(upper);
	clipboard.get().image_limits(limits).image().unwrap();
	while lower < upper {
		let middle = lower + (upper - lower) / 2;
		limits.max_encoded_bytes = Some(middle);
		match clipboard.get().image_limits(limits).image() {
			Ok(_) => upper = middle,
			Err(Error::ImageTooLarge) => lower = middle + 1,
			Err(error) => panic!("unexpected clipboard error: {error}"),
		}
	}
	assert!(upper > 0);
	limits.max_encoded_bytes = Some(upper);
	assert_eq!(clipboard.get().image_limits(limits).image().unwrap().bytes.as_ref(), bytes);
	limits.max_encoded_bytes = Some(upper - 1);
	assert!(matches!(clipboard.get().image_limits(limits).image(), Err(Error::ImageTooLarge)));
	assert_eq!(clipboard.get_image().unwrap().bytes.as_ref(), bytes);
}
