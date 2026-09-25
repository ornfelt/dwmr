/* See LICENSE file for copyright and license details. */
//! Drawable abstraction: a port of dwm's drw.c / drw.h.
//!
//! `Drw` owns an off-screen pixmap and a GC; the bar is rendered into the
//! pixmap and then copied onto the bar window with [`Drw::map`]. Fonts are
//! kept in a `Vec` (dwm uses a linked list); index 0 is the primary font and
//! fallback fonts found through fontconfig are appended at the end.

use std::ffi::CString;
use std::mem;
use std::os::raw::{c_int, c_uint};
use std::ptr;

use x11::xft::*;
use x11::xlib::*;
use x11::xrender::{XGlyphInfo, XRenderColor};

use crate::fontconfig::*;
use crate::util::die;

const UTF_INVALID: u32 = 0xFFFD;

/// Decode one UTF-8 sequence at the start of `s`.
///
/// Returns `(len, codepoint, err)`: the number of bytes consumed, the decoded
/// code point (`UTF_INVALID` on error) and whether the sequence was invalid.
fn utf8decode(s: &[u8]) -> (usize, u32, bool) {
    static LENS: [u8; 32] = [
        /* 0XXXX */ 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        /* 10XXX */ 0, 0, 0, 0, 0, 0, 0, 0, /* invalid */
        /* 110XX */ 2, 2, 2, 2,
        /* 1110X */ 3, 3,
        /* 11110 */ 4,
        /* 11111 */ 0, /* invalid */
    ];
    static LEADING_MASK: [u8; 4] = [0x7F, 0x1F, 0x0F, 0x07];
    static OVERLONG: [u32; 4] = [0x0, 0x80, 0x0800, 0x10000];

    let Some(&first) = s.first() else {
        return (0, UTF_INVALID, true);
    };
    let len = LENS[(first >> 3) as usize] as usize;
    if len == 0 {
        return (1, UTF_INVALID, true);
    }

    let mut cp = (first & LEADING_MASK[len - 1]) as u32;
    for i in 1..len {
        match s.get(i) {
            Some(&b) if b != 0 && (b & 0xC0) == 0x80 => cp = (cp << 6) | (b & 0x3F) as u32,
            _ => return (i, UTF_INVALID, true),
        }
    }
    /* out of range, surrogate, overlong encoding */
    if cp > 0x10FFFF || (cp >> 11) == 0x1B || cp < OVERLONG[len - 1] {
        return (len, UTF_INVALID, true);
    }

    (len, cp, false)
}

/// A cursor handle (`Cur`).
#[derive(Default)]
pub struct Cur {
    pub cursor: Cursor,
}

/// A loaded font (`Fnt`).
pub struct Fnt {
    dpy: *mut Display,
    pub h: u32,
    pub xfont: *mut XftFont,
    pub pattern: *mut FcPattern,
}

/* Clr scheme index */
pub const COL_FG: usize = 0;
pub const COL_BG: usize = 1;
pub const COL_BORDER: usize = 2;
pub type Clr = XftColor;
/// A `Clr` that was never allocated (all zero), the value before setup().
pub const CLR_NONE: Clr = Clr { pixel: 0, color: XRenderColor { red: 0, green: 0, blue: 0, alpha: 0 } };

pub struct Drw {
    pub w: u32,
    pub h: u32,
    pub dpy: *mut Display,
    pub screen: c_int,
    pub root: Window,
    pub drawable: Drawable,
    pub gc: GC,
    scheme: Vec<Clr>,
    pub fonts: Vec<Fnt>,
    /* keep track of a couple codepoints for which we have no match
     * (these are function-local statics in drw.c) */
    nomatches: [u32; 128],
    ellipsis_width: u32,
    invalid_width: u32,
}

const INVALID: &str = "\u{FFFD}";

impl Drw {
    /* Drawable abstraction */
    pub fn create(dpy: *mut Display, screen: c_int, root: Window, w: u32, h: u32) -> Drw {
        // SAFETY: dpy is an open display and root a window on `screen`.
        let (drawable, gc) = unsafe {
            let drawable = XCreatePixmap(dpy, root, w, h, XDefaultDepth(dpy, screen) as c_uint);
            let gc = XCreateGC(dpy, root, 0, ptr::null_mut());
            XSetLineAttributes(dpy, gc, 1, LineSolid, CapButt, JoinMiter);
            (drawable, gc)
        };
        Drw {
            w,
            h,
            dpy,
            screen,
            root,
            drawable,
            gc,
            scheme: Vec::new(),
            fonts: Vec::new(),
            nomatches: [0; 128],
            ellipsis_width: 0,
            invalid_width: 0,
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.w = w;
        self.h = h;
        // SAFETY: the pixmap belongs to this Drw and is replaced atomically.
        unsafe {
            if self.drawable != 0 {
                XFreePixmap(self.dpy, self.drawable);
            }
            self.drawable = XCreatePixmap(self.dpy, self.root, w, h, XDefaultDepth(self.dpy, self.screen) as c_uint);
        }
    }

    /// Release the X resources (`drw_free`). Must be called before the
    /// display is closed; there is intentionally no `Drop` impl.
    pub fn free(&mut self) {
        // SAFETY: handles are valid until freed here, then nulled out.
        unsafe {
            if self.drawable != 0 {
                XFreePixmap(self.dpy, self.drawable);
                self.drawable = 0;
            }
            if !self.gc.is_null() {
                XFreeGC(self.dpy, self.gc);
                self.gc = ptr::null_mut();
            }
        }
        Self::fontset_free(&mut self.fonts);
    }

    /* This function is an implementation detail. Library users should use
     * fontset_create instead. */
    fn xfont_create(&self, fontname: Option<&str>, fontpattern: *mut FcPattern) -> Option<Fnt> {
        let xfont;
        let mut pattern: *mut FcPattern = ptr::null_mut();

        if let Some(fontname) = fontname {
            let Ok(cname) = CString::new(fontname) else {
                eprintln!("error, cannot load font from name: '{}'", fontname);
                return None;
            };
            /* Using the pattern found at font->xfont->pattern does not yield the
             * same substitution results as using the pattern returned by
             * FcNameParse; using the latter results in the desired fallback
             * behaviour whereas the former just results in missing-character
             * rectangles being drawn, at least with some fonts. */
            // SAFETY: cname is a valid NUL terminated string.
            unsafe {
                xfont = XftFontOpenName(self.dpy, self.screen, cname.as_ptr());
                if xfont.is_null() {
                    eprintln!("error, cannot load font from name: '{}'", fontname);
                    return None;
                }
                pattern = FcNameParse(cname.as_ptr() as *const FcChar8);
                if pattern.is_null() {
                    eprintln!("error, cannot parse font name to pattern: '{}'", fontname);
                    XftFontClose(self.dpy, xfont);
                    return None;
                }
            }
        } else if !fontpattern.is_null() {
            // SAFETY: fontpattern is a live pattern; Xft takes ownership on success.
            unsafe {
                xfont = XftFontOpenPattern(self.dpy, fontpattern);
            }
            if xfont.is_null() {
                eprintln!("error, cannot load font from pattern.");
                return None;
            }
        } else {
            die("no font specified.");
        }

        // SAFETY: xfont is non-null and points to an XftFont owned by Xft.
        let h = unsafe { ((*xfont).ascent + (*xfont).descent).max(0) as u32 };
        Some(Fnt { dpy: self.dpy, h, xfont, pattern })
    }

    fn xfont_free(font: Fnt) {
        // SAFETY: both handles were created by xfont_create and are freed once.
        unsafe {
            if !font.pattern.is_null() {
                FcPatternDestroy(font.pattern);
            }
            XftFontClose(font.dpy, font.xfont);
        }
    }

    /* Fnt abstraction */
    /// Load `fonts` in order, skipping the ones that fail, and make them the
    /// current font set (the previous one is not freed, as in dwm; take it
    /// out with [`Drw::setfontset`] first to keep it). Returns whether at
    /// least one font was loaded.
    pub fn fontset_create(&mut self, fonts: &[String]) -> bool {
        let mut ret = Vec::with_capacity(fonts.len());
        for name in fonts {
            if let Some(cur) = self.xfont_create(Some(name), ptr::null_mut()) {
                ret.push(cur);
            }
        }
        self.fonts = ret;
        !self.fonts.is_empty()
    }

    /// Free a font set (`drw_fontset_free(Fnt *)`), leaving it empty.
    pub fn fontset_free(set: &mut Vec<Fnt>) {
        for font in set.drain(..) {
            Self::xfont_free(font);
        }
    }

    /// Text extents of `text` in `font`: returns `(width, height)`.
    pub fn font_getexts(font: &Fnt, text: &[u8]) -> (u32, u32) {
        let mut ext = XGlyphInfo { width: 0, height: 0, x: 0, y: 0, xOff: 0, yOff: 0 };
        let len = c_int::try_from(text.len()).unwrap_or(c_int::MAX);
        // SAFETY: font.xfont is valid and text/len describe a readable buffer.
        unsafe {
            XftTextExtentsUtf8(font.dpy, font.xfont, text.as_ptr(), len, &mut ext);
        }
        (ext.xOff.max(0) as u32, font.h)
    }

    /* Colorscheme abstraction */
    pub fn clr_create(&self, clrname: &str) -> Clr {
        match self.clr_alloc(clrname) {
            Some(clr) => clr,
            None => die(&format!("error, cannot allocate color '{}'", clrname)),
        }
    }

    /// `drw_clr_create` without the die(): `None` when the color cannot be
    /// allocated, for color names that come from the status text at runtime.
    pub fn clr_alloc(&self, clrname: &str) -> Option<Clr> {
        let cname = CString::new(clrname).ok()?;
        let mut dest = CLR_NONE;
        // SAFETY: valid display/visual/colormap and a NUL terminated name.
        let ok = unsafe {
            XftColorAllocName(
                self.dpy,
                XDefaultVisual(self.dpy, self.screen),
                XDefaultColormap(self.dpy, self.screen),
                cname.as_ptr(),
                &mut dest,
            )
        };
        if ok == 0 {
            return None;
        }
        /* an opaque alpha byte, so the borders of 32-bit (ARGB) windows do not
         * turn transparent under picom (not the alpha patch: no ARGB visual) */
        dest.pixel |= 0xff << 24;
        Some(dest)
    }

    /// Create a color scheme. Needs at least two colors; returns an empty
    /// scheme otherwise (dwm returns NULL).
    pub fn scm_create(&self, clrnames: &[String]) -> Vec<Clr> {
        if clrnames.len() < 2 {
            return Vec::new();
        }
        clrnames.iter().map(|name| self.clr_create(name)).collect()
    }

    pub fn clr_free(&self, c: &mut Clr) {
        // SAFETY: c was allocated by XftColorAllocName on this display.
        unsafe {
            XftColorFree(self.dpy, XDefaultVisual(self.dpy, self.screen), XDefaultColormap(self.dpy, self.screen), c);
        }
    }

    pub fn scm_free(&self, scm: &mut Vec<Clr>) {
        for c in scm.iter_mut() {
            self.clr_free(c);
        }
        scm.clear();
    }

    /* Cursor abstraction */
    pub fn cur_create(&self, shape: c_uint) -> Cur {
        // SAFETY: XCreateFontCursor accepts any cursorfont shape.
        Cur { cursor: unsafe { XCreateFontCursor(self.dpy, shape) } }
    }

    pub fn cur_free(&self, cursor: &mut Cur) {
        if cursor.cursor == 0 {
            return;
        }
        // SAFETY: the cursor was created by cur_create and is freed once.
        unsafe { XFreeCursor(self.dpy, cursor.cursor) };
        cursor.cursor = 0;
    }

    /* Drawing context manipulation */
    /// Make `set` the current font set. dwm only repoints `drw->fonts`; here
    /// the sets are swapped, so `set` holds the previous one afterwards and
    /// the caller keeps ownership of both.
    pub fn setfontset(&mut self, set: &mut Vec<Fnt>) {
        mem::swap(&mut self.fonts, set);
    }

    /// Select the color scheme used by `rect` and `text` (the colors are
    /// copied, so the caller keeps ownership of the scheme).
    pub fn setscheme(&mut self, scm: &[Clr]) {
        self.scheme.clear();
        self.scheme.extend_from_slice(scm);
    }

    /// `drw->scheme[ColFg] = clr`: change the foreground of the current
    /// scheme. Only the copy made by [`Drw::setscheme`] changes.
    pub fn setfg(&mut self, clr: Clr) {
        if let Some(fg) = self.scheme.get_mut(COL_FG) {
            *fg = clr;
        }
    }

    /* Drawing functions */
    pub fn rect(&mut self, x: i32, y: i32, w: u32, h: u32, filled: bool, invert: bool) {
        if self.scheme.len() < 2 {
            return;
        }
        // SAFETY: gc/drawable are valid for the lifetime of this Drw.
        unsafe {
            XSetForeground(self.dpy, self.gc, if invert { self.scheme[COL_BG].pixel } else { self.scheme[COL_FG].pixel });
            if filled {
                XFillRectangle(self.dpy, self.drawable, self.gc, x, y, w, h);
            } else {
                XDrawRectangle(self.dpy, self.drawable, self.gc, x, y, w.wrapping_sub(1), h.wrapping_sub(1));
            }
        }
    }

    /// Draw `text` into the rectangle (x, y, w, h) with `lpad` pixels of left
    /// padding, eliding with "..." when it does not fit. Returns the x
    /// coordinate right after the drawn area.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, x: i32, y: i32, w: u32, h: u32, lpad: u32, text: &str, invert: bool) -> i32 {
        self.text_impl(x, y, w, h, lpad, text, invert as u32)
    }

    /* In drw.c the `invert` argument doubles as the clamp width when called
     * with x = y = w = h = 0 from drw_fontset_getwidth_clamp(); that hack is
     * kept here, behind the bool wrapper above. */
    #[allow(clippy::too_many_arguments)]
    fn text_impl(&mut self, mut x: i32, y: i32, mut w: u32, h: u32, lpad: u32, text: &str, invert: u32) -> i32 {
        let mut ellipsis_x = 0;
        let mut ellipsis_w = 0;
        let mut d: *mut XftDraw = ptr::null_mut();
        let render = x != 0 || y != 0 || w != 0 || h != 0;
        let mut utf8codepoint = 0u32;
        let mut charexists = false;
        let mut overflow = false;

        if (render && (self.scheme.len() < 2 || w == 0)) || self.fonts.is_empty() {
            return 0;
        }

        if !render {
            w = if invert != 0 { invert } else { !invert };
        } else {
            // SAFETY: gc/drawable/visual/colormap are all valid on self.dpy.
            unsafe {
                XSetForeground(self.dpy, self.gc, self.scheme[if invert != 0 { COL_FG } else { COL_BG }].pixel);
                XFillRectangle(self.dpy, self.drawable, self.gc, x, y, w, h);
                if w < lpad {
                    return x + w as i32;
                }
                d = XftDrawCreate(
                    self.dpy,
                    self.drawable,
                    XDefaultVisual(self.dpy, self.screen),
                    XDefaultColormap(self.dpy, self.screen),
                );
            }
            x += lpad as i32;
            w -= lpad;
        }

        let bytes = text.as_bytes();
        let mut pos = 0usize; /* `text` pointer in drw.c */
        let mut usedfont = 0usize;
        if self.ellipsis_width == 0 && render {
            self.ellipsis_width = self.fontset_getwidth("...");
        }
        if self.invalid_width == 0 && render {
            self.invalid_width = self.fontset_getwidth(INVALID);
        }
        loop {
            let mut ew = 0u32;
            let mut ellipsis_len = 0usize;
            let mut utf8err = false;
            let mut utf8strlen = 0usize;
            let utf8str = pos;
            let mut nextfont: Option<usize> = None;
            while pos < bytes.len() {
                let (utf8charlen, cp, err) = utf8decode(&bytes[pos..]);
                utf8codepoint = cp;
                utf8err = err;
                for curfont in 0..self.fonts.len() {
                    // SAFETY: every xfont in self.fonts is a live XftFont.
                    charexists = charexists || unsafe { XftCharExists(self.dpy, self.fonts[curfont].xfont, utf8codepoint) } != 0;
                    if charexists {
                        let (tmpw, _) = Self::font_getexts(&self.fonts[curfont], &bytes[pos..pos + utf8charlen]);
                        if ew + self.ellipsis_width <= w {
                            /* keep track where the ellipsis still fits */
                            ellipsis_x = x + ew as i32;
                            ellipsis_w = w - ew;
                            ellipsis_len = utf8strlen;
                        }

                        if ew + tmpw > w {
                            overflow = true;
                            /* called from fontset_getwidth_clamp():
                             * it wants the width AFTER the overflow
                             */
                            if !render {
                                x += tmpw as i32;
                            } else {
                                utf8strlen = ellipsis_len;
                            }
                        } else if curfont == usedfont {
                            pos += utf8charlen;
                            utf8strlen += if utf8err { 0 } else { utf8charlen };
                            ew += if utf8err { 0 } else { tmpw };
                        } else {
                            nextfont = Some(curfont);
                        }
                        break;
                    }
                }

                if overflow || !charexists || nextfont.is_some() || utf8err {
                    break;
                } else {
                    charexists = false;
                }
            }

            if utf8strlen > 0 {
                if render {
                    let font = &self.fonts[usedfont];
                    // SAFETY: d, the font and the scheme color are valid; the
                    // byte range lies inside `bytes`.
                    unsafe {
                        let ty = y + (h as i32 - font.h as i32) / 2 + (*font.xfont).ascent;
                        XftDrawStringUtf8(
                            d,
                            &self.scheme[if invert != 0 { COL_BG } else { COL_FG }],
                            font.xfont,
                            x,
                            ty,
                            bytes[utf8str..].as_ptr(),
                            c_int::try_from(utf8strlen).unwrap_or(c_int::MAX),
                        );
                    }
                }
                x += ew as i32;
                w = w.wrapping_sub(ew);
            }
            if utf8err && (!render || self.invalid_width < w) {
                if render {
                    self.text_impl(x, y, w, h, 0, INVALID, invert);
                }
                x += self.invalid_width as i32;
                w = w.wrapping_sub(self.invalid_width);
            }
            if render && overflow {
                self.text_impl(ellipsis_x, y, ellipsis_w, h, 0, "...", invert);
            }

            if pos >= bytes.len() || overflow {
                break;
            } else if let Some(nf) = nextfont {
                charexists = false;
                usedfont = nf;
            } else {
                /* Regardless of whether or not a fallback font is found, the
                 * character must be drawn. */
                charexists = true;

                let mut hash = utf8codepoint;
                hash = ((hash >> 16) ^ hash).wrapping_mul(0x21F0AAAD);
                hash = ((hash >> 15) ^ hash).wrapping_mul(0xD35A2D97);
                let h0 = (((hash >> 15) ^ hash) % self.nomatches.len() as u32) as usize;
                let h1 = ((hash >> 17) % self.nomatches.len() as u32) as usize;
                /* avoid expensive XftFontMatch call when we know we won't find a match */
                if self.nomatches[h0] == utf8codepoint || self.nomatches[h1] == utf8codepoint {
                    usedfont = 0; /* no_match */
                    continue;
                }

                if self.fonts[0].pattern.is_null() {
                    /* Refer to the comment in xfont_create for more information. */
                    die("the first font in the cache must be loaded from a font string.");
                }

                // SAFETY: fontconfig objects are created, used and destroyed
                // within this block; the pattern of fonts[0] is non-null.
                let match_ = unsafe {
                    let fccharset = FcCharSetCreate();
                    FcCharSetAddChar(fccharset, utf8codepoint);

                    let fcpattern = FcPatternDuplicate(self.fonts[0].pattern);
                    FcPatternAddCharSet(fcpattern, FC_CHARSET, fccharset);
                    FcPatternAddBool(fcpattern, FC_SCALABLE, FcTrue);

                    FcConfigSubstitute(ptr::null_mut(), fcpattern, FcMatchPattern);
                    FcDefaultSubstitute(fcpattern);
                    let mut result = FcResult::NoMatch;
                    let match_ = XftFontMatch(self.dpy, self.screen, fcpattern, &mut result);

                    FcCharSetDestroy(fccharset);
                    FcPatternDestroy(fcpattern);
                    match_
                };

                if !match_.is_null() {
                    let found = self.xfont_create(None, match_);
                    // SAFETY: the font (if any) is live.
                    let exists = found.as_ref().is_some_and(|f| unsafe { XftCharExists(self.dpy, f.xfont, utf8codepoint) } != 0);
                    match found {
                        Some(font) if exists => {
                            self.fonts.push(font);
                            usedfont = self.fonts.len() - 1;
                        }
                        found => {
                            if let Some(font) = found {
                                Self::xfont_free(font);
                            }
                            let slot = if self.nomatches[h0] != 0 { h1 } else { h0 };
                            self.nomatches[slot] = utf8codepoint;
                            usedfont = 0; /* no_match */
                        }
                    }
                }
            }
        }
        if !d.is_null() {
            // SAFETY: d was created above and is destroyed exactly once.
            unsafe { XftDrawDestroy(d) };
        }

        x + if render { w as i32 } else { 0 }
    }

    /* Map functions */
    pub fn map(&self, win: Window, x: i32, y: i32, w: u32, h: u32) {
        // SAFETY: drawable/gc are valid; win is a window on the same screen.
        unsafe {
            XCopyArea(self.dpy, self.drawable, win, self.gc, x, y, w, h, x, y);
            XSync(self.dpy, False);
        }
    }

    pub fn fontset_getwidth(&mut self, text: &str) -> u32 {
        if self.fonts.is_empty() {
            return 0;
        }
        self.text_impl(0, 0, 0, 0, 0, text, 0).max(0) as u32
    }

    #[allow(dead_code)]
    pub fn fontset_getwidth_clamp(&mut self, text: &str, n: u32) -> u32 {
        let mut tmp = 0;
        if !self.fonts.is_empty() && n != 0 {
            tmp = self.text_impl(0, 0, 0, 0, 0, text, n).max(0) as u32;
        }
        n.min(tmp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8decode_ascii_and_multibyte() {
        assert_eq!(utf8decode(b"a"), (1, 'a' as u32, false));
        assert_eq!(utf8decode("é".as_bytes()), (2, 'é' as u32, false));
        assert_eq!(utf8decode("€".as_bytes()), (3, '€' as u32, false));
        assert_eq!(utf8decode("😀".as_bytes()), (4, '😀' as u32, false));
    }

    #[test]
    fn utf8decode_invalid() {
        /* lone continuation byte */
        assert_eq!(utf8decode(&[0x80]), (1, UTF_INVALID, true));
        /* truncated sequence */
        assert_eq!(utf8decode(&[0xE2, 0x82]), (2, UTF_INVALID, true));
        /* overlong encoding of '/' */
        assert_eq!(utf8decode(&[0xC0, 0xAF]), (2, UTF_INVALID, true));
        /* surrogate */
        assert_eq!(utf8decode(&[0xED, 0xA0, 0x80]), (3, UTF_INVALID, true));
        /* empty */
        assert_eq!(utf8decode(b""), (0, UTF_INVALID, true));
    }
}
