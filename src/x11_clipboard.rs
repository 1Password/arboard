use std::mem::{size_of, transmute, uninitialized};

use libc::*;
use x11::xlib::*;
use x11::xmu::*;

use std::{ptr, slice, thread};
use std::env::set_current_dir;
use std::path::Path;

pub struct ClipboardContext {
    getter: ClipboardContextGetter,
}

pub struct ClipboardContextGetter {
    display: *mut Display,
    window: Window,
    selection: Atom,
    utf8string: Atom,
}

impl ClipboardContextGetter {
    pub fn new() -> Result<ClipboardContextGetter, &'static str> {
        // http://sourceforge.net/p/xclip/code/HEAD/tree/trunk/xclip.c
        let dpy = unsafe { XOpenDisplay(0 as *mut c_char) };
        if dpy.is_null() {
            return Err("XOpenDisplay")
        }
        let win = unsafe { XCreateSimpleWindow(dpy, XDefaultRootWindow(dpy), 0, 0, 1, 1, 0, 0, 0) };
        if win == 0 {
            return Err("XCreateSimpleWindow")
        }
        if unsafe { XSelectInput(dpy, win, PropertyChangeMask) } == 0 {
            return Err("XSelectInput");
        }
        let sel = unsafe { XmuInternAtom(dpy, _XA_CLIPBOARD) };
        if sel == 0 {
            return Err("XA_CLIPBOARD")
        }
        let utf8 = unsafe { XmuInternAtom(dpy, _XA_UTF8_STRING) };
        if utf8 == 0 {
            return Err("XA_UTF8_STRING")
        }
        Ok(ClipboardContextGetter {
            display: dpy,
            window: win,
            selection: sel,
            utf8string: utf8,
        })
    }

    pub fn get_contents(&self) -> Result<String, &str> {
        enum XCOutState {
            None,
            SentConvSel,
            BadTarget,
            Incr,
        };
        fn mach_itemsize(format: c_int) -> usize {
            match format {
                8 => size_of::<c_char>(),
                16 => size_of::<c_short>(),
                32 => size_of::<c_long>(),
                _ => panic!("unexpected format for mach_itemsize: {}", format),
            }
        }
        fn xcout(dpy: *mut Display, win: Window, evt: &mut XEvent,
                sel: Atom, target: Atom, type_: &mut Atom, dest: &mut Vec<u8>,
                context: &mut XCOutState) {
            let pty_atom = unsafe { XInternAtom(dpy, b"SERVO_CLIPBOARD_OUT\0".as_ptr() as *mut ::libc::c_char, 0) };
            let incr_atom = unsafe { XInternAtom(dpy, b"INCR\0".as_ptr() as *mut ::libc::c_char, 0) };

            let mut buffer: *mut c_uchar = ptr::null_mut();
            let mut pty_format: c_int = 0;
            let mut pty_size: c_ulong = 0;
            let mut pty_items: c_ulong = 0;

            match *context {
                XCOutState::None => {
                    unsafe { XConvertSelection(dpy, sel, target, pty_atom, win, CurrentTime); }
                    *context = XCOutState::SentConvSel;
                    return;
                },
                XCOutState::SentConvSel => {
                    let event: &mut XSelectionEvent = unsafe { transmute(evt) };
                    if event.type_ != SelectionNotify {
                        return;
                    }
                    if event.property == 0 {
                        *context = XCOutState::BadTarget;
                        return;
                    }
                    unsafe {
                        XGetWindowProperty(dpy, win, pty_atom, 0, 0, 0, 0, type_,
                                            &mut pty_format, &mut pty_items, &mut pty_size,
                                            &mut buffer);
                        XFree(buffer as *mut c_void);
                    }
                    if *type_ == incr_atom {
                        unsafe {
                            XDeleteProperty(dpy, win, pty_atom);
                            XFlush(dpy);
                        }
                        *context = XCOutState::Incr;
                        return;
                    }
                    unsafe {
                        XGetWindowProperty(dpy, win, pty_atom, 0, pty_size as c_long, 0, 0, type_,
                                            &mut pty_format, &mut pty_items, &mut pty_size,
                                            &mut buffer);
                    }
                    let pty_machsize: c_ulong = pty_items * (mach_itemsize(pty_format) as c_ulong);
                    dest.push_all(unsafe { slice::from_raw_parts_mut(buffer, (pty_machsize as usize) / size_of::<u8>()) });
                    *context = XCOutState::None;
                },
                XCOutState::BadTarget => panic!("should be unreachable"),
                XCOutState::Incr => {
                    let event: &mut XPropertyEvent = unsafe { transmute(evt) };
                    if event.type_ != PropertyNotify {
                        return;
                    }
                    if event.state != PropertyNewValue {
                        return;
                    }
                    unsafe {
                        XGetWindowProperty(dpy, win, pty_atom, 0, 0, 0, 0, type_,
                                            &mut pty_format, &mut pty_items, &mut pty_size,
                                            &mut buffer);
                        XFree(buffer as *mut c_void);
                    }
                    if pty_size == 0 {
                        unsafe {
                            XDeleteProperty(dpy, win, pty_atom);
                            XFlush(dpy);
                        }
                        *context = XCOutState::None;
                        return;
                    }
                    unsafe {
                        XGetWindowProperty(dpy, win, pty_atom, 0, pty_size as c_long, 0, 0, type_,
                                            &mut pty_format, &mut pty_items, &mut pty_size,
                                            &mut buffer);
                    }
                    let pty_machsize: c_ulong = pty_items * (mach_itemsize(pty_format) as c_ulong);
                    dest.push_all(unsafe { slice::from_raw_parts_mut(buffer, (pty_machsize as usize) / size_of::<u8>()) });
                    *context = XCOutState::None;
                },
            }
        }
        let mut sel_buf = vec![];
        let mut sel_type = 0;
        let mut state = XCOutState::None;
        let mut event: XEvent = unsafe { uninitialized() };
        let mut target = self.utf8string;
        loop {
            if let XCOutState::None = state {} else {
                unsafe { XNextEvent(self.display, &mut event) };
            }
            xcout(self.display, self.window, &mut event, self.selection, target, &mut sel_type, &mut sel_buf, &mut state);
            if let XCOutState::BadTarget = state {
                if target == self.utf8string {
                    state = XCOutState::None;
                    target = XA_STRING;
                    continue;
                }
                else {
                    return Err("unable to negotiate format");
                }
            }
            if let XCOutState::None = state {
                break;
            }
        }
        Ok(String::from_utf8_lossy(&sel_buf).into_owned())
    }
}

impl Drop for ClipboardContextGetter {
    fn drop(&mut self) {
        let retcode = unsafe { XCloseDisplay(self.display) };
        if retcode != 0 {
            panic!("XCloseDisplay failed. (return code {})", retcode);
        }
    }
}

impl ClipboardContext {
    pub fn new() -> Result<ClipboardContext, &'static str> {
        let getter = try!(ClipboardContextGetter::new());
        Ok(ClipboardContext {
            getter: getter,
        })
    }

    pub fn get_contents(&self) -> Result<String, &str> {
        self.getter.get_contents()
    }

    pub fn set_contents(&self, string_to_copy: String) -> Result<(), &str> {
        // Under x11, "copying data into the clipboard" is actually
        //  accomplished by starting a "server" process that owns the
        //  copied data, and streams copied chunks of it through an
        //  event loop when another process requests it.

        // xclip uses fork(2) to ensure that the clipboard "server"
        //  outlives the process that generated the data.

        // Since there are potential complications of using fork(2)
        //  from rust (e.g. multiple calls of destructors), threads are
        //  used for now (until the complications are reviewed in more
        //  detail). As such, the clipboard "server" provided by this
        //  function will not outlive the calling process.

        // chdir to / in case the directory of the program is removed/unmounted
        if let Err(_) = set_current_dir(Path::new("/")) {
            return Err("set_current_dir");
        }

        #[derive(Debug)]
        enum XCInState {
            None,
            //SeqRel, // this is defined in xclib.h, but never used
            Incr,
        }

        // result indicates whether the transfer is finished
        fn xcin(dpy: *mut Display, win: &mut Window, evt: &XEvent,
                pty: &mut Atom, target: Atom, txt: &[u8], pos: &mut usize,
                context: &mut XCInState, &targets: &Atom, &incr_atom: &Atom) -> bool {
            // xclip cites ICCCM 2.5 for this heuristic
            let mut chunk_size = unsafe { XExtendedMaxRequestSize(dpy) / 4 } as usize;
            if chunk_size == 0 {
                chunk_size = unsafe { XMaxRequestSize(dpy) / 4 } as usize;
            }

            match *context {
                XCInState::None => {
                    if evt.get_type() != SelectionRequest {
                        return false;
                    }
                    let event: &XSelectionRequestEvent = unsafe { transmute(evt) };

                    *win = event.requestor;
                    *pty = event.property;

                    *pos = 0;
                    if event.target == targets {
                        let types: *mut u8 = unsafe { transmute([targets, target].as_mut_ptr()) };
                        unsafe { XChangeProperty(dpy, *win, *pty, XA_ATOM, 32, PropModeReplace, types, 2) };
                    }
                    else if txt.len() > chunk_size {
                        unsafe {
                            XChangeProperty(dpy, *win, *pty, incr_atom, 32, PropModeReplace, ptr::null(), 0);
                            XSelectInput(dpy, *win, PropertyChangeMask);
                        }
                        *context = XCInState::Incr;
                    }
                    else {
                        unsafe { XChangeProperty(dpy, *win, *pty, target, 8, PropModeReplace, txt.as_ptr(), txt.len() as c_int) };
                    }
                    let mut response: XEvent = XSelectionEvent {
                        property: *pty,
                        type_: SelectionNotify,
                        display: event.display,
                        requestor: event.requestor,
                        selection: event.selection,
                        target: event.target,
                        time: event.time,
                        serial: unsafe { uninitialized() },
                        send_event: unsafe { uninitialized() },
                    }.into();
                    unsafe {
                        XSendEvent(dpy, event.requestor, 0, 0, &mut response);
                        XFlush(dpy);
                    }
                    if event.target == targets {
                        return false;
                    }
                    return if txt.len() > chunk_size { false } else { true };
                },
                XCInState::Incr => {
                    if evt.get_type() != PropertyNotify {
                        return false;
                    };
                    let event: &XPropertyEvent = unsafe { transmute(evt) };
                    if event.state != PropertyDelete {
                        return false;
                    }
                    let mut chunk_len = chunk_size;
                    if (*pos + chunk_len) > txt.len() {
                        chunk_len = txt.len() - *pos;
                    }
                    if *pos > txt.len() {
                        chunk_len = 0;
                    }
                    unsafe {
                        if chunk_len != 0 {
                            XChangeProperty(dpy, *win, *pty, target, 8, PropModeReplace, &txt[*pos], chunk_len as c_int);
                        }
                        else {
                            XChangeProperty(dpy, *win, *pty, target, 8, PropModeReplace, ptr::null(), 0);
                        }
                        XFlush(dpy);
                    }
                    if chunk_len != 0 {
                        *context = XCInState::None
                    }
                    *pos += chunk_size;
                    return if chunk_len > 0 { false } else { true };
                },
            }
        }

        // TODO: some mechanism for reusing the clipboard-server thread / avoiding resource leaks
        thread::spawn(move || {
            let display: *mut Display = unsafe { XOpenDisplay(0 as *mut c_char) };
            if display.is_null() { return; }
            let win = unsafe { XCreateSimpleWindow(display, XDefaultRootWindow(display), 0, 0, 1, 1, 0, 0, 0) };
            if win == 0 { return; }
            let sel = unsafe { XmuInternAtom(display, _XA_CLIPBOARD) };
            if sel == 0 { return; }

            unsafe {
                XSelectInput(display, win, PropertyChangeMask);
                XSetSelectionOwner(display, sel, win, CurrentTime);
            }

            let mut event: XEvent = unsafe { uninitialized() };
            let mut clear = false;
            let mut context = XCInState::None;
            let mut position = 0;
            let mut cwin = unsafe { uninitialized() };
            let mut pty = unsafe { uninitialized() };
            let target = XA_STRING;

            let targets = unsafe { XInternAtom(display, b"TARGETS\0".as_ptr() as *mut i8, 0) };
            let incr_atom = unsafe { XInternAtom(display, b"INCR\0".as_ptr() as *mut i8, 0) };

            // https://github.com/rust-lang/rust/issues/25343
            'outer: loop {
                'inner: loop {
                    unsafe { XNextEvent(display, &mut event) };
                    let finished = xcin(display, &mut cwin, &event, &mut pty, target, string_to_copy.as_bytes(), &mut position, &mut context, &targets, &incr_atom);
                    if event.get_type() == SelectionClear {
                        clear = true;
                    }
                    if let XCInState::None = context {
                        if clear {
                            break 'outer;
                        }
                    }
                    if finished {
                        break 'inner;
                    }
                }
            }
        });
        Ok(())
    }
}
