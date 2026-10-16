use arboard::{Clipboard, ClipboardContent, ClipboardFormat, Error};

#[cfg(all(unix, not(target_os = "macos")))]
#[path = "support/x11_formats.rs"]
mod x11_formats;

#[test]
fn native_format_operations() {
	let mut clipboard = Clipboard::new().expect("A native clipboard is required for this test");
	#[cfg(all(unix, not(target_os = "macos")))]
	if let Ok(overridden) = std::env::var("ARBOARD_X11_TEXT_ALIAS_OVERRIDE") {
		x11_formats::read_text_aliases(&mut clipboard, &overridden);
		return;
	}
	#[cfg(all(unix, not(target_os = "macos")))]
	if std::env::var_os("ARBOARD_FORMAT_WAIT_READER").is_some() {
		use arboard::{GetExtLinux, LinuxClipboardKind};
		let format = ClipboardFormat::register("org.arboard.wait-once").unwrap();
		assert_eq!(
			clipboard.get().clipboard(LinuxClipboardKind::Primary).raw(&format).unwrap(),
			[9]
		);
		return;
	}
	let custom = ClipboardFormat::register("org.arboard.test").unwrap();
	let absent = ClipboardFormat::register("org.arboard.absent").unwrap();
	assert_eq!(custom, ClipboardFormat::register("org.arboard.test").unwrap());
	assert_eq!(custom.name().unwrap().as_deref(), Some("org.arboard.test"));
	let rtf = b"{\\rtf1\\ansi \xe9}";
	if std::env::var_os("ARBOARD_FORMAT_READER").is_some() {
		assert!(clipboard.get().formats().unwrap().contains(&custom));
		assert!(clipboard.get().raw(&custom).unwrap().starts_with(&[0, 255, 0, 42]));
		assert_eq!(clipboard.get().text().unwrap(), "hello \u{1f980}");
		assert!(clipboard.get().html().unwrap().contains("<b>hello</b>"));
		assert!(clipboard.get().rtf().unwrap().starts_with(rtf));
		return;
	}
	clipboard
		.set()
		.formats([
			ClipboardContent::Text("hello \u{1f980}".into()),
			ClipboardContent::Html("<b>hello</b>".into()),
			ClipboardContent::Rtf(rtf.as_slice().into()),
			ClipboardContent::Raw(custom.clone(), [0, 255, 0, 42].as_slice().into()),
		])
		.unwrap();
	assert!(clipboard.get().formats().unwrap().contains(&custom));
	assert!(clipboard.get().contains(&custom).unwrap());
	assert!(!clipboard.get().contains(&absent).unwrap());
	assert!(matches!(clipboard.get().raw(&absent), Err(Error::ContentNotAvailable)));
	assert!(clipboard.get().raw(&custom).unwrap().starts_with(&[0, 255, 0, 42]));
	assert_eq!(clipboard.get().text().unwrap(), "hello \u{1f980}");
	assert!(clipboard.get().html().unwrap().contains("<b>hello</b>"));
	assert!(clipboard.get().rtf().unwrap().starts_with(rtf));
	#[cfg(target_os = "macos")]
	for format in clipboard.get().formats().unwrap() {
		assert!(
			clipboard.get().raw(&format).is_ok(),
			"Discovered format must be readable: {format:?}"
		);
	}
	let status = std::process::Command::new(std::env::current_exe().unwrap())
		.args(["--exact", "native_format_operations", "--nocapture"])
		.env("ARBOARD_FORMAT_READER", "1")
		.status()
		.unwrap();
	assert!(status.success(), "A separate process must discover and read each representation");

	assert!(matches!(
		clipboard.set().formats([
			ClipboardContent::Raw(custom.clone(), [1].as_slice().into()),
			ClipboardContent::Raw(custom.clone(), [2].as_slice().into()),
		]),
		Err(Error::DuplicateFormat)
	));
	assert!(clipboard.get().raw(&custom).unwrap().starts_with(&[0, 255, 0, 42]));
	assert!(matches!(clipboard.set().formats([]), Err(Error::InvalidFormat)));
	assert!(clipboard.get().contains(&custom).unwrap());
	assert!(matches!(
		clipboard
			.set()
			.formats([ClipboardContent::Text("one".into()), ClipboardContent::Text("two".into()),]),
		Err(Error::DuplicateFormat)
	));
	assert!(clipboard.get().contains(&custom).unwrap());

	#[cfg(windows)]
	let rtf_name = "Rich Text Format";
	#[cfg(target_os = "macos")]
	let rtf_name = "public.rtf";
	#[cfg(all(unix, not(target_os = "macos")))]
	let rtf_name = "text/rtf";
	assert!(matches!(
		clipboard.set().formats([
			ClipboardContent::Rtf(rtf.as_slice().into()),
			ClipboardContent::Raw(
				ClipboardFormat::register(rtf_name).unwrap(),
				rtf.as_slice().into()
			),
		]),
		Err(Error::DuplicateFormat)
	));
	assert!(clipboard.get().contains(&custom).unwrap());

	clipboard.set().rtf(rtf.as_slice()).unwrap();
	assert!(clipboard.get().rtf().unwrap().starts_with(rtf));
	assert!(!clipboard.get().contains(&custom).unwrap());
	clipboard.set().raw(custom.clone(), [].as_slice()).unwrap();
	let empty = clipboard.get().raw(&custom).unwrap();
	assert!(empty.iter().all(|byte| *byte == 0));
	#[cfg(not(windows))]
	assert!(empty.is_empty());

	privacy(&mut clipboard, &custom);
	#[cfg(all(unix, not(target_os = "macos")))]
	linux(&mut clipboard, &custom);
	#[cfg(windows)]
	windows(&mut clipboard, &custom);
	#[cfg(target_os = "macos")]
	macos(&mut clipboard, &custom);
	mixed_files(&mut clipboard);
	#[cfg(target_os = "macos")]
	macos_legacy_files(&mut clipboard);
	#[cfg(feature = "image-data")]
	mixed_images(&mut clipboard);
	clipboard.clear().unwrap();
	assert!(clipboard.get().formats().unwrap().is_empty());
}

#[cfg(target_os = "macos")]
fn macos_legacy_files(clipboard: &mut Clipboard) {
	let root = std::env::current_dir().unwrap();
	let files = vec![root.join("Cargo.toml"), root.join("README.md")];
	let legacy = ClipboardFormat::register("NSStringPboardType").unwrap();
	let canonical = ClipboardFormat::register("public.utf8-plain-text").unwrap();
	let custom = ClipboardFormat::register("org.arboard.legacy-files").unwrap();
	for count in 0..=2 {
		let mut contents = vec![ClipboardContent::Raw(legacy.clone(), b"hello".as_slice().into())];
		if count != 0 {
			contents.push(ClipboardContent::Files(files[..count].into()));
		}
		clipboard.set().formats(contents).unwrap();
		assert_eq!(clipboard.get().raw(&legacy).unwrap(), b"hello");
		assert_eq!(clipboard.get().text().unwrap(), "hello");
		if count != 0 {
			assert_eq!(clipboard.get().file_list().unwrap().len(), count);
		}
	}
	for (first, last) in [(&legacy, &canonical), (&canonical, &legacy)] {
		private_set(clipboard)
			.formats([
				ClipboardContent::Raw(first.clone(), b"first".as_slice().into()),
				ClipboardContent::Files(files.as_slice().into()),
				ClipboardContent::Raw(last.clone(), b"last".as_slice().into()),
				ClipboardContent::Raw(custom.clone(), b"custom".as_slice().into()),
			])
			.unwrap();
		assert_eq!(clipboard.get().raw(&legacy).unwrap(), b"last");
		assert_eq!(clipboard.get().raw(&canonical).unwrap(), b"last");
		assert_eq!(clipboard.get().text().unwrap(), "last");
		assert_eq!(clipboard.get().file_list().unwrap().len(), 2);
		assert_eq!(clipboard.get().raw(&custom).unwrap(), b"custom");
		assert_private(clipboard);
	}
	let pasteboard = unsafe { objc2_app_kit::NSPasteboard::generalPasteboard() };
	let change_count = unsafe { pasteboard.changeCount() };
	assert!(matches!(
		clipboard.set().formats([
			ClipboardContent::Text("must not replace contents".into()),
			ClipboardContent::Files(files.as_slice().into()),
			ClipboardContent::Raw(
				ClipboardFormat::register("not a valid pasteboard type").unwrap(),
				b"unsupported".as_slice().into()
			),
		]),
		Err(Error::UnsupportedFormat)
	));
	assert_eq!(unsafe { pasteboard.changeCount() }, change_count);
	assert_eq!(clipboard.get().text().unwrap(), "last");
	assert_eq!(clipboard.get().file_list().unwrap().len(), 2);
	assert_eq!(clipboard.get().raw(&custom).unwrap(), b"custom");
	assert_private(clipboard);
}

fn private_set(clipboard: &mut Clipboard) -> arboard::Set<'_> {
	#[cfg(target_os = "macos")]
	use arboard::SetExtApple;
	#[cfg(all(unix, not(target_os = "macos")))]
	use arboard::SetExtLinux;
	#[cfg(windows)]
	use arboard::SetExtWindows;
	let set = clipboard.set().exclude_from_history();
	#[cfg(windows)]
	let set = set.exclude_from_cloud().exclude_from_monitoring();
	set
}

fn assert_private(clipboard: &mut Clipboard) {
	#[cfg(target_os = "macos")]
	let names = ["org.nspasteboard.ConcealedType"];
	#[cfg(all(unix, not(target_os = "macos")))]
	let names = ["x-kde-passwordManagerHint"];
	#[cfg(windows)]
	let names = [
		"CanIncludeInClipboardHistory",
		"CanUploadToCloudClipboard",
		"ExcludeClipboardContentFromMonitorProcessing",
	];
	for name in names {
		assert!(clipboard.get().contains(&ClipboardFormat::register(name).unwrap()).unwrap());
	}
}

fn mixed_files(clipboard: &mut Clipboard) {
	let root = std::env::current_dir().unwrap();
	let files = vec![root.join("Cargo.toml"), root.join("README.md")];
	for files in [&files[..1], files.as_slice()] {
		private_set(clipboard)
			.formats([
				ClipboardContent::Text("files alternative".into()),
				ClipboardContent::Files(files.into()),
			])
			.unwrap();
		assert_eq!(clipboard.get().text().unwrap(), "files alternative");
		assert_eq!(
			clipboard
				.get()
				.file_list()
				.unwrap()
				.iter()
				.map(|path| path.canonicalize().unwrap())
				.collect::<Vec<_>>(),
			files.iter().map(|path| path.canonicalize().unwrap()).collect::<Vec<_>>()
		);
		assert_private(clipboard);
	}
	#[cfg(target_os = "macos")]
	{
		let pasteboard = unsafe { objc2_app_kit::NSPasteboard::generalPasteboard() };
		let items = unsafe { pasteboard.pasteboardItems() }.unwrap();
		assert_eq!(items.len(), 2);
		let marker = objc2_foundation::NSString::from_str("org.nspasteboard.ConcealedType");
		for item in items {
			assert!(unsafe { item.dataForType(&marker) }.is_some());
		}
	}

	#[cfg(target_os = "macos")]
	let formats = vec![ClipboardFormat::register("public.file-url").unwrap()];
	#[cfg(all(unix, not(target_os = "macos")))]
	let formats = ["text/uri-list", "x-special/gnome-copied-files", "x-special/nautilus-clipboard"]
		.map(|name| ClipboardFormat::register(name).unwrap())
		.to_vec();
	#[cfg(windows)]
	let formats = vec![ClipboardFormat::from_windows_id(15).unwrap()];
	for format in formats {
		assert!(clipboard.get().contains(&format).unwrap());
		assert!(matches!(
			clipboard.set().formats([
				ClipboardContent::Files(files.as_slice().into()),
				ClipboardContent::Raw(format, [].as_slice().into()),
			]),
			Err(Error::DuplicateFormat)
		));
		assert_eq!(clipboard.get().text().unwrap(), "files alternative");
	}
	for invalid in [vec![], vec![files[0].clone(), files[0].join("not-a-directory")]] {
		assert!(matches!(
			clipboard.set().formats([
				ClipboardContent::Text("must not replace contents".into()),
				ClipboardContent::Files(invalid.into()),
			]),
			Err(Error::ConversionFailure)
		));
		assert_eq!(clipboard.get().text().unwrap(), "files alternative");
	}
}

#[cfg(feature = "image-data")]
fn mixed_images(clipboard: &mut Clipboard) {
	let image = arboard::ImageData {
		width: 2,
		height: 2,
		bytes: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 127, 63, 31, 255].into(),
	};
	private_set(clipboard)
		.formats([
			ClipboardContent::Text("image alternative".into()),
			ClipboardContent::Image(image.clone()),
		])
		.unwrap();
	assert_eq!(clipboard.get().text().unwrap(), "image alternative");
	let actual = clipboard.get().image().unwrap();
	assert_eq!((actual.width, actual.height), (image.width, image.height));
	assert_eq!(actual.bytes, image.bytes);
	assert_private(clipboard);
	#[cfg(target_os = "macos")]
	let formats = vec![ClipboardFormat::register("public.tiff").unwrap()];
	#[cfg(all(unix, not(target_os = "macos")))]
	let formats = vec![ClipboardFormat::register("image/png").unwrap()];
	#[cfg(windows)]
	let formats = vec![
		ClipboardFormat::register("PNG").unwrap(),
		ClipboardFormat::from_windows_id(17).unwrap(),
	];
	for format in formats {
		assert!(clipboard.get().contains(&format).unwrap());
		assert!(matches!(
			clipboard.set().formats([
				ClipboardContent::Image(image.clone()),
				ClipboardContent::Raw(format, [].as_slice().into()),
			]),
			Err(Error::DuplicateFormat)
		));
		assert_eq!(clipboard.get().text().unwrap(), "image alternative");
	}
	for invalid in [
		arboard::ImageData { width: 2, height: 1, bytes: vec![0; 3].into() },
		arboard::ImageData { width: 0, height: 1, bytes: vec![].into() },
		arboard::ImageData { width: usize::MAX, height: 2, bytes: vec![0; 4].into() },
	] {
		assert!(matches!(
			clipboard.set().formats([
				ClipboardContent::Text("must not replace contents".into()),
				ClipboardContent::Image(invalid),
			]),
			Err(Error::ConversionFailure)
		));
		assert_eq!(clipboard.get().text().unwrap(), "image alternative");
	}
	let root = std::env::current_dir().unwrap();
	private_set(clipboard)
		.formats([
			ClipboardContent::Text("image and files".into()),
			ClipboardContent::Image(image.clone()),
			ClipboardContent::Files(vec![root.join("Cargo.toml"), root.join("README.md")].into()),
		])
		.unwrap();
	assert_eq!(clipboard.get().text().unwrap(), "image and files");
	assert_eq!(clipboard.get().image().unwrap().bytes, image.bytes);
	assert_eq!(clipboard.get().file_list().unwrap().len(), 2);
	assert_private(clipboard);
}

#[cfg(target_os = "macos")]
fn macos(clipboard: &mut Clipboard, custom: &ClipboardFormat) {
	use objc2::runtime::ProtocolObject;
	use objc2_app_kit::{NSPasteboard, NSPasteboardItem};
	use objc2_foundation::{NSArray, NSData, NSString};

	let name = NSString::from_str(&custom.name().unwrap().unwrap());
	let text = NSString::from_str("public.utf8-plain-text");
	let first = unsafe { NSPasteboardItem::new() };
	let second = unsafe { NSPasteboardItem::new() };
	for (item, value) in [(&first, b"first".as_slice()), (&second, b"second".as_slice())] {
		let data = NSData::with_bytes(value);
		assert!(unsafe { item.setData_forType(&data, &name) });
		assert!(unsafe { item.setData_forType(&data, &text) });
	}
	let items = NSArray::from_retained_slice(&[
		ProtocolObject::from_retained(first),
		ProtocolObject::from_retained(second),
	]);
	let pasteboard = unsafe { NSPasteboard::generalPasteboard() };
	unsafe { pasteboard.clearContents() };
	assert!(unsafe { pasteboard.writeObjects(&items) });
	assert_eq!(clipboard.get().raw(custom).unwrap(), b"first");
	assert_eq!(
		clipboard.get().raw(&ClipboardFormat::register("public.utf8-plain-text").unwrap()).unwrap(),
		b"first\nsecond"
	);
	assert_eq!(clipboard.get().text().unwrap(), "first");
}

fn privacy(clipboard: &mut Clipboard, custom: &ClipboardFormat) {
	#[cfg(target_os = "macos")]
	use arboard::SetExtApple;
	#[cfg(all(unix, not(target_os = "macos")))]
	use arboard::SetExtLinux;
	#[cfg(windows)]
	use arboard::SetExtWindows;

	let set = clipboard.set().exclude_from_history();
	#[cfg(windows)]
	let set = set.exclude_from_monitoring().exclude_from_cloud();
	set.formats([ClipboardContent::Raw(custom.clone(), [42].as_slice().into())]).unwrap();
	#[cfg(target_os = "macos")]
	let markers = ["org.nspasteboard.ConcealedType"];
	#[cfg(all(unix, not(target_os = "macos")))]
	let markers = ["x-kde-passwordManagerHint"];
	#[cfg(windows)]
	let markers = [
		"CanIncludeInClipboardHistory",
		"CanUploadToCloudClipboard",
		"ExcludeClipboardContentFromMonitorProcessing",
	];
	for marker in markers {
		let marker = ClipboardFormat::register(marker).unwrap();
		assert!(clipboard.get().contains(&marker).unwrap());
		let bytes = clipboard.get().raw(&marker).unwrap();
		#[cfg(windows)]
		assert!(bytes.starts_with(&[0, 0, 0, 0]));
		#[cfg(target_os = "macos")]
		assert!(bytes.is_empty());
		#[cfg(all(unix, not(target_os = "macos")))]
		assert_eq!(bytes, b"secret");
	}

	let marker = ClipboardFormat::register(markers[0]).unwrap();
	assert!(matches!(
		clipboard.set().exclude_from_history().raw(marker, [1].as_slice()),
		Err(Error::DuplicateFormat)
	));
	assert!(clipboard.get().raw(custom).unwrap().starts_with(&[42]));
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux(clipboard: &mut Clipboard, custom: &ClipboardFormat) {
	use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};
	let targets = ClipboardFormat::register("TARGETS").unwrap();
	let is_x11 = clipboard.get().contains(&targets).unwrap();
	if is_x11 {
		x11_formats::actual_type_is_preserved(clipboard);
		use x11rb::connection::RequestConnection;
		assert!(matches!(clipboard.get().raw(&targets), Err(Error::UnsupportedFormat)));
		assert!(matches!(
			clipboard.set().raw(targets, [0].as_slice()),
			Err(Error::UnsupportedFormat)
		));
		let (connection, _) = x11rb::connect(None).unwrap();
		let too_large = vec![0; connection.maximum_request_bytes()];
		assert!(matches!(
			clipboard.set().raw(custom.clone(), too_large),
			Err(Error::ConversionFailure)
		));
		assert_eq!(clipboard.get().raw(custom).unwrap(), [42]);
	}
	clipboard
		.set()
		.clipboard(LinuxClipboardKind::Primary)
		.raw(custom.clone(), [7].as_slice())
		.unwrap();
	assert!(clipboard.get().clipboard(LinuxClipboardKind::Primary).contains(custom).unwrap());
	assert_eq!(clipboard.get().clipboard(LinuxClipboardKind::Primary).raw(custom).unwrap(), [7]);
	assert_eq!(clipboard.get().raw(custom).unwrap(), [42]);

	let file = std::env::current_dir().unwrap().join("Cargo.toml");
	clipboard.set().file_list(&[&file]).unwrap();
	assert_eq!(clipboard.get().file_list().unwrap(), vec![file]);
	for name in ["text/uri-list", "x-special/gnome-copied-files", "x-special/nautilus-clipboard"] {
		let format = ClipboardFormat::register(name).unwrap();
		let bytes = clipboard.get().raw(&format).unwrap();
		clipboard.set().raw(format, bytes).unwrap();
		assert_eq!(
			clipboard.get().file_list().unwrap(),
			vec![std::env::current_dir().unwrap().join("Cargo.toml")]
		);
		clipboard.set().file_list(&[std::env::current_dir().unwrap().join("Cargo.toml")]).unwrap();
	}
	{
		use std::os::unix::ffi::OsStringExt;
		let directory = std::env::current_dir()
			.unwrap()
			.join("target")
			.join(format!("format-files-{}", std::process::id()));
		std::fs::create_dir_all(&directory).unwrap();
		let mut paths = vec![directory.join("space #?%.txt")];
		for path in &paths {
			std::fs::write(path, []).unwrap();
		}
		let result = (|| -> Result<_, Error> {
			clipboard.set().file_list(&paths)?;
			let uri = ClipboardFormat::register("text/uri-list")?;
			let bytes = clipboard.get().raw(&uri)?;
			let mut bytes =
				String::from_utf8(bytes).unwrap().replace("file:///", "file://localhost/");
			bytes.push_str("\r\nfile://localhost/non-utf8-%FF");
			clipboard.set().raw(uri, bytes.into_bytes())?;
			clipboard.get().file_list()
		})();
		std::fs::remove_dir_all(&directory).unwrap();
		paths.push(std::path::PathBuf::from(std::ffi::OsString::from_vec(
			b"/non-utf8-\xff".to_vec(),
		)));
		assert_eq!(result.unwrap(), paths);
	}
	let html = ClipboardFormat::register("text/html").unwrap();
	clipboard.set().raw(html, b"<b>raw HTML</b>".as_slice()).unwrap();
	assert!(!clipboard.get().contains(&ClipboardFormat::register("text/plain").unwrap()).unwrap());

	let waiting = ClipboardFormat::register("org.arboard.waiting").unwrap();
	let writer_format = waiting.clone();
	let (finished, result) = std::sync::mpsc::channel();
	let writer = std::thread::spawn(move || {
		Clipboard::new()
			.unwrap()
			.set()
			.clipboard(LinuxClipboardKind::Primary)
			.exclude_from_history()
			.wait()
			.raw(writer_format, [9].as_slice())
			.unwrap();
		finished.send(()).unwrap();
	});
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
	while !clipboard.get().clipboard(LinuxClipboardKind::Primary).contains(&waiting).unwrap() {
		assert!(std::time::Instant::now() < deadline, "The writer must publish before it waits");
		std::thread::sleep(std::time::Duration::from_millis(5));
	}
	assert!(matches!(result.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
	let marker = ClipboardFormat::register("x-kde-passwordManagerHint").unwrap();
	assert_eq!(
		clipboard.get().clipboard(LinuxClipboardKind::Primary).raw(&marker).unwrap(),
		b"secret"
	);
	clipboard.set().clipboard(LinuxClipboardKind::Primary).text("replacement").unwrap();
	result.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
	writer.join().unwrap();

	let start = std::time::Instant::now();
	let deadline = start + std::time::Duration::from_millis(150);
	clipboard
		.set()
		.clipboard(LinuxClipboardKind::Primary)
		.wait_until(deadline)
		.raw(waiting, [8].as_slice())
		.unwrap();
	assert!(std::time::Instant::now() >= deadline, "A deadline write must wait without readers");
	clipboard.set().clipboard(LinuxClipboardKind::Primary).text("replacement").unwrap();

	let once = ClipboardFormat::register("org.arboard.wait-once").unwrap();
	let writer_format = once.clone();
	let (finished, result) = std::sync::mpsc::channel();
	let writer = std::thread::spawn(move || {
		Clipboard::new()
			.unwrap()
			.set()
			.clipboard(LinuxClipboardKind::Primary)
			.wait_once()
			.raw(writer_format, [9].as_slice())
			.unwrap();
		finished.send(()).unwrap();
	});
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
	while !clipboard.get().clipboard(LinuxClipboardKind::Primary).contains(&once).unwrap() {
		assert!(std::time::Instant::now() < deadline);
		std::thread::sleep(std::time::Duration::from_millis(5));
	}
	assert!(
		matches!(
			result.recv_timeout(std::time::Duration::from_millis(50)),
			Err(std::sync::mpsc::RecvTimeoutError::Timeout)
		),
		"A single-request write must wait for a reader"
	);
	let status = std::process::Command::new(std::env::current_exe().unwrap())
		.args(["--exact", "native_format_operations", "--nocapture"])
		.env("ARBOARD_FORMAT_WAIT_READER", "1")
		.status()
		.unwrap();
	assert!(status.success());
	result.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
	writer.join().unwrap();
	if is_x11 {
		x11_formats::advertised_text_aliases(clipboard);
	}
}

#[cfg(windows)]
fn windows(clipboard: &mut Clipboard, custom: &ClipboardFormat) {
	let prefix = "org.arboard.boundary.";
	for units in [51, 52, 53] {
		for name in [
			format!("{prefix}{}", "a".repeat(units - prefix.len())),
			format!("{prefix}{}😀", "b".repeat(units - prefix.len() - 2)),
		] {
			assert_eq!(name.encode_utf16().count(), units);
			let format = ClipboardFormat::register(&name).unwrap();
			assert_eq!(format.name().unwrap().as_deref(), Some(name.as_str()));
			assert_eq!(ClipboardFormat::register(&name).unwrap(), format);
		}
	}
	let text = ClipboardFormat::from_windows_id(13).unwrap();
	assert_eq!(text.windows_id(), 13);
	assert_eq!(text.name().unwrap(), None);
	assert_eq!(custom, &ClipboardFormat::register("ORG.ARBOARD.TEST").unwrap());
	for id in [2, 3, 9, 14, 0x80, 0x200, 0x300] {
		let handle = ClipboardFormat::from_windows_id(id).unwrap();
		assert!(matches!(clipboard.get().raw(&handle), Err(Error::UnsupportedFormat)));
		assert!(matches!(
			clipboard.set().raw(handle, [0].as_slice()),
			Err(Error::UnsupportedFormat)
		));
		assert!(clipboard.get().contains(custom).unwrap());
	}
	assert!(matches!(ClipboardFormat::from_windows_id(0), Err(Error::InvalidFormat)));
	assert!(matches!(ClipboardFormat::from_windows_id(0x10000), Err(Error::InvalidFormat)));
	assert!(matches!(
		clipboard.set().formats([
			ClipboardContent::Text("text".into()),
			ClipboardContent::Raw(text, [0, 0].as_slice().into()),
		]),
		Err(Error::DuplicateFormat)
	));
}
