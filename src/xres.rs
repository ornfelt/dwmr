/* See LICENSE file for copyright and license details. */
//! The X-Resource extension (libXRes) entry points winpid() needs. dwm's
//! swallow patch asks xcb-res for the client's PID; the `x11` crate has no
//! XRes binding, so the equivalent Xlib calls are declared here and linked
//! against libXRes directly.

use std::os::raw::{c_int, c_long, c_uint, c_void};

use x11::xlib::{Bool, Display, Status, XID};

/// `XRES_CLIENT_ID_PID_MASK` (`1 << XRES_CLIENT_ID_PID`), X11/extensions/XRes.h
pub const XRES_CLIENT_ID_PID_MASK: c_uint = 1 << 1;

#[repr(C)]
pub struct XResClientIdSpec {
    pub client: XID,
    pub mask: c_uint,
}

#[repr(C)]
pub struct XResClientIdValue {
    pub spec: XResClientIdSpec,
    pub length: c_long,
    pub value: *mut c_void,
}

#[link(name = "XRes")]
extern "C" {
    pub fn XResQueryExtension(dpy: *mut Display, event_base_return: *mut c_int, error_base_return: *mut c_int) -> Bool;
    pub fn XResQueryClientIds(
        dpy: *mut Display,
        num_specs: c_long,
        client_specs: *mut XResClientIdSpec,
        num_ids: *mut c_long,
        client_ids: *mut *mut XResClientIdValue,
    ) -> Status;
    pub fn XResClientIdsDestroy(num_ids: c_long, client_ids: *mut XResClientIdValue);
    /// Returns -1 if no pid is associated to the value.
    pub fn XResGetClientPid(value: *mut XResClientIdValue) -> libc::pid_t;
}
