/*
Copyright (C) 2016 Avraham Weinstock

This program is free software; you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation; either version 2 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License along
with this program; if not, write to the Free Software Foundation, Inc.,
51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
*/

use common::*;
use std::mem::{size_of, transmute, uninitialized};

use x11::xlib::*;
use x11::xmu::*;

use std::{ptr, slice, thread};
use std::os::raw::*;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::error::Error;

pub struct X11ClipboardContext {
    getter: X11ClipboardContextGetter,
    transmit_clear: Sender<()>,
    transmit_data: Sender<String>,
    first_send: bool,
}

pub struct X11ClipboardContextGetter {
    display: *mut Display,
    window: Window,
    selection: Atom,
    utf8string: Atom,
}

pub struct X11ClipboardContextSetter {
    display: *mut Display,
    window: Window,
    selection: Atom,
    chunk_size: usize,
    receive_clear: Receiver<()>,
}

impl X11ClipboardContextGetter {
    pub fn new() -> Result<X11ClipboardContextGetter, Box<Error>> {
        // http://sourceforge.net/p/xclip/code/HEAD/tree/trunk/xclip.c
        let dpy = unsafe { XOpenDisplay(0 as *mut c_char) };
        if dpy.is_null() {
            return Err(err("XOpenDisplay"))
        }
        let win = unsafe { XCreateSimpleWindow(dpy, XDefaultRootWindow(dpy), 0, 0, 1, 1, 0, 0, 0) };
        if win == 0 {
            return Err(err("XCreateSimpleWindow"))
        }
        if unsafe { XSelectInput(dpy, win, PropertyChangeMask) } == 0 {
            return Err(err("XSelectInput"));
        }
        let sel = unsafe { XmuInternAtom(dpy, _XA_CLIPBOARD) };
        if sel == 0 {
            return Err(err("XA_CLIPBOARD"))
        }
        let utf8 = unsafe { XmuInternAtom(dpy, _XA_UTF8_STRING) };
        if utf8 == 0 {
            return Err(err("XA_UTF8_STRING"))
        }
        Ok(X11ClipboardContextGetter {
            display: dpy,
            window: win,
            selection: sel,
            utf8string: utf8,
        })
    }

    pub fn get_contents(&mut self) -> Result<String, Box<Error>> {
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
            let pty_atom = unsafe { XInternAtom(dpy, b"SERVO_CLIPBOARD_OUT\0".as_ptr() as *mut c_char, 0) };
            let incr_atom = unsafe { XInternAtom(dpy, b"INCR\0".as_ptr() as *mut c_char, 0) };

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
                        XFree(buffer as *mut _);
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
                    dest.extend_from_slice(unsafe { slice::from_raw_parts_mut(buffer, (pty_machsize as usize) / size_of::<u8>()) });
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
                        XFree(buffer as *mut _);
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
                    dest.extend_from_slice(unsafe { slice::from_raw_parts_mut(buffer, (pty_machsize as usize) / size_of::<u8>()) });
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
                    return Err(err("unable to negotiate format"));
                }
            }
            if let XCOutState::None = state {
                break;
            }
        }
        Ok(String::from_utf8_lossy(&sel_buf).into_owned())
    }
}

// Under x11, "copying data into the clipboard" is actually
//  accomplished by starting a "server" process that owns the
//  copied data, and streams copied chunks of it through an
//  event loop when another process requests it.

// xclip uses fork(2) to ensure that the clipboard "server"
//  outlives the process that generated the data.

// Since there are potential complications of using fork(2)
//  from rust (e.g. multiple calls of destructors), threads are
//  used for now (until the complications are reviewed in more
//  detail). As such, the clipboard "server" provided by
//  X11ClipboardContext::set_contents will not outlive the calling
//  process.

// X11ClipboardContextSetter is intended to be created {on the thread,
//  in the process} that will be serving the clipboard data.

impl X11ClipboardContextSetter {
    pub fn new(receive_clear: Receiver<()>) -> Result<X11ClipboardContextSetter, Box<Error>> {
        let dpy = unsafe { XOpenDisplay(0 as *mut c_char) };
        if dpy.is_null() {
            return Err(err("XOpenDisplay"))
        }
        let win = unsafe { XCreateSimpleWindow(dpy, XDefaultRootWindow(dpy), 0, 0, 1, 1, 0, 0, 0) };
        if win == 0 {
            return Err(err("XCreateSimpleWindow"))
        }
        if unsafe { XSelectInput(dpy, win, PropertyChangeMask) } == 0 {
            return Err(err("XSelectInput"));
        }
        let sel = unsafe { XmuInternAtom(dpy, _XA_CLIPBOARD) };
        if sel == 0 {
            return Err(err("XA_CLIPBOARD"))
        }
        // xclip cites ICCCM 2.5 for this heuristic
        let mut chunk_size = unsafe { XExtendedMaxRequestSize(dpy) / 4 } as usize;
        if chunk_size == 0 {
            chunk_size = unsafe { XMaxRequestSize(dpy) / 4 } as usize;
        }
        if chunk_size == 0 {
            return Err(err("XExtendedMaxRequestSize/XMaxRequestSize"));
        }

        Ok(X11ClipboardContextSetter {
            display: dpy,
            window: win,
            selection: sel,
            chunk_size: chunk_size,
            receive_clear: receive_clear,
        })
    }
    pub fn set_contents(&self, string_to_copy: String) {
        #[derive(Debug)]
        enum XCInState {
            None,
            //SeqRel, // this is defined in xclib.h, but never used
            Incr(Window, Atom, usize),
        }

        // result indicates whether the transfer is finished
        fn xcin(dpy: *mut Display, evt: &XEvent, target: Atom, txt: &[u8], context: &mut XCInState,
                &targets: &Atom, &incr_atom: &Atom, chunk_size: usize) -> bool {
            match *context {
                XCInState::None => {
                    if evt.get_type() != SelectionRequest {
                        return false;
                    }
                    let event: &XSelectionRequestEvent = unsafe { transmute(evt) };

                    if event.target == targets {
                        let types: *mut u8 = unsafe { transmute([targets, target].as_mut_ptr()) };
                        unsafe { XChangeProperty(dpy, event.requestor, event.property, XA_ATOM, 32, PropModeReplace, types, 2) };
                    }
                    else if txt.len() > chunk_size {
                        unsafe {
                            XChangeProperty(dpy, event.requestor, event.property, incr_atom, 32, PropModeReplace, ptr::null(), 0);
                            XSelectInput(dpy, event.requestor, PropertyChangeMask);
                        }
                        *context = XCInState::Incr(event.requestor, event.property, 0);
                    }
                    else {
                        unsafe { XChangeProperty(dpy, event.requestor, event.property, target, 8, PropModeReplace, txt.as_ptr(), txt.len() as c_int) };
                    }
                    let mut response: XEvent = XSelectionEvent {
                        property: event.property,
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
                XCInState::Incr(win, pty, pos) => {
                    if evt.get_type() != PropertyNotify {
                        return false;
                    };
                    let event: &XPropertyEvent = unsafe { transmute(evt) };
                    if event.state != PropertyDelete {
                        return false;
                    }
                    let mut chunk_len = chunk_size;
                    if (pos + chunk_len) > txt.len() {
                        chunk_len = txt.len() - pos;
                    }
                    if pos > txt.len() {
                        chunk_len = 0;
                    }
                    unsafe {
                        if chunk_len != 0 {
                            XChangeProperty(dpy, win, pty, target, 8, PropModeReplace, &txt[pos], chunk_len as c_int);
                        }
                        else {
                            XChangeProperty(dpy, win, pty, target, 8, PropModeReplace, ptr::null(), 0);
                        }
                        XFlush(dpy);
                    }
                    if chunk_len != 0 {
                        *context = XCInState::None
                    } else {
                        *context = XCInState::Incr(win, pty, pos + chunk_size);
                    }
                    return if chunk_len > 0 { false } else { true };
                },
            }
        }

        unsafe {
            XSetSelectionOwner(self.display, self.selection, self.window, CurrentTime);
        }

        let mut event: XEvent = unsafe { uninitialized() };
        let mut clear = false;
        let mut context = XCInState::None;
        let target = XA_STRING;

        let targets = unsafe { XInternAtom(self.display, b"TARGETS\0".as_ptr() as *mut c_char, 0) };
        let incr_atom = unsafe { XInternAtom(self.display, b"INCR\0".as_ptr() as *mut c_char, 0) };

        'outer: loop {
            'inner: loop {
                unsafe { XNextEvent(self.display, &mut event) };
                let finished = xcin(self.display, &event, target, string_to_copy.as_bytes(), &mut context, &targets, &incr_atom, self.chunk_size);
                if event.get_type() == SelectionClear {
                    clear = true;
                }
                if let Ok(()) = self.receive_clear.try_recv() {
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
    }
}

fn close_display_or_panic(display: *mut Display) {
    let retcode = unsafe { XCloseDisplay(display) };
    if retcode != 0 {
        panic!("XCloseDisplay failed. (return code {})", retcode);
    }
}

impl Drop for X11ClipboardContextGetter {
    fn drop(&mut self) {
        close_display_or_panic(self.display);
    }
}

impl Drop for X11ClipboardContextSetter {
    fn drop(&mut self) {
        close_display_or_panic(self.display);
    }
}

impl X11ClipboardContext {
    pub fn new() -> Result<X11ClipboardContext, Box<Error>> {
        let getter = try!(X11ClipboardContextGetter::new());

        let (transmit_clear, receive_clear) = channel();
        let (transmit_data, receive_data) = channel();

        thread::spawn(move || {
            let setter = X11ClipboardContextSetter::new(receive_clear).unwrap();
            for data in receive_data.iter() {
                setter.set_contents(data);
            }
        });

        Ok(X11ClipboardContext {
            getter: getter,
            transmit_clear: transmit_clear,
            transmit_data: transmit_data,
            first_send: true,
        })
    }

    pub fn get_contents(&mut self) -> Result<String, Box<Error>> {
        self.getter.get_contents()
    }

    pub fn set_contents(&mut self, data: String) -> Result<(), Box<Error>> {
        if !self.first_send {
            try!(self.transmit_clear.send(()));
        }
        try!(self.transmit_data.send(data));
        self.first_send = false;
        Ok(())
    }
}
