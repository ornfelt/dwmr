/* See LICENSE file for copyright and license details. */
//! A tiny test client, port of dwm's transient.c: opens a fixed-size
//! floating window and, after the first event and a 5 second delay, a
//! transient window for it.
//!
//! cargo run --example transient

use std::ptr;

use x11::xlib::*;

fn main() {
    // SAFETY: straight-line Xlib usage on a display that is checked for NULL.
    unsafe {
        let d = XOpenDisplay(ptr::null());
        if d.is_null() {
            std::process::exit(1);
        }
        let r = XDefaultRootWindow(d);

        let f = XCreateSimpleWindow(d, r, 100, 100, 400, 400, 0, 0, 0);
        let mut h: XSizeHints = std::mem::zeroed();
        h.min_width = 400;
        h.max_width = 400;
        h.min_height = 400;
        h.max_height = 400;
        h.flags = PMinSize | PMaxSize;
        XSetWMNormalHints(d, f, &mut h);
        XStoreName(d, f, c"floating".as_ptr());
        XMapWindow(d, f);

        let mut t: Window = 0;
        let mut e: XEvent = std::mem::zeroed();
        XSelectInput(d, f, ExposureMask);
        loop {
            XNextEvent(d, &mut e);

            if t == 0 {
                std::thread::sleep(std::time::Duration::from_secs(5));
                t = XCreateSimpleWindow(d, r, 50, 50, 100, 100, 0, 0, 0);
                XSetTransientForHint(d, t, f);
                XStoreName(d, t, c"transient".as_ptr());
                XMapWindow(d, t);
                XSelectInput(d, t, ExposureMask);
            }
        }
    }
}
