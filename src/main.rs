/* See LICENSE file for copyright and license details.
 *
 * dwmr - dynamic window manager, a Rust port of dwm.
 *
 * To understand everything else, start reading Dwm::run() in dwm.rs.
 */
mod config;
mod drw;
mod dwm;
mod fontconfig;
mod util;
mod xres;

use std::ptr;

use x11::xlib::{XCloseDisplay, XOpenDisplay, XSupportsLocale};

use crate::dwm::Dwm;
use crate::util::die;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 && args[1] == "-v" {
        die(&format!("dwmr-{}", VERSION));
    } else if args.len() != 1 {
        die("usage: dwmr [-v]");
    }
    // SAFETY: plain libc/Xlib calls with valid arguments; no other thread exists yet.
    unsafe {
        if libc::setlocale(libc::LC_CTYPE, c"".as_ptr()).is_null() || XSupportsLocale() == 0 {
            eprintln!("warning: no locale support");
        }
    }
    let config = config::load();
    // SAFETY: XOpenDisplay(NULL) is always sound; the result is checked for NULL.
    let dpy = unsafe { XOpenDisplay(ptr::null()) };
    if dpy.is_null() {
        die("dwmr: cannot open display");
    }
    let mut dwm = Dwm::new(dpy, config);
    dwm.checkotherwm();
    dwm.setup();
    dwm.scan();
    dwm.run();
    dwm.cleanup();
    // SAFETY: dpy is a valid display that nothing uses afterwards.
    unsafe { XCloseDisplay(dpy) };
}
