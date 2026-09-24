/* See LICENSE file for copyright and license details. */
//! The handful of fontconfig entry points drw.rs needs. The `x11` crate only
//! declares the opaque `FcPattern`/`FcCharSet` types, so the functions are
//! declared here and linked against libfontconfig directly.

#![allow(non_upper_case_globals)]

use std::os::raw::{c_char, c_int};

pub use x11::xft::{FcChar32, FcCharSet, FcPattern};

pub type FcBool = c_int;
pub type FcChar8 = u8;
/// `FcMatchKind`; only `FcMatchPattern` is used.
pub type FcMatchKind = c_int;
pub enum FcConfig {}

pub const FcTrue: FcBool = 1;
pub const FcMatchPattern: FcMatchKind = 0;

/// `FC_CHARSET` ("charset"), NUL terminated for FFI.
pub const FC_CHARSET: *const c_char = c"charset".as_ptr();
/// `FC_SCALABLE` ("scalable"), NUL terminated for FFI.
pub const FC_SCALABLE: *const c_char = c"scalable".as_ptr();

#[link(name = "fontconfig")]
extern "C" {
    pub fn FcNameParse(name: *const FcChar8) -> *mut FcPattern;
    pub fn FcPatternDestroy(p: *mut FcPattern);
    pub fn FcPatternDuplicate(p: *const FcPattern) -> *mut FcPattern;
    pub fn FcPatternAddCharSet(p: *mut FcPattern, object: *const c_char, c: *const FcCharSet) -> FcBool;
    pub fn FcPatternAddBool(p: *mut FcPattern, object: *const c_char, b: FcBool) -> FcBool;
    pub fn FcConfigSubstitute(config: *mut FcConfig, p: *mut FcPattern, kind: FcMatchKind) -> FcBool;
    pub fn FcDefaultSubstitute(pattern: *mut FcPattern);
    pub fn FcCharSetCreate() -> *mut FcCharSet;
    pub fn FcCharSetAddChar(fcs: *mut FcCharSet, ucs4: FcChar32) -> FcBool;
    pub fn FcCharSetDestroy(fcs: *mut FcCharSet);
}
