#[cfg(target_os = "macos")]
pub mod osx_clipboard {
    use common::*;
    use core_graphics::color_space::{CGColorSpace, CGColorSpaceRef};
    use core_graphics::image::{CGImage, CGImageAlphaInfo};
    use core_graphics::{
        base::{kCGRenderingIntentDefault, CGFloat},
        data_provider::{CGDataProvider, CustomData},
    };
    use objc::runtime::{Class, Object};
    use objc_foundation::{INSArray, INSObject, INSString};
    use objc_foundation::{NSArray, NSDictionary, NSObject, NSString};
    use objc_id::{Id, Owned};
    use std::error::Error;
    use std::{mem::transmute, ops::Deref};
    pub struct OSXClipboardContext {
        pasteboard: Id<Object>,
    }
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}
    #[repr(C)]
    pub struct NSSize {
        pub width: CGFloat,
        pub height: CGFloat,
    }
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl ::core::marker::Copy for NSSize {}
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl ::core::clone::Clone for NSSize {
        #[inline]
        fn clone(&self) -> NSSize {
            {
                let _: ::core::clone::AssertParamIsClone<CGFloat>;
                let _: ::core::clone::AssertParamIsClone<CGFloat>;
                *self
            }
        }
    }
    struct PixelArray {
        data: Vec<u8>,
    }
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl ::core::fmt::Debug for PixelArray {
        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
            match *self {
                PixelArray {
                    data: ref __self_0_0,
                } => {
                    let mut debug_trait_builder = f.debug_struct("PixelArray");
                    let _ = debug_trait_builder.field("data", &&(*__self_0_0));
                    debug_trait_builder.finish()
                }
            }
        }
    }
    #[automatically_derived]
    #[allow(unused_qualifications)]
    impl ::core::clone::Clone for PixelArray {
        #[inline]
        fn clone(&self) -> PixelArray {
            match *self {
                PixelArray {
                    data: ref __self_0_0,
                } => PixelArray {
                    data: ::core::clone::Clone::clone(&(*__self_0_0)),
                },
            }
        }
    }
    impl CustomData for PixelArray {
        unsafe fn ptr(&self) -> *const u8 {
            self.data.as_ptr()
        }
        unsafe fn len(&self) -> usize {
            self.data.len()
        }
    }
    /// Returns an NSImage object on success.
    fn image_from_pixels(
        pixels: Vec<u8>,
        width: usize,
        height: usize,
    ) -> Result<Id<NSObject>, Box<dyn Error>> {
        let colorspace = CGColorSpace::create_device_rgb();
        let bitmap_info: u32 = CGImageAlphaInfo::CGImageAlphaLast as u32;
        let pixel_data: Box<Box<dyn CustomData>> = Box::new(Box::new(PixelArray { data: pixels }));
        let provider = unsafe { CGDataProvider::from_custom_data(pixel_data) };
        let rendering_intent = kCGRenderingIntentDefault;
        let cg_image = CGImage::new(
            width,
            height,
            8,
            32,
            4 * width,
            &colorspace,
            bitmap_info,
            &provider,
            false,
            rendering_intent,
        );
        let NSImage_class = Class::get("NSImage").ok_or(err("Class::get(\"NSImage\")"))?;
        let size = NSSize {
            width: width as CGFloat,
            height: height as CGFloat,
        };
        let image: Id<NSObject> = unsafe {
            Id::from_ptr({
                let sel = {
                    {
                        #[allow(deprecated)]
                        #[inline(always)]
                        fn register_sel(name: &str) -> ::objc::runtime::Sel {
                            unsafe {
                                static SEL: ::std::sync::atomic::AtomicUsize =
                                    ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                let ptr = SEL.load(::std::sync::atomic::Ordering::Relaxed)
                                    as *const ::std::os::raw::c_void;
                                if ptr.is_null() {
                                    let sel = ::objc::runtime::sel_registerName(
                                        name.as_ptr() as *const _
                                    );
                                    SEL.store(
                                        sel.as_ptr() as usize,
                                        ::std::sync::atomic::Ordering::Relaxed,
                                    );
                                    sel
                                } else {
                                    ::objc::runtime::Sel::from_ptr(ptr)
                                }
                            }
                        }
                        register_sel("alloc\u{0}")
                    }
                };
                let result;
                match ::objc::__send_message(&*NSImage_class, sel, ()) {
                    Err(s) => ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                        &[""],
                        &match (&s,) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    )),
                    Ok(r) => result = r,
                }
                result
            })
        };
        let ptr: *const std::ffi::c_void = unsafe { transmute(&*cg_image) };
        let a = &*cg_image;
        let image: Id<NSObject> = unsafe {
            Id::from_ptr({
                let sel = {
                    {
                        #[allow(deprecated)]
                        #[inline(always)]
                        fn register_sel(name: &str) -> ::objc::runtime::Sel {
                            unsafe {
                                static SEL: ::std::sync::atomic::AtomicUsize =
                                    ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                let ptr = SEL.load(::std::sync::atomic::Ordering::Relaxed)
                                    as *const ::std::os::raw::c_void;
                                if ptr.is_null() {
                                    let sel = ::objc::runtime::sel_registerName(
                                        name.as_ptr() as *const _
                                    );
                                    SEL.store(
                                        sel.as_ptr() as usize,
                                        ::std::sync::atomic::Ordering::Relaxed,
                                    );
                                    sel
                                } else {
                                    ::objc::runtime::Sel::from_ptr(ptr)
                                }
                            }
                        }
                        register_sel("initWithCGImage:size:\u{0}")
                    }
                };
                let result;
                match ::objc::__send_message(&*image, sel, (cg_image, &size)) {
                    Err(s) => ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                        &[""],
                        &match (&s,) {
                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                arg0,
                                ::core::fmt::Display::fmt,
                            )],
                        },
                    )),
                    Ok(r) => result = r,
                }
                result
            })
        };
        Ok(image)
    }
    impl ClipboardProvider for OSXClipboardContext {
        fn new() -> Result<OSXClipboardContext, Box<Error>> {
            let cls = match Class::get("NSPasteboard").ok_or(err("Class::get(\"NSPasteboard\")")) {
                ::core::result::Result::Ok(val) => val,
                ::core::result::Result::Err(err) => {
                    return ::core::result::Result::Err(::core::convert::From::from(err));
                }
            };
            let pasteboard: *mut Object = unsafe {
                {
                    let sel = {
                        {
                            #[allow(deprecated)]
                            #[inline(always)]
                            fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                unsafe {
                                    static SEL: ::std::sync::atomic::AtomicUsize =
                                        ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                    let ptr = SEL.load(::std::sync::atomic::Ordering::Relaxed)
                                        as *const ::std::os::raw::c_void;
                                    if ptr.is_null() {
                                        let sel = ::objc::runtime::sel_registerName(
                                            name.as_ptr() as *const _
                                        );
                                        SEL.store(
                                            sel.as_ptr() as usize,
                                            ::std::sync::atomic::Ordering::Relaxed,
                                        );
                                        sel
                                    } else {
                                        ::objc::runtime::Sel::from_ptr(ptr)
                                    }
                                }
                            }
                            register_sel("generalPasteboard\u{0}")
                        }
                    };
                    let result;
                    match ::objc::__send_message(&*cls, sel, ()) {
                        Err(s) => ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                            &[""],
                            &match (&s,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        )),
                        Ok(r) => result = r,
                    }
                    result
                }
            };
            if pasteboard.is_null() {
                return Err(err("NSPasteboard#generalPasteboard returned null"));
            }
            let pasteboard: Id<Object> = unsafe { Id::from_ptr(pasteboard) };
            Ok(OSXClipboardContext {
                pasteboard: pasteboard,
            })
        }
        fn get_text(&mut self) -> Result<String, Box<Error>> {
            let string_class: Id<NSObject> = {
                let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
                unsafe { transmute(cls) }
            };
            let classes: Id<NSArray<NSObject, Owned>> =
                NSArray::from_vec(<[_]>::into_vec(box [string_class]));
            let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
            let string_array: Id<NSArray<NSString>> = unsafe {
                let obj: *mut NSArray<NSString> = {
                    let sel = {
                        {
                            #[allow(deprecated)]
                            #[inline(always)]
                            fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                unsafe {
                                    static SEL: ::std::sync::atomic::AtomicUsize =
                                        ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                    let ptr = SEL.load(::std::sync::atomic::Ordering::Relaxed)
                                        as *const ::std::os::raw::c_void;
                                    if ptr.is_null() {
                                        let sel = ::objc::runtime::sel_registerName(
                                            name.as_ptr() as *const _
                                        );
                                        SEL.store(
                                            sel.as_ptr() as usize,
                                            ::std::sync::atomic::Ordering::Relaxed,
                                        );
                                        sel
                                    } else {
                                        ::objc::runtime::Sel::from_ptr(ptr)
                                    }
                                }
                            }
                            register_sel("readObjectsForClasses:options:\u{0}")
                        }
                    };
                    let result;
                    match ::objc::__send_message(&*self.pasteboard, sel, (&*classes, &*options)) {
                        Err(s) => ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                            &[""],
                            &match (&s,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        )),
                        Ok(r) => result = r,
                    }
                    result
                };
                if obj.is_null() {
                    return Err(err(
                        "pasteboard#readObjectsForClasses:options: returned null",
                    ));
                }
                Id::from_ptr(obj)
            };
            if string_array.count() == 0 {
                Err(err(
                    "pasteboard#readObjectsForClasses:options: returned empty",
                ))
            } else {
                Ok(string_array[0].as_str().to_owned())
            }
        }
        fn set_text(&mut self, data: String) -> Result<(), Box<Error>> {
            let string_array = NSArray::from_vec(<[_]>::into_vec(box [NSString::from_str(&data)]));
            let _: usize = unsafe {
                {
                    let sel = {
                        {
                            #[allow(deprecated)]
                            #[inline(always)]
                            fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                unsafe {
                                    static SEL: ::std::sync::atomic::AtomicUsize =
                                        ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                    let ptr = SEL.load(::std::sync::atomic::Ordering::Relaxed)
                                        as *const ::std::os::raw::c_void;
                                    if ptr.is_null() {
                                        let sel = ::objc::runtime::sel_registerName(
                                            name.as_ptr() as *const _
                                        );
                                        SEL.store(
                                            sel.as_ptr() as usize,
                                            ::std::sync::atomic::Ordering::Relaxed,
                                        );
                                        sel
                                    } else {
                                        ::objc::runtime::Sel::from_ptr(ptr)
                                    }
                                }
                            }
                            register_sel("clearContents\u{0}")
                        }
                    };
                    let result;
                    match ::objc::__send_message(&*self.pasteboard, sel, ()) {
                        Err(s) => ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                            &[""],
                            &match (&s,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        )),
                        Ok(r) => result = r,
                    }
                    result
                }
            };
            let success: bool = unsafe {
                {
                    let sel = {
                        {
                            #[allow(deprecated)]
                            #[inline(always)]
                            fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                unsafe {
                                    static SEL: ::std::sync::atomic::AtomicUsize =
                                        ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                    let ptr = SEL.load(::std::sync::atomic::Ordering::Relaxed)
                                        as *const ::std::os::raw::c_void;
                                    if ptr.is_null() {
                                        let sel = ::objc::runtime::sel_registerName(
                                            name.as_ptr() as *const _
                                        );
                                        SEL.store(
                                            sel.as_ptr() as usize,
                                            ::std::sync::atomic::Ordering::Relaxed,
                                        );
                                        sel
                                    } else {
                                        ::objc::runtime::Sel::from_ptr(ptr)
                                    }
                                }
                            }
                            register_sel("writeObjects:\u{0}")
                        }
                    };
                    let result;
                    match ::objc::__send_message(&*self.pasteboard, sel, (string_array,)) {
                        Err(s) => ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                            &[""],
                            &match (&s,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        )),
                        Ok(r) => result = r,
                    }
                    result
                }
            };
            return if success {
                Ok(())
            } else {
                Err(err("NSPasteboard#writeObjects: returned false"))
            };
        }
        fn get_binary_contents(&mut self) -> Result<Option<ClipboardContent>, Box<Error>> {
            let string_class: Id<NSObject> = {
                let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
                unsafe { transmute(cls) }
            };
            let image_class: Id<NSObject> = {
                let cls: Id<Class> = unsafe { Id::from_ptr(class("NSImage")) };
                unsafe { transmute(cls) }
            };
            let url_class: Id<NSObject> = {
                let cls: Id<Class> = unsafe { Id::from_ptr(class("NSURL")) };
                unsafe { transmute(cls) }
            };
            let classes = <[_]>::into_vec(box [url_class, image_class, string_class]);
            let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(classes);
            let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
            let contents: Id<NSArray<NSObject>> = unsafe {
                let obj: *mut NSArray<NSObject> = {
                    let sel = {
                        {
                            #[allow(deprecated)]
                            #[inline(always)]
                            fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                unsafe {
                                    static SEL: ::std::sync::atomic::AtomicUsize =
                                        ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                    let ptr = SEL.load(::std::sync::atomic::Ordering::Relaxed)
                                        as *const ::std::os::raw::c_void;
                                    if ptr.is_null() {
                                        let sel = ::objc::runtime::sel_registerName(
                                            name.as_ptr() as *const _
                                        );
                                        SEL.store(
                                            sel.as_ptr() as usize,
                                            ::std::sync::atomic::Ordering::Relaxed,
                                        );
                                        sel
                                    } else {
                                        ::objc::runtime::Sel::from_ptr(ptr)
                                    }
                                }
                            }
                            register_sel("readObjectsForClasses:options:\u{0}")
                        }
                    };
                    let result;
                    match ::objc::__send_message(&*self.pasteboard, sel, (&*classes, &*options)) {
                        Err(s) => ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                            &[""],
                            &match (&s,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        )),
                        Ok(r) => result = r,
                    }
                    result
                };
                if obj.is_null() {
                    return Err(err(
                        "pasteboard#readObjectsForClasses:options: returned null",
                    ));
                }
                Id::from_ptr(obj)
            };
            if contents.count() == 0 {
                Ok(None)
            } else {
                let obj = &contents[0];
                if obj.is_kind_of(Class::get("NSString").unwrap()) {
                    let s: &NSString = unsafe { transmute(obj) };
                    Ok(Some(ClipboardContent::Utf8(s.as_str().to_owned())))
                } else if obj.is_kind_of(Class::get("NSImage").unwrap()) {
                    let tiff: &NSArray<NSObject> = unsafe {
                        {
                            let sel = {
                                {
                                    #[allow(deprecated)]
                                    #[inline(always)]
                                    fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                        unsafe {
                                            static SEL: ::std::sync::atomic::AtomicUsize =
                                                ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                            let ptr = SEL
                                                .load(::std::sync::atomic::Ordering::Relaxed)
                                                as *const ::std::os::raw::c_void;
                                            if ptr.is_null() {
                                                let sel = ::objc::runtime::sel_registerName(
                                                    name.as_ptr() as *const _,
                                                );
                                                SEL.store(
                                                    sel.as_ptr() as usize,
                                                    ::std::sync::atomic::Ordering::Relaxed,
                                                );
                                                sel
                                            } else {
                                                ::objc::runtime::Sel::from_ptr(ptr)
                                            }
                                        }
                                    }
                                    register_sel("TIFFRepresentation\u{0}")
                                }
                            };
                            let result;
                            match ::objc::__send_message(&*obj, sel, ()) {
                                Err(s) => {
                                    ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                                        &[""],
                                        &match (&s,) {
                                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                                arg0,
                                                ::core::fmt::Display::fmt,
                                            )],
                                        },
                                    ))
                                }
                                Ok(r) => result = r,
                            }
                            result
                        }
                    };
                    let len: usize = unsafe {
                        {
                            let sel = {
                                {
                                    #[allow(deprecated)]
                                    #[inline(always)]
                                    fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                        unsafe {
                                            static SEL: ::std::sync::atomic::AtomicUsize =
                                                ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                            let ptr = SEL
                                                .load(::std::sync::atomic::Ordering::Relaxed)
                                                as *const ::std::os::raw::c_void;
                                            if ptr.is_null() {
                                                let sel = ::objc::runtime::sel_registerName(
                                                    name.as_ptr() as *const _,
                                                );
                                                SEL.store(
                                                    sel.as_ptr() as usize,
                                                    ::std::sync::atomic::Ordering::Relaxed,
                                                );
                                                sel
                                            } else {
                                                ::objc::runtime::Sel::from_ptr(ptr)
                                            }
                                        }
                                    }
                                    register_sel("length\u{0}")
                                }
                            };
                            let result;
                            match ::objc::__send_message(&*tiff, sel, ()) {
                                Err(s) => {
                                    ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                                        &[""],
                                        &match (&s,) {
                                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                                arg0,
                                                ::core::fmt::Display::fmt,
                                            )],
                                        },
                                    ))
                                }
                                Ok(r) => result = r,
                            }
                            result
                        }
                    };
                    let bytes: *const u8 = unsafe {
                        {
                            let sel = {
                                {
                                    #[allow(deprecated)]
                                    #[inline(always)]
                                    fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                        unsafe {
                                            static SEL: ::std::sync::atomic::AtomicUsize =
                                                ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                            let ptr = SEL
                                                .load(::std::sync::atomic::Ordering::Relaxed)
                                                as *const ::std::os::raw::c_void;
                                            if ptr.is_null() {
                                                let sel = ::objc::runtime::sel_registerName(
                                                    name.as_ptr() as *const _,
                                                );
                                                SEL.store(
                                                    sel.as_ptr() as usize,
                                                    ::std::sync::atomic::Ordering::Relaxed,
                                                );
                                                sel
                                            } else {
                                                ::objc::runtime::Sel::from_ptr(ptr)
                                            }
                                        }
                                    }
                                    register_sel("bytes\u{0}")
                                }
                            };
                            let result;
                            match ::objc::__send_message(&*tiff, sel, ()) {
                                Err(s) => {
                                    ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                                        &[""],
                                        &match (&s,) {
                                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                                arg0,
                                                ::core::fmt::Display::fmt,
                                            )],
                                        },
                                    ))
                                }
                                Ok(r) => result = r,
                            }
                            result
                        }
                    };
                    let vec = unsafe { std::slice::from_raw_parts(bytes, len) };
                    Ok(Some(ClipboardContent::Tiff(vec.into())))
                } else if obj.is_kind_of(Class::get("NSURL").unwrap()) {
                    let s: &NSString = unsafe {
                        {
                            let sel = {
                                {
                                    #[allow(deprecated)]
                                    #[inline(always)]
                                    fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                        unsafe {
                                            static SEL: ::std::sync::atomic::AtomicUsize =
                                                ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                            let ptr = SEL
                                                .load(::std::sync::atomic::Ordering::Relaxed)
                                                as *const ::std::os::raw::c_void;
                                            if ptr.is_null() {
                                                let sel = ::objc::runtime::sel_registerName(
                                                    name.as_ptr() as *const _,
                                                );
                                                SEL.store(
                                                    sel.as_ptr() as usize,
                                                    ::std::sync::atomic::Ordering::Relaxed,
                                                );
                                                sel
                                            } else {
                                                ::objc::runtime::Sel::from_ptr(ptr)
                                            }
                                        }
                                    }
                                    register_sel("absoluteString\u{0}")
                                }
                            };
                            let result;
                            match ::objc::__send_message(&*obj, sel, ()) {
                                Err(s) => {
                                    ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                                        &[""],
                                        &match (&s,) {
                                            (arg0,) => [::core::fmt::ArgumentV1::new(
                                                arg0,
                                                ::core::fmt::Display::fmt,
                                            )],
                                        },
                                    ))
                                }
                                Ok(r) => result = r,
                            }
                            result
                        }
                    };
                    Ok(Some(ClipboardContent::Utf8(s.as_str().to_owned())))
                } else {
                    Err(err(
                        "pasteboard#readObjectsForClasses:options: returned unknown class",
                    ))
                }
            }
        }
        fn get_image(&mut self) -> Result<ImageData, Box<dyn Error>> {
            Err("Not implemented".into())
        }
        fn set_image(&mut self, data: ImageData) -> Result<(), Box<dyn Error>> {
            let pixels = data.bytes.into();
            let image = image_from_pixels(pixels, data.width, data.height)?;
            let objects: Id<NSArray<NSObject, Owned>> =
                NSArray::from_vec(<[_]>::into_vec(box [image]));
            let _: usize = unsafe {
                {
                    let sel = {
                        {
                            #[allow(deprecated)]
                            #[inline(always)]
                            fn register_sel(name: &str) -> ::objc::runtime::Sel {
                                unsafe {
                                    static SEL: ::std::sync::atomic::AtomicUsize =
                                        ::std::sync::atomic::ATOMIC_USIZE_INIT;
                                    let ptr = SEL.load(::std::sync::atomic::Ordering::Relaxed)
                                        as *const ::std::os::raw::c_void;
                                    if ptr.is_null() {
                                        let sel = ::objc::runtime::sel_registerName(
                                            name.as_ptr() as *const _
                                        );
                                        SEL.store(
                                            sel.as_ptr() as usize,
                                            ::std::sync::atomic::Ordering::Relaxed,
                                        );
                                        sel
                                    } else {
                                        ::objc::runtime::Sel::from_ptr(ptr)
                                    }
                                }
                            }
                            register_sel("writeObjects:\u{0}")
                        }
                    };
                    let result;
                    match ::objc::__send_message(&*self.pasteboard, sel, (&*objects,)) {
                        Err(s) => ::std::rt::begin_panic_fmt(&::core::fmt::Arguments::new_v1(
                            &[""],
                            &match (&s,) {
                                (arg0,) => [::core::fmt::ArgumentV1::new(
                                    arg0,
                                    ::core::fmt::Display::fmt,
                                )],
                            },
                        )),
                        Ok(r) => result = r,
                    }
                    result
                }
            };
            Ok(())
        }
    }
    #[inline]
    pub fn class(name: &str) -> *mut Class {
        unsafe { transmute(Class::get(name)) }
    }
}
