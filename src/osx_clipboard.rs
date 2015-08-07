use objc::runtime::{Object, Class};
use objc_foundation::{INSArray, INSString, INSObject};
use objc_foundation::{NSArray, NSDictionary, NSString, NSObject};
use objc_id::{Id, Owned};
use std::error::Error;
use std::mem::transmute;
use std::io::{stderr, Write};

pub struct ClipboardContext {
    pasteboard: Id<Object>,
}

// required to bring NSPasteboard into the path of the class-resolver
#[link(name = "AppKit", kind = "framework")]
extern {}

impl ClipboardContext {
    pub fn new() -> Result<ClipboardContext, Box<Error+Sync+Send>> {
        let cls = try!(Class::get("NSPasteboard").ok_or(Box::<Error+Send+Sync>::from("Class::get(\"NSPasteboard\")")));
        let pasteboard = unsafe { Id::from_ptr(msg_send![cls, generalPasteboard]) };
        writeln!(stderr(), "pasteboard: {:p}", pasteboard);
        Ok(ClipboardContext {
            pasteboard: pasteboard,
        })
    }
    pub fn get_contents(&self) -> Result<String, Box<Error+Sync+Send>> {
        let string_class: Id<NSObject> = {
            let cls: Id<Class> = unsafe { Id::from_ptr(class("NSString")) };
            unsafe { transmute(cls) }
        };
        let classes: Id<NSArray<NSObject, Owned>> = NSArray::from_vec(vec![string_class]);
        let options: Id<NSDictionary<NSObject, NSObject>> = NSDictionary::new();
        let string_array: *mut Object = unsafe { msg_send![self.pasteboard, readObjectsForClasses:&*classes options:&*options] };
        if string_array.is_null() {
            return Err("pasteboard#readObjectsForClasses:options: returned null".into());
        }
        let length: usize = unsafe { msg_send![string_array, count] };
        if length == 0 {
            return Err("pasteboard#readObjectsForClasses:options: returned empty".into());
        }
        let string: Id<NSString> = {
            let obj: *mut Object = unsafe { msg_send![string_array, objectAtIndex:0] };
            let wrapped: Id<Object> = unsafe { Id::from_ptr(obj) };
            unsafe { transmute(wrapped) }
        };
        Ok(string.as_str().to_owned())
    }
    pub fn set_contents(&self, data: String) -> Result<(), Box<Error+Sync+Send>> {
        let string_array = NSArray::from_vec(vec![NSString::from_str(&data)]);
        let _: usize = unsafe { msg_send![self.pasteboard, clearContents] };
        let success: bool = unsafe { msg_send![self.pasteboard, writeObjects:string_array] };
        return if success { Ok(()) } else {
            Err(Box::<Error+Send+Sync>::from("NSPasteboard#writeObjects: returned false"))
        }
    }
}

// this is a convenience function that both cocoa-rs and
//  glutin define, which seems to depend on the fact that
//  Option::None has the same representation as a null pointer
#[inline]
pub fn class(name: &str) -> *mut Class {
    unsafe {
        transmute(Class::get(name))
    }
}
