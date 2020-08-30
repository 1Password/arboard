//!
//!
//!
//!
//! This implementation is a port of https://github.com/dacap/clip
//! The most of the structure and most comments are retained from the source.
//!
//! Most changes are to conform with Rusts rules for example there are multiple overloads of
//! the `get_atom` functtion in the original but there's no function overloading in Rust so
//! those a split apart into functions with different names. (`get_atom_by_id` and the other one
//! at the time of writing I haven't needed to use)
//!
//! More noteably the `Manager` class had to be split into mutliple `structs` and some member
//! functions were made into global functions to conform  Rust's synchronization API
//! (mutexes and locks).
//!
//!
//!

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::marker::PhantomData;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex, MutexGuard, RwLock,
};
use std::time::Duration;

use common::*;

use xcb::ffi::xproto::{
    xcb_atom_t, xcb_change_property, xcb_get_property, xcb_get_property_reply,
    xcb_get_property_reply_t, xcb_get_property_value, xcb_get_property_value_length,
    xcb_get_selection_owner_reply, xcb_property_notify_event_t, xcb_selection_clear_event_t,
    xcb_selection_notify_event_t, xcb_selection_request_event_t, xcb_send_event, XCB_ATOM_NONE,
    XCB_EVENT_MASK_NO_EVENT, XCB_PROPERTY_NEW_VALUE, XCB_PROP_MODE_REPLACE, XCB_SELECTION_NOTIFY,
};
use xcb::{
    self,
    xproto::{self, get_selection_owner},
};

use libc;

const ATOM: usize = 0;
const INCR: usize = 1;
const TARGETS: usize = 2;
const CLIPBOARD: usize = 3;
const MIME_IMAGE_PNG: usize = 4;
const ATOM_PAIR: usize = 5;
const SAVE_TARGETS: usize = 6;
const MULTIPLE: usize = 7;
const CLIPBOARD_MANAGER: usize = 8;

static COMMON_ATOM_NAMES: [&'static str; 9] = [
    "ATOM",
    "INCR",
    "TARGETS",
    "CLIPBOARD",
    "image/png",
    "ATOM_PAIR",
    "SAVE_TARGETS",
    "MULTIPLE",
    "CLIPBOARD_MANAGER",
];

type BufferPtr = Option<Vec<u8>>;
type Atoms = Vec<xcb::xproto::Atom>;
type NotifyCallback = Option<Arc<dyn (Fn() -> bool) + Send + Sync + 'static>>;

lazy_static! {
    static ref MANAGER: Option<Mutex<Manager>> = {
        let manager = Manager::new().ok();
        manager.map(|m| Mutex::new(m))
    };
}
lazy_static! {
    /// Connection to X11 server
    /// This is an `Arc<Mutex<>>` because it's shared between the user's thread and
    /// the x11 event processing thread.
    static ref SHARED_STATE: Mutex<SharedState> = {
        let connection = xcb::Connection::connect(None).unwrap().0;
        Mutex::new(SharedState {
            conn: connection,
            atoms: Default::default(),
            common_atoms: Default::default()
        })
    };

    // Used to wait/notify the arrival of the SelectionNotify event when
    // we requested the clipboard content from other selection owner.
    static ref CONDVAR: Condvar = Condvar::new();

    //static ref ATOM_HANDLER: Mutex<AtomHandler> = Mutex::new(Default::default());
}

/// The name indicates that objects in this struct are shared between
/// the event processing thread and the user tread. However it's important
/// that the `Manager` itself is also shared. So the real reason for splitting these
/// apart from the `Manager` is to conform to Rust's locking and aliasing rules but that is hard to
/// convey in a short name.
struct SharedState {
    conn: xcb::Connection,

    // Cache of known atoms
    atoms: BTreeMap<String, xcb::xproto::Atom>,

    // Cache of common used atoms by us
    common_atoms: Atoms,
}
/// Need to manually `impl Send` because the connection contains pointers,
/// and no pointer is `Send` by default.
unsafe impl Send for SharedState {}

impl SharedState {
    fn get_atom_by_id(&mut self, id: usize) -> xproto::Atom {
        if self.common_atoms.is_empty() {
            self.common_atoms = self.get_atoms(&COMMON_ATOM_NAMES);
        }
        self.common_atoms[id]
    }

    fn get_atoms(&mut self, names: &[&'static str]) -> Atoms {
        let connection = SHARED_STATE.lock().unwrap();

        let mut results = vec![0; names.len()];
        let mut cookies = HashMap::with_capacity(names.len());

        for (res, name) in results.iter_mut().zip(names.iter()) {
            let found = self.atoms.iter().find(|pair| pair.0 == name);
            if let Some(pair) = found {
                *res = *pair.1;
            } else {
                cookies.insert(name, xproto::intern_atom(&connection.conn, false, name));
            }
        }

        for (res, name) in results.iter_mut().zip(names.iter()) {
            if *res == 0 {
                let reply = unsafe {
                    xcb::ffi::xproto::xcb_intern_atom_reply(
                        connection.conn.get_raw_conn(),
                        cookies.get(name).unwrap().cookie,
                        std::ptr::null_mut(),
                    )
                };
                if reply != std::ptr::null_mut() {
                    unsafe {
                        *res = (*reply).atom;
                        self.atoms.insert((*name).into(), *res);
                        libc::free(reply as *mut _);
                    }
                }
            }
        }
        results
    }
}

struct Manager {
    /// Original comment: "Access to the whole Manager"
    ///
    /// Rust Implementation note:
    /// This mutex ensures that certain members of this struct are not accessed
    /// at the same time from both the event processing thread and the user's thread.    
    ///
    /// Using a Mutex like this is not idiomatic Rust. The reason for
    /// this is to match the original implementation.
    ///
    /// I'm sure that a more idiomatic solution is possible but in the interest of time,
    /// I'll stick to the structure of the original code.
    ///
    /// Original is form a cpp library called `clip`. https://github.com/dacap/clip
    //mutex: Mutex<()>,

    // Temporal background window used to own the clipboard and process
    // all events related about the clipboard in a background thread
    window: xcb::xproto::Window,

    // Thread used to run a background message loop to wait X11 events
    // about clipboard. The X11 selection owner will be a hidden window
    // created by us just for the clipboard purpose/communication.
    thread_handle: Option<std::thread::JoinHandle<()>>,

    // WARNING: The callback must not attempt to lock the manager or the shared state.
    // (Otherwise the code needs to be restructured slightly)
    //
    // Internal callback used when a SelectionNotify is received (or the
    // whole data content is received by the INCR method). So this
    // callback can use the notification by different purposes (e.g. get
    // the data length only, or get/process the data content, etc.).
    callback: NotifyCallback,

    // Result returned by the m_callback. Used as return value in the
    // get_data_from_selection_owner() function. For example, if the
    // callback must read a "image/png" file from the clipboard data and
    // fails, the callback can return false and finally the get_image()
    // will return false (i.e. there is data, but it's not a valid image
    // format).
    callback_result: bool,

    // Cache of atoms related to text or image content
    text_atoms: Atoms,
    image_atoms: Atoms,

    // Actual clipboard data generated by us (when we "copy" content in
    // the clipboard, it means that we own the X11 "CLIPBOARD"
    // selection, and in case of SelectionRequest events, we've to
    // return the data stored in this "m_data" field)
    data: BTreeMap<xcb::xproto::Atom, BufferPtr>,

    // Copied image in the clipboard. As we have to transfer the image
    // in some specific format (e.g. image/png) we want to keep a copy
    // of the image and make the conversion when the clipboard data is
    // requested by other process.
    image: super::common::ImageData<'static>,

    // True if we have received an INCR notification so we're going to
    // process several PropertyNotify to concatenate all data chunks.
    incr_process: bool,

    /// Variable used to wait more time if we've received an INCR
    /// notification, which means that we're going to receive large
    /// amounts of data from the selection owner.
    ///mutable bool m_incr_received;
    incr_received: bool,

    // Target/selection format used in the SelectionNotify. Used in the
    // INCR method to get data from the same property in the same format
    // (target) on each PropertyNotify.
    target_atom: xcb::xproto::Atom,

    // Each time we receive data from the selection owner, we put that
    // data in this buffer. If we get the data with the INCR method,
    // we'll concatenate chunks of data in this buffer to complete the
    // whole clipboard content.
    reply_data: BufferPtr,

    // Used to concatenate chunks of data in "m_reply_data" from several
    // PropertyNotify when we are getting the selection owner data with
    // the INCR method.
    reply_offset: usize,
    // List of user-defined formats/atoms.
    //custom_formats: Vec<xcb::xproto::Atom>,
}

impl Manager {
    fn new() -> Result<Self, Box<dyn Error>> {
        use xcb::ffi::xproto::{
            XCB_CW_EVENT_MASK, XCB_EVENT_MASK_PROPERTY_CHANGE, XCB_EVENT_MASK_STRUCTURE_NOTIFY,
            XCB_WINDOW_CLASS_INPUT_OUTPUT,
        };
        let connection = SHARED_STATE.lock().unwrap();
        let setup = connection.conn.get_setup();
        if std::ptr::null() == setup.ptr {
            return Err("Could not get setup for connection".into());
        }
        let screen = setup.roots().data;
        if std::ptr::null() == screen {
            return Err("Could not get screen from setup".into());
        }
        let event_mask =
            // Just in case that some program reports SelectionNotify events
            // with XCB_EVENT_MASK_PROPERTY_CHANGE mask.
            XCB_EVENT_MASK_PROPERTY_CHANGE |
            // To receive DestroyNotify event and stop the message loop.
            XCB_EVENT_MASK_STRUCTURE_NOTIFY;
        let window = connection.conn.generate_id();
        unsafe {
            xcb::ffi::xproto::xcb_create_window(
                connection.conn.get_raw_conn(),
                0,
                window,
                (*screen).root,
                0,
                0,
                1,
                1,
                0,
                XCB_WINDOW_CLASS_INPUT_OUTPUT as _,
                (*screen).root_visual,
                XCB_CW_EVENT_MASK,
                &event_mask,
            );
        }

        let thread_handle = std::thread::spawn(process_x11_events);

        Ok(Manager {
            //mutex: Mutex::new(()),
            window: 0,
            thread_handle: Some(thread_handle),
            callback: None,
            callback_result: false,
            text_atoms: Default::default(),
            image_atoms: Default::default(),
            data: Default::default(),
            image: super::common::ImageData {
                width: 0,
                height: 0,
                bytes: std::borrow::Cow::from(vec![]),
            },
            incr_process: false,
            incr_received: false,
            target_atom: 0,
            reply_data: Default::default(),
            reply_offset: 0,
        })
    }

    //fn get_atom_by_name(&self)

    fn clear_data(&mut self) {
        self.data.clear();
        self.image.width = 0;
        self.image.height = 0;
        self.image.bytes = Vec::new().into();
    }

    fn set_requestor_property_with_clipboard_content(
        &mut self,
        requestor: xproto::Window,
        property: xproto::Atom,
        target: xproto::Atom,
    ) -> bool {
        todo!()
    }

    fn copy_reply_data(&mut self, reply: *mut xcb_get_property_reply_t) {
        let src = unsafe { xcb_get_property_value(reply) } as *const u8;
        // n = length of "src" in bytes
        let n = unsafe { xcb_get_property_value_length(reply) } as usize;
        let req = self.reply_offset + n;
        match &mut self.reply_data {
            None => {
                self.reply_offset = 0; // Rust impl: I added this just to be extra sure.
                self.reply_data = Some(vec![0u8; req]);
            }
            // The "m_reply_data" size can be smaller because the size
            // specified in INCR property is just a lower bound.
            Some(reply_data) => {
                if req > reply_data.len() {
                    reply_data.resize(req, 0u8);
                }
            }
        }
        let src_slice = unsafe { std::slice::from_raw_parts(src, n) };
        self.reply_data.as_mut().unwrap()[self.reply_offset..req].copy_from_slice(src_slice);
        self.reply_offset += n;
    }

    // Rust impl: It's strange, the reply attribute is also unused in the original code.
    fn call_callback(&mut self, _reply: *mut xcb_get_property_reply_t) {
        self.callback_result = false;
        if let Some(callback) = &self.callback {
            self.callback_result = callback();
        }
        CONDVAR.notify_one();

        self.reply_data = None;
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        // TODO this code should be in the drop function because layz_static objects
        // don't get dropped when the program exits. It should instead be in some
        // sort of at_exit function, maybe THE at_exit function which I can access through libc

        if self.data.is_empty() && self.window != 0 && self.window == get_x11_selection_owner() {
            //xcb::xproto::Window
            let mut x11_clipboard_manager = 0;
            {
                let mut shared = SHARED_STATE.lock().unwrap();
                let clipboard_manager_atom = shared.get_atom_by_id(CLIPBOARD_MANAGER);
                let cookie = { get_selection_owner(&shared.conn, clipboard_manager_atom) };
                let reply = {
                    unsafe {
                        xcb::ffi::xproto::xcb_get_selection_owner_reply(
                            shared.conn.get_raw_conn(),
                            cookie.cookie,
                            std::ptr::null_mut(),
                        )
                    }
                };
                if reply != std::ptr::null_mut() {
                    unsafe {
                        x11_clipboard_manager = (*reply).owner;
                        libc::free(reply as *mut _);
                    }
                }
            }
            if x11_clipboard_manager != 0 {
                let atoms;
                let selection;
                {
                    let mut shared = SHARED_STATE.lock().unwrap();
                    atoms = vec![shared.get_atom_by_id(SAVE_TARGETS)];
                    selection = shared.get_atom_by_id(CLIPBOARD_MANAGER);
                } // let go of the shared state lock before invoking `get_data_from_selection_owner`

                // Start the SAVE_TARGETS mechanism so the X11
                // CLIPBOARD_MANAGER will save our clipboard data
                // from now on.
                get_data_from_selection_owner(&atoms, Some(Arc::new(|| true)), selection);
            }
        }

        if self.window != 0 {
            let con = SHARED_STATE.lock().unwrap();
            unsafe { xcb::ffi::xproto::xcb_destroy_window(con.conn.get_raw_conn(), self.window) };
            con.conn.flush();
        }

        if let Some(handle) = self.thread_handle.take() {
            handle.join().ok();
        }

        // This is not needed because the connection is automatically disconnected when droped
        // if (m_connection)
        //     xcb_disconnect(m_connection);
    }
}

fn process_x11_events() {
    use xcb::ffi::xproto::{
        XCB_DESTROY_NOTIFY, XCB_PROPERTY_NOTIFY, XCB_SELECTION_CLEAR, XCB_SELECTION_REQUEST,
    };

    let mut stop = false;
    let mut event;
    while !stop {
        event = {
            let maybe_event = SHARED_STATE.lock().unwrap().conn.wait_for_event();
            if let Some(e) = maybe_event {
                e
            } else {
                break;
            }
        };
        if event.ptr == std::ptr::null_mut() {
            break;
        }
        let resp_type = unsafe { (*event.ptr).response_type & !0x80 };
        match resp_type {
            XCB_DESTROY_NOTIFY => {
                stop = true;
            }

            // Someone else has new content in the clipboard, so is
            // notifying us that we should delete our data now.
            XCB_SELECTION_CLEAR => {
                handle_selection_clear_event(event.ptr as *mut xcb_selection_clear_event_t);
            }

            // Someone is requesting the clipboard content from us.
            XCB_SELECTION_REQUEST => {
                handle_selection_request_event(event.ptr as *mut xcb_selection_request_event_t);
            }

            // We've requested the clipboard content and this is the
            // answer.
            XCB_SELECTION_NOTIFY => {
                handle_selection_notify_event(event.ptr as *mut xcb_selection_notify_event_t);
            }
            XCB_PROPERTY_NOTIFY => {
                handle_property_notify_event(event.ptr as *mut xcb_property_notify_event_t);
            }
            _ => {}
        }
        unsafe {
            libc::free(event.ptr as *mut _);
        }
    }
}

fn handle_selection_clear_event(event: *mut xcb_selection_clear_event_t) {
    let selection = unsafe { (*event).selection };
    let clipboard_atom = {
        let mut shared = SHARED_STATE.lock().unwrap();
        shared.get_atom_by_id(CLIPBOARD)
    };
    if selection == clipboard_atom {
        if let Some(manager) = &*MANAGER {
            manager.lock().unwrap().clear_data();
        }
    }
}

fn handle_selection_request_event(event: *mut xcb_selection_request_event_t) {
    let manager_mutex;
    if let Some(m) = &*MANAGER {
        manager_mutex = m;
    } else {
        return;
    };
    let target;
    let requestor;
    let property;
    let time;
    let selection;
    unsafe {
        target = (*event).target;
        requestor = (*event).requestor;
        property = (*event).property;
        time = (*event).time;
        selection = (*event).selection;
    }
    let targets_atom;
    let save_targets_atom;
    let multiple_atom;
    let atom_atom;
    {
        let mut shared = SHARED_STATE.lock().unwrap();
        targets_atom = shared.get_atom_by_id(TARGETS);
        save_targets_atom = shared.get_atom_by_id(SAVE_TARGETS);
        multiple_atom = shared.get_atom_by_id(MULTIPLE);
        atom_atom = shared.get_atom_by_id(ATOM);
    }
    if target == targets_atom {
        let mut targets = Atoms::with_capacity(4);
        targets.push(targets_atom);
        targets.push(save_targets_atom);
        targets.push(multiple_atom);
        let manager = manager_mutex.lock().unwrap();
        for atom in manager.data.keys() {
            targets.push(*atom);
        }

        let shared = SHARED_STATE.lock().unwrap();
        // Set the "property" of "requestor" with the clipboard
        // formats ("targets", atoms) that we provide.
        unsafe {
            xcb_change_property(
                shared.conn.get_raw_conn(),
                XCB_PROP_MODE_REPLACE as u8,
                requestor,
                property,
                atom_atom,
                8 * std::mem::size_of::<xcb_atom_t>() as u8,
                targets.len() as u32,
                targets.as_ptr() as *const _,
            )
        };
    } else if target == save_targets_atom {
        // Do nothing
    } else if target == multiple_atom {
        let mut manager = manager_mutex.lock().unwrap();
        let reply = {
            let mut shared = SHARED_STATE.lock().unwrap();
            let atom_pair_atom = shared.get_atom_by_id(ATOM_PAIR);
            get_and_delete_property(&mut shared.conn, requestor, property, atom_pair_atom, false)
        };
        if reply != std::ptr::null_mut() {
            let mut ptr: *mut xcb_atom_t =
                unsafe { xcb_get_property_value(reply) } as *mut xcb_atom_t;
            let end = unsafe {
                ptr.offset(
                    xcb_get_property_value_length(reply) as isize
                        / std::mem::size_of::<xcb_atom_t>() as isize,
                )
            };
            while ptr < end {
                let target;
                let property;
                unsafe {
                    target = *ptr;
                    ptr = ptr.offset(1);
                    property = *ptr;
                    ptr = ptr.offset(1);
                }
                if !manager
                    .set_requestor_property_with_clipboard_content(requestor, property, target)
                {
                    let shared = SHARED_STATE.lock().unwrap();
                    unsafe {
                        xcb_change_property(
                            shared.conn.get_raw_conn(),
                            XCB_PROP_MODE_REPLACE as u8,
                            requestor,
                            property,
                            XCB_ATOM_NONE,
                            0,
                            0,
                            std::ptr::null(),
                        )
                    };
                }
            }
            unsafe {
                libc::free(reply as *mut _);
            }
        }
    } else {
        let mut manager = manager_mutex.lock().unwrap();
        if !manager.set_requestor_property_with_clipboard_content(requestor, property, target) {
            return;
        }
    }

    // Notify the "requestor" that we've already updated the property.
    let notify = xcb_selection_notify_event_t {
        response_type: XCB_SELECTION_NOTIFY,
        pad0: 0,
        sequence: 0,
        time: time,
        requestor: requestor,
        selection: selection,
        target: target,
        property: property,
    };
    let shared = SHARED_STATE.lock().unwrap();
    unsafe {
        xcb_send_event(
            shared.conn.get_raw_conn(),
            0,
            requestor,
            XCB_EVENT_MASK_NO_EVENT,
            &notify as *const _ as *const _,
        )
    };
    shared.conn.flush();
}

fn handle_selection_notify_event(event: *mut xcb_selection_notify_event_t) {
    let manager_mutex;
    if let Some(m) = &*MANAGER {
        manager_mutex = m;
    } else {
        return;
    };
    let target;
    let requestor;
    let property;
    unsafe {
        target = (*event).target;
        requestor = (*event).requestor;
        property = (*event).property;
    }
    let mut shared = SHARED_STATE.lock().unwrap();
    let mut manager = manager_mutex.lock().unwrap();
    assert_eq!(requestor, manager.window);

    if target == shared.get_atom_by_id(TARGETS) {
        manager.target_atom = shared.get_atom_by_id(ATOM);
    } else {
        manager.target_atom = target;
    }

    let mut reply = get_and_delete_property(
        &mut shared.conn,
        requestor,
        property,
        manager.target_atom,
        true,
    );
    if reply != std::ptr::null_mut() {
        let reply_type = unsafe { (*reply).type_ };
        // In this case, We're going to receive the clipboard content in
        // chunks of data with several PropertyNotify events.
        let incr_atom = shared.get_atom_by_id(INCR);
        if reply_type == incr_atom {
            unsafe {
                libc::free(reply as *mut _);
            }
            reply = get_and_delete_property(&mut shared.conn, requestor, property, incr_atom, true);
            if reply != std::ptr::null_mut() {
                if unsafe { xcb_get_property_value_length(reply) } == 4 {
                    let n = unsafe { *(xcb_get_property_value(reply) as *mut u32) };
                    manager.reply_data = Some(vec![0u8; n as usize]);
                    manager.reply_offset = 0;
                    manager.incr_process = true;
                    manager.incr_received = true;
                }
                unsafe {
                    libc::free(reply as *mut _);
                }
            }
        } else {
            // Simple case, the whole clipboard content in just one reply
            // (without the INCR method).
            manager.reply_data = None;
            manager.reply_offset = 0;
            manager.copy_reply_data(reply);
            manager.call_callback(reply);

            unsafe {
                libc::free(reply as *mut _);
            }
        }
    }
}

fn handle_property_notify_event(event: *mut xcb_property_notify_event_t) {
    let manager_mutex;
    if let Some(m) = &*MANAGER {
        manager_mutex = m;
    } else {
        return;
    };
    let state;
    let atom;
    let window;
    unsafe {
        state = (*event).state as u32;
        atom = (*event).atom;
        window = (*event).window;
    }
    let mut manager = manager_mutex.lock().unwrap();
    let mut shared = SHARED_STATE.lock().unwrap();
    if manager.incr_process
        && state == XCB_PROPERTY_NEW_VALUE
        && atom == shared.get_atom_by_id(CLIPBOARD)
    {
        let reply =
            get_and_delete_property(&mut shared.conn, window, atom, manager.target_atom, true);
        if reply != std::ptr::null_mut() {
            manager.incr_received = true;

            // When the length is 0 it means that the content was
            // completely sent by the selection owner.
            if unsafe { xcb_get_property_value_length(reply) } > 0 {
                manager.copy_reply_data(reply);
            } else {
                // Now that m_reply_data has the complete clipboard content,
                // we can call the m_callback.
                manager.call_callback(reply);
                manager.incr_process = false;
            }
            unsafe {
                libc::free(reply as *mut _);
            }
        }
    }
}

fn get_and_delete_property(
    conn: &mut xcb::base::Connection,
    window: xproto::Window,
    property: xproto::Atom,
    atom: xproto::Atom,
    delete_prop: bool,
) -> *mut xcb_get_property_reply_t {
    let cookie = unsafe {
        xcb_get_property(
            conn.get_raw_conn(),
            if delete_prop { 1 } else { 0 },
            window,
            property,
            atom,
            0,
            0x1fffffff, // 0x1fffffff = INT32_MAX / 4
        )
    };
    let mut err = std::ptr::null_mut();
    let reply = unsafe { xcb_get_property_reply(conn.get_raw_conn(), cookie, &mut err as *mut _) };
    if err != std::ptr::null_mut() {
        // TODO report error
        unsafe {
            libc::free(err as *mut _);
        }
    }
    reply
}

fn get_data_from_selection_owner(
    atoms: &Atoms,
    callback: NotifyCallback,
    mut selection: xproto::Atom,
) -> bool {
    // Wait a response for 100 milliseconds
    const CV_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

    let manager_mutex;
    if let Some(m) = &*MANAGER {
        manager_mutex = m;
    } else {
        return false;
    };
    if selection == 0 {
        let mut shared = SHARED_STATE.lock().unwrap();
        selection = shared.get_atom_by_id(CLIPBOARD);
    }
    let mut manager = manager_mutex.lock().unwrap();
    manager.callback = callback;

    // Clear data if we are not the selection owner.
    if manager.window != get_x11_selection_owner() {
        manager.data.clear();
    }

    // Ask to the selection owner for its content on each known
    // text format/atom.
    for atom in atoms.iter() {
        {
            let mut shared = SHARED_STATE.lock().unwrap();
            let clipboard_atom = shared.get_atom_by_id(CLIPBOARD);
            xproto::convert_selection(
                &shared.conn,
                manager.window,
                selection,
                *atom,
                clipboard_atom,
                xcb::base::CURRENT_TIME,
            );
            shared.conn.flush();
        }

        // We use the "m_incr_received" to wait several timeouts in case
        // that we've received the INCR SelectionNotify or
        // PropertyNotify events.
        'incr_loop: loop {
            manager.incr_received = false;
            match CONDVAR.wait_timeout(manager, CV_TIMEOUT) {
                Ok((guard, status)) => {
                    manager = guard;
                    if !status.timed_out() {
                        // If the condition variable was notified, it means that the
                        // callback was called correctly.
                        return manager.callback_result;
                    }

                    if !manager.incr_received {
                        break 'incr_loop;
                    }
                }
                Err(err) => {
                    panic!(
                        "A critical error occured while working with the x11 clipboard. {}",
                        err
                    );
                }
            }
        }
    }

    manager.callback = None;
    false
}

fn get_x11_selection_owner() -> xcb::xproto::Window {
    let mut result = 0;

    let mut shared = SHARED_STATE.lock().unwrap();
    let clipboard_atom = shared.get_atom_by_id(CLIPBOARD);
    let cookie = xproto::get_selection_owner(&shared.conn, clipboard_atom);
    let reply = unsafe {
        xcb_get_selection_owner_reply(
            shared.conn.get_raw_conn(),
            cookie.cookie,
            std::ptr::null_mut(),
        )
    };
    if reply != std::ptr::null_mut() {
        result = unsafe { (*reply).owner };
        unsafe {
            libc::free(reply as *mut _);
        }
    }

    result
}

pub struct X11ClipboardContext {}

impl ClipboardProvider for X11ClipboardContext {
    fn new() -> Result<Self, Box<dyn Error>> {
        Ok(X11ClipboardContext {})
    }

    fn get_text(&mut self) -> Result<String, Box<dyn Error>> {
        //let manager = locking::lock_manager()?;
        //manager.get_atom_by_id(CLIPBOARD);
        todo!()
        // if let Some(manager) = &*MANAGER {
        //     MANAGER_LOCK.with(|lock| {
        //         let mut lock = lock.borrow_mut();
        //         *lock = Some(manager.lock().unwrap());
        //     });
        //     todo!()
        // } else {
        //     Err("Could not create the clipboard".into())
        // }
    }

    fn set_text(&mut self, text: String) -> Result<(), Box<dyn Error>> {
        todo!()
    }

    fn get_image(&mut self) -> Result<ImageData, Box<dyn Error>> {
        todo!()
    }

    fn set_image(&mut self, data: ImageData) -> Result<(), Box<dyn Error>> {
        todo!()
    }
}
