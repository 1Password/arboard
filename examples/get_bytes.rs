use std::{fs, path::PathBuf};

use arboard::Clipboard;

const FILE_TYPES: &'static [(&[u8], &str)] = &[
	(b"image/png", "png"),
	(b"image/jpeg", "jpeg"),
	(b"image/bmp", "bmp"),
	(b"image/gif", "gif"),
	(b"text/html", "html"),
	(b"text/plain", "txt"),
	(b"text/uri-list", "txt"),
	(b"SAVE_TARGETS", "sav.txt"),
];

fn main() {
	let mut clipboard = Clipboard::new().unwrap();

	println!("Formats available are: {:#?}", clipboard.get_formats());

	let tmp_path = PathBuf::from("./examples/tmp/");
	if tmp_path.exists() {
		fs::remove_dir_all(&tmp_path).unwrap();
	}
	fs::create_dir_all(&tmp_path).unwrap();

	for (mime, ext) in FILE_TYPES.iter() {

		let path = tmp_path.join(&format!(
			"output-{}.{ext}",
			mime.into_iter()
				.map(|c| if (*c as char).is_alphanumeric() { *c as char } else { '-' })
				.collect::<String>()
		));

		if let Ok(data) = clipboard.get_bytes(mime) {
			println!("Saving {:?} as {}", mime, path.display());
			fs::write(path, data).unwrap();
		} else {
			println!(
				r#""{}" mime-type not available"#,
				String::from_utf8_lossy(mime) // mime.into_iter().map(|c| *c as char).collect::<String>()
			)
		}
	}
}
