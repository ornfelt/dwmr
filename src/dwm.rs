/* See LICENSE file for copyright and license details.
 *
 * dynamic window manager is designed like any other X client as well. It is
 * driven through handling X events. In contrast to other X clients, a window
 * manager selects for SubstructureRedirectMask on the root window, to receive
 * events about window (dis-)appearance. Only one X connection at a time is
 * allowed to select for this event mask.
 *
 * The event handlers of dwm are organized in a match (compiled to a jump
 * table) which is consulted whenever a new event has been fetched. This
 * allows event dispatching in O(1) time.
 *
 * Each child of the root window is called a client, except windows which have
 * set the override_redirect flag. Clients are organized in a linked client
 * list on each monitor, the focus history is remembered through a stack list
 * on each monitor. Each client contains a bit array to indicate the tags of a
 * client.
 *
 * Instead of raw pointers, clients live in a slab (`Dwm::clients`) and
 * monitors in a `Vec` (`Dwm::mons`, in list order); the `next`/`snext`
 * links are indices into the slab. Stale indices can only ever cause a
 * logic error, never a memory error.
 *
 * Keys and tagging rules are organized as arrays and defined in config.rs
 * (defaults) and ~/.config/dwmr/config.toml.
 *
 * To understand everything else, start reading main() in main.rs.
 */
#![allow(non_upper_case_globals)] /* Xlib event/constant names are used as-is */

use std::ffi::CString;
use std::mem;
use std::os::raw::{c_char, c_int, c_long, c_uchar, c_uint, c_ulong};
use std::ptr;
use std::rc::Rc;
use std::sync::OnceLock;

use x11::keysym::XK_Num_Lock;
#[cfg(feature = "xinerama")]
use x11::xinerama::{XineramaIsActive, XineramaQueryScreens, XineramaScreenInfo};
use x11::xlib::*;

use crate::config::{
    Arg, ArrangeFn, Config, Layout, CLK_CLIENT_WIN, CLK_LT_SYMBOL, CLK_ROOT_WIN, CLK_STATUS_TEXT, CLK_TAG_BAR,
    SCHEME_NORM, SCHEME_SEL,
};
use crate::drw::{Clr, Cur, Drw, COL_BORDER};
use crate::util::{die, truncate_utf8};
use crate::VERSION;

/* vanitygaps.c is #included by dwm's config.h; here it is a child module,
 * so its layouts and actions are methods of Dwm like everything else */
#[path = "vanitygaps.rs"]
pub mod vanitygaps;

/* macros */
const BUTTONMASK: c_long = ButtonPressMask | ButtonReleaseMask;
const MOUSEMASK: c_long = BUTTONMASK | PointerMotionMask;

/// `WIDTH(X)`
#[inline]
fn width(c: &Client) -> i32 {
    c.w + 2 * c.bw
}

/// `HEIGHT(X)`
#[inline]
fn height(c: &Client) -> i32 {
    c.h + 2 * c.bw
}

/// `INTERSECT(x,y,w,h,m)`
#[inline]
fn intersect(x: i32, y: i32, w: i32, h: i32, m: &Monitor) -> i32 {
    0.max((x + w).min(m.wx + m.ww) - x.max(m.wx)) * 0.max((y + h).min(m.wy + m.wh) - y.max(m.wy))
}

/// `GETINC(X)` (stacker)
#[inline]
fn getinc(x: i32) -> i32 {
    x - 2000
}

/// `INC(X)` (stacker)
#[inline]
pub const fn inc(x: i32) -> i32 {
    x + 2000
}

/// `ISINC(X)` (stacker)
#[inline]
fn isinc(x: i32) -> bool {
    x > 1000 && x < 3000
}

/// `MOD(N,M)` (stacker): the non-negative remainder; `m` must be positive
#[inline]
fn modulo(n: i32, m: i32) -> i32 {
    n.rem_euclid(m)
}

/* enums */
const CUR_NORMAL: usize = 0;
const CUR_RESIZE: usize = 1;
const CUR_MOVE: usize = 2;
const CUR_LAST: usize = 3; /* cursor */
const NET_SUPPORTED: usize = 0;
const NET_WM_NAME: usize = 1;
const NET_WM_STATE: usize = 2;
const NET_WM_CHECK: usize = 3;
const NET_WM_FULLSCREEN: usize = 4;
const NET_WM_STICKY: usize = 5;
const NET_ACTIVE_WINDOW: usize = 6;
const NET_WM_WINDOW_TYPE: usize = 7;
const NET_WM_WINDOW_TYPE_DIALOG: usize = 8;
const NET_CLIENT_LIST: usize = 9;
const NET_LAST: usize = 10; /* EWMH atoms */
const WM_PROTOCOLS: usize = 0;
const WM_DELETE: usize = 1;
const WM_STATE: usize = 2;
const WM_TAKE_FOCUS: usize = 3;
const WM_LAST: usize = 4; /* default atoms */

/* X request codes (X11/Xproto.h), not exported by the x11 crate */
const X_CONFIGURE_WINDOW: c_uchar = 12;
const X_GRAB_BUTTON: c_uchar = 28;
const X_GRAB_KEY: c_uchar = 33;
const X_SET_INPUT_FOCUS: c_uchar = 42;
const X_COPY_AREA: c_uchar = 62;
const X_POLY_SEGMENT: c_uchar = 66;
const X_POLY_FILL_RECTANGLE: c_uchar = 70;
const X_POLY_TEXT8: c_uchar = 74;
/* cursor shapes (X11/cursorfont.h) */
const XC_FLEUR: c_uint = 52;
const XC_LEFT_PTR: c_uint = 68;
const XC_SIZING: c_uint = 120;
/* ICCCM window states (X11/Xutil.h) */
const WITHDRAWN_STATE: c_long = 0;
const NORMAL_STATE: c_long = 1;
const ICONIC_STATE: c_long = 3;

/* fixed buffer sizes from dwm.c, applied as byte limits */
const NAME_SIZE: usize = 256; /* Client.name, stext */
const LTSYMBOL_SIZE: usize = 16; /* Monitor.ltsymbol */

/// Index of a client in `Dwm::clients`.
pub type ClientId = usize;
/// Index of a monitor in `Dwm::mons`.
pub type MonId = usize;

#[derive(Default)]
pub struct Client {
    pub name: String,
    pub mina: f32,
    pub maxa: f32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /* stored float geometry, used on mode revert */
    pub sfx: i32,
    pub sfy: i32,
    pub sfw: i32,
    pub sfh: i32,
    pub oldx: i32,
    pub oldy: i32,
    pub oldw: i32,
    pub oldh: i32,
    pub basew: i32,
    pub baseh: i32,
    pub incw: i32,
    pub inch: i32,
    pub maxw: i32,
    pub maxh: i32,
    pub minw: i32,
    pub minh: i32,
    pub hintsvalid: bool,
    pub bw: i32,
    pub oldbw: i32,
    pub tags: u32,
    pub isfixed: bool,
    pub isfloating: bool,
    pub isurgent: bool,
    pub neverfocus: bool,
    pub oldstate: bool,
    pub isfullscreen: bool,
    pub issticky: bool,
    pub isbrowser: bool,
    pub next: Option<ClientId>,
    pub snext: Option<ClientId>,
    pub mon: MonId,
    pub win: Window,
}

#[derive(Default)]
pub struct Monitor {
    pub ltsymbol: String,
    pub mfact: f32,
    pub nmaster: i32,
    pub num: i32,
    pub by: i32, /* bar geometry */
    pub mx: i32,
    pub my: i32,
    pub mw: i32,
    pub mh: i32, /* screen size */
    pub wx: i32,
    pub wy: i32,
    pub ww: i32,
    pub wh: i32, /* window area  */
    pub gappih: i32, /* horizontal gap between windows */
    pub gappiv: i32, /* vertical gap between windows */
    pub gappoh: i32, /* horizontal outer gaps */
    pub gappov: i32, /* vertical outer gaps */
    pub seltags: usize,
    pub sellt: usize,
    pub tagset: [u32; 2],
    pub showbar: bool,
    pub topbar: bool,
    pub clients: Option<ClientId>,
    pub sel: Option<ClientId>,
    pub stack: Option<ClientId>,
    pub barwin: Window,
    /// Indices into `Dwm::layouts`.
    pub lt: [usize; 2],
}

/* variables */
const BROKEN: &str = "broken";
/// `int (*)(Display *, XErrorEvent *)`
type XErrorHandler = Option<unsafe extern "C" fn(*mut Display, *mut XErrorEvent) -> c_int>;
/// The Xlib error handler that was installed before ours (`xerrorxlib`).
static XERRORXLIB: OnceLock<XErrorHandler> = OnceLock::new();

/// All of dwm's global state.
pub struct Dwm {
    config: Rc<Config>,
    /// The layouts; `cleanup()` appends an empty one, hence a copy of `config.layouts`.
    layouts: Vec<Layout>,
    stext: String,
    screen: c_int,
    sw: i32,
    sh: i32, /* X display screen geometry width, height */
    bh: i32, /* bar height */
    lrpad: i32, /* sum of left and right padding for text */
    numlockmask: u32,
    wmatom: [Atom; WM_LAST],
    netatom: [Atom; NET_LAST],
    running: bool,
    cursor: [Cur; CUR_LAST],
    scheme: Vec<Vec<Clr>>,
    dpy: *mut Display,
    drw: Drw,
    mons: Vec<Monitor>,
    selmon: MonId,
    root: Window,
    wmcheckwin: Window,
    /// Client slab plus its free list.
    clients: Vec<Client>,
    free_clients: Vec<ClientId>,
    /* vanitygaps */
    enablegaps: bool,
    /// Outer gaps for a lone browser window; toggled by togglebgaps, hence
    /// not read from the (immutable) config after startup.
    browsergaps: bool,
}

/// The event handler table (`handler[LASTEvent]`).
fn handler(ty: c_int) -> Option<fn(&mut Dwm, &XEvent)> {
    match ty {
        ButtonPress => Some(Dwm::buttonpress),
        ClientMessage => Some(Dwm::clientmessage),
        ConfigureRequest => Some(Dwm::configurerequest),
        ConfigureNotify => Some(Dwm::configurenotify),
        DestroyNotify => Some(Dwm::destroynotify),
        Expose => Some(Dwm::expose),
        FocusIn => Some(Dwm::focusin),
        KeyPress => Some(Dwm::keypress),
        MappingNotify => Some(Dwm::mappingnotify),
        MapRequest => Some(Dwm::maprequest),
        PropertyNotify => Some(Dwm::propertynotify),
        UnmapNotify => Some(Dwm::unmapnotify),
        _ => None,
    }
}

#[cfg(feature = "xinerama")]
fn isuniquegeom(unique: &[XineramaScreenInfo], info: &XineramaScreenInfo) -> bool {
    !unique
        .iter()
        .any(|u| u.x_org == info.x_org && u.y_org == info.y_org && u.width == info.width && u.height == info.height)
}

/* function implementations */
impl Dwm {
    /// Bind to an open display. This also performs the "init screen" part of
    /// dwm's setup() because the drawing context needs the screen and root.
    pub fn new(dpy: *mut Display, config: Config) -> Dwm {
        // SAFETY: dpy is an open display.
        let (screen, sw, sh, root) = unsafe {
            let screen = XDefaultScreen(dpy);
            (screen, XDisplayWidth(dpy, screen), XDisplayHeight(dpy, screen), XRootWindow(dpy, screen))
        };
        let drw = Drw::create(dpy, screen, root, sw.max(1) as u32, sh.max(1) as u32);
        Dwm {
            layouts: config.layouts.clone(),
            browsergaps: config.browsergaps,
            config: Rc::new(config),
            stext: String::new(),
            screen,
            sw,
            sh,
            bh: 0,
            lrpad: 0,
            numlockmask: 0,
            wmatom: [0; WM_LAST],
            netatom: [0; NET_LAST],
            running: true,
            cursor: Default::default(),
            scheme: Vec::new(),
            dpy,
            drw,
            mons: Vec::new(),
            selmon: 0,
            root,
            wmcheckwin: 0,
            clients: Vec::new(),
            free_clients: Vec::new(),
            enablegaps: true,
        }
    }

    /* slab helpers (ecalloc/free of a Client) */
    fn alloc_client(&mut self, c: Client) -> ClientId {
        if let Some(id) = self.free_clients.pop() {
            self.clients[id] = c;
            id
        } else {
            self.clients.push(c);
            self.clients.len() - 1
        }
    }

    fn free_client(&mut self, c: ClientId) {
        /* make a stale index harmless: no window, no links */
        self.clients[c] = Client::default();
        self.free_clients.push(c);
    }

    /* small accessors replacing dwm's macros */

    /// `CLEANMASK(mask)`
    #[inline]
    fn cleanmask(&self, mask: u32) -> u32 {
        mask & !(self.numlockmask | LockMask)
            & (ShiftMask | ControlMask | Mod1Mask | Mod2Mask | Mod3Mask | Mod4Mask | Mod5Mask)
    }

    /// `ISVISIBLE(C)`
    #[inline]
    fn isvisible(&self, c: ClientId) -> bool {
        let c = &self.clients[c];
        let m = &self.mons[c.mon];
        c.tags & m.tagset[m.seltags] != 0 || c.issticky
    }

    /// `TAGMASK`
    #[inline]
    /// `NUMTAGS` (scratchpads): the normal tags plus one tag per scratchpad
    fn numtags(&self) -> usize {
        self.config.tags.len() + self.config.scratchpads.len()
    }

    /// `TAGMASK`; the config loader guarantees NUMTAGS <= 31
    fn tagmask(&self) -> u32 {
        (1u32 << self.numtags()) - 1
    }

    /// The mask of the normal tags only, without the scratchpad tags; used
    /// by hide_vacant_tags for "a window on every tag" (dwm compares with
    /// TAGMASK, which with scratchpads would never match).
    fn tagbits(&self) -> u32 {
        (1u32 << self.config.tags.len()) - 1
    }

    /// `SPTAG(i)` (scratchpads); `i` must be a valid scratchpad index
    fn sptag(&self, i: usize) -> u32 {
        (1u32 << self.config.tags.len()) << i
    }

    /// `SPTAGMASK` (scratchpads)
    fn sptagmask(&self) -> u32 {
        ((1u32 << self.config.scratchpads.len()) - 1) << self.config.tags.len()
    }

    /// `TEXTW(X)`; takes the fields explicitly so it can be used while other
    /// fields of `self` are borrowed.
    #[inline]
    fn textw(drw: &mut Drw, lrpad: i32, text: &str) -> i32 {
        drw.fontset_getwidth(text) as i32 + lrpad
    }

    /// `m->lt[m->sellt]->arrange`
    #[inline]
    fn arrange_fn(&self, m: MonId) -> Option<fn(&mut Dwm, MonId)> {
        let mon = &self.mons[m];
        self.layouts.get(mon.lt[mon.sellt]).and_then(|l| l.arrange)
    }

    /// `m->lt[m->sellt]->symbol`
    #[inline]
    fn lt_symbol(&self, m: MonId) -> &str {
        let mon = &self.mons[m];
        self.layouts.get(mon.lt[mon.sellt]).map_or("", |l| l.symbol.as_str())
    }

    fn applyrules(&mut self, c: ClientId) {
        let config = Rc::clone(&self.config);
        let mut ch = XClassHint { res_name: ptr::null_mut(), res_class: ptr::null_mut() };

        /* rule matching */
        self.clients[c].isfloating = false;
        self.clients[c].tags = 0;
        // SAFETY: the returned strings are freed with XFree below and only
        // read in between.
        let (class, instance) = unsafe {
            XGetClassHint(self.dpy, self.clients[c].win, &mut ch);
            let class = if ch.res_class.is_null() {
                BROKEN.to_string()
            } else {
                std::ffi::CStr::from_ptr(ch.res_class).to_string_lossy().into_owned()
            };
            let instance = if ch.res_name.is_null() {
                BROKEN.to_string()
            } else {
                std::ffi::CStr::from_ptr(ch.res_name).to_string_lossy().into_owned()
            };
            if !ch.res_class.is_null() {
                XFree(ch.res_class as *mut _);
            }
            if !ch.res_name.is_null() {
                XFree(ch.res_name as *mut _);
            }
            (class, instance)
        };

        /* firefox, Firefox, firefox-esr, ... (used by getgaps) */
        self.clients[c].isbrowser = class.as_bytes().get(..7).is_some_and(|p| p.eq_ignore_ascii_case(b"firefox"));

        let sptagmask = self.sptagmask();
        for r in &config.rules {
            if r.title.as_ref().is_none_or(|t| self.clients[c].name.contains(t.as_str()))
                && r.class.as_ref().is_none_or(|cl| class.contains(cl.as_str()))
                && r.instance.as_ref().is_none_or(|i| instance.contains(i.as_str()))
            {
                self.clients[c].isfloating = r.isfloating;
                self.clients[c].tags |= r.tags;
                if r.tags & sptagmask != 0 && r.isfloating {
                    let m = &self.mons[self.clients[c].mon];
                    let (wx, wy, ww, wh) = (m.wx, m.wy, m.ww, m.wh);
                    let cl = &mut self.clients[c];
                    cl.x = wx + (ww / 2 - width(cl) / 2);
                    cl.y = wy + (wh / 2 - height(cl) / 2);
                }

                if let Some(m) = self.mons.iter().position(|m| m.num == r.monitor) {
                    self.clients[c].mon = m;
                }
            }
        }
        let tagmask = self.tagmask();
        let mon = &self.mons[self.clients[c].mon];
        let tags = self.clients[c].tags & tagmask;
        self.clients[c].tags = if tags != 0 { tags } else { mon.tagset[mon.seltags] & !sptagmask };
    }

    fn applysizehints(&mut self, c: ClientId, x: &mut i32, y: &mut i32, w: &mut i32, h: &mut i32, interact: bool) -> bool {
        let m = self.clients[c].mon;

        /* set minimum possible */
        *w = 1.max(*w);
        *h = 1.max(*h);
        {
            let cl = &self.clients[c];
            let mon = &self.mons[m];
            if interact {
                if *x > self.sw {
                    *x = self.sw - width(cl);
                }
                if *y > self.sh {
                    *y = self.sh - height(cl);
                }
                if *x + *w + 2 * cl.bw < 0 {
                    *x = 0;
                }
                if *y + *h + 2 * cl.bw < 0 {
                    *y = 0;
                }
            } else {
                if *x >= mon.wx + mon.ww {
                    *x = mon.wx + mon.ww - width(cl);
                }
                if *y >= mon.wy + mon.wh {
                    *y = mon.wy + mon.wh - height(cl);
                }
                if *x + *w + 2 * cl.bw <= mon.wx {
                    *x = mon.wx;
                }
                if *y + *h + 2 * cl.bw <= mon.wy {
                    *y = mon.wy;
                }
            }
        }
        if *h < self.bh {
            *h = self.bh;
        }
        if *w < self.bh {
            *w = self.bh;
        }
        if self.config.resizehints || self.clients[c].isfloating || self.arrange_fn(m).is_none() {
            if !self.clients[c].hintsvalid {
                self.updatesizehints(c);
            }
            let cl = &self.clients[c];
            /* see last two sentences in ICCCM 4.1.2.3 */
            let baseismin = cl.basew == cl.minw && cl.baseh == cl.minh;
            if !baseismin {
                /* temporarily remove base dimensions */
                *w -= cl.basew;
                *h -= cl.baseh;
            }
            /* adjust for aspect limits */
            if cl.mina > 0.0 && cl.maxa > 0.0 {
                if cl.maxa < *w as f32 / *h as f32 {
                    *w = (*h as f32 * cl.maxa + 0.5) as i32;
                } else if cl.mina < *h as f32 / *w as f32 {
                    *h = (*w as f32 * cl.mina + 0.5) as i32;
                }
            }
            if baseismin {
                /* increment calculation requires this */
                *w -= cl.basew;
                *h -= cl.baseh;
            }
            /* adjust for increment value */
            if cl.incw != 0 {
                *w -= w.wrapping_rem(cl.incw);
            }
            if cl.inch != 0 {
                *h -= h.wrapping_rem(cl.inch);
            }
            /* restore base dimensions */
            *w = (*w + cl.basew).max(cl.minw);
            *h = (*h + cl.baseh).max(cl.minh);
            if cl.maxw != 0 {
                *w = (*w).min(cl.maxw);
            }
            if cl.maxh != 0 {
                *h = (*h).min(cl.maxh);
            }
        }
        let cl = &self.clients[c];
        *x != cl.x || *y != cl.y || *w != cl.w || *h != cl.h
    }

    /// `arrange(m)`; `None` arranges all monitors (`arrange(NULL)`).
    fn arrange(&mut self, m: Option<MonId>) {
        match m {
            Some(m) => {
                let stack = self.mons[m].stack;
                self.showhide(stack);
                self.arrangemon(m);
                self.restack(m);
            }
            None => {
                for m in 0..self.mons.len() {
                    let stack = self.mons[m].stack;
                    self.showhide(stack);
                }
                for m in 0..self.mons.len() {
                    self.arrangemon(m);
                }
            }
        }
    }

    fn arrangemon(&mut self, m: MonId) {
        let mut symbol = self.lt_symbol(m).to_string();
        truncate_utf8(&mut symbol, LTSYMBOL_SIZE - 1);
        self.mons[m].ltsymbol = symbol;
        if let Some(arrange) = self.arrange_fn(m) {
            arrange(self, m);
        }
    }

    fn attach(&mut self, c: ClientId) {
        let m = self.clients[c].mon;
        self.clients[c].next = self.mons[m].clients;
        self.mons[m].clients = Some(c);
    }

    fn attachstack(&mut self, c: ClientId) {
        let m = self.clients[c].mon;
        self.clients[c].snext = self.mons[m].stack;
        self.mons[m].stack = Some(c);
    }

    fn buttonpress(&mut self, e: &XEvent) {
        let config = Rc::clone(&self.config);
        let ev: XButtonEvent = e.into();
        let mut arg = Arg::None;

        let mut click = CLK_ROOT_WIN;
        /* focus monitor if necessary */
        let m = self.wintomon(ev.window);
        if m != self.selmon && (config.focusonwheel || (ev.button != Button4 && ev.button != Button5)) {
            let sel = self.mons[self.selmon].sel;
            self.unfocus(sel, true);
            self.selmon = m;
            self.focus(None);
        }
        if ev.window == self.mons[self.selmon].barwin {
            let mut i = 0;
            let mut x = 0;
            let mut occ = 0u32;
            let tagbits = self.tagbits();
            let mut c = self.mons[self.selmon].clients;
            while let Some(j) = c {
                occ |= if self.clients[j].tags == tagbits { 0 } else { self.clients[j].tags };
                c = self.clients[j].next;
            }
            loop {
                /* Do not reserve space for vacant tags */
                let mon = &self.mons[self.selmon];
                if occ & 1 << i != 0 || mon.tagset[mon.seltags] & 1 << i != 0 {
                    x += Self::textw(&mut self.drw, self.lrpad, &config.tags[i]);
                }
                if ev.x >= x {
                    i += 1;
                    if i < config.tags.len() {
                        continue;
                    }
                }
                break;
            }
            if i < config.tags.len() {
                click = CLK_TAG_BAR;
                arg = Arg::Ui(1 << i);
            } else if ev.x < x + Self::textw(&mut self.drw, self.lrpad, &self.mons[self.selmon].ltsymbol) {
                click = CLK_LT_SYMBOL;
            } else if ev.x > self.mons[self.selmon].ww - Self::textw(&mut self.drw, self.lrpad, &self.stext) + self.lrpad - 2 {
                click = CLK_STATUS_TEXT;
            }
            /* notitle: the space between the layout symbol and the status is
             * no click target; click stays ClkRootWin */
        } else if let Some(c) = self.wintoclient(ev.window) {
            if config.focusonwheel || (ev.button != Button4 && ev.button != Button5) {
                /* deliberately no restack() here, unlike dwm and the focusonclick
                 * patch: a click focuses a floating window without raising it */
                self.focus(Some(c));
            }
            // SAFETY: plain Xlib call on an open display.
            unsafe { XAllowEvents(self.dpy, ReplayPointer, CurrentTime) };
            click = CLK_CLIENT_WIN;
        }
        for b in &config.buttons {
            if click == b.click && b.button == ev.button && self.cleanmask(b.mask) == self.cleanmask(ev.state) {
                (b.func)(self, if click == CLK_TAG_BAR && b.arg.is_zero() { &arg } else { &b.arg });
            }
        }
    }

    pub fn checkotherwm(&mut self) {
        // SAFETY: installing C error handlers and selecting on the root
        // window; the handlers are `unsafe extern "C"` fns defined below.
        unsafe {
            let xerrorxlib = XSetErrorHandler(Some(xerrorstart));
            let _ = XERRORXLIB.set(xerrorxlib);
            /* this causes an error if some other window manager is running */
            XSelectInput(self.dpy, XDefaultRootWindow(self.dpy), SubstructureRedirectMask);
            XSync(self.dpy, False);
            XSetErrorHandler(Some(xerror));
            XSync(self.dpy, False);
        }
    }

    pub fn cleanup(&mut self) {
        let a = Arg::Ui(!0);
        self.view(&a);
        /* selmon->lt[selmon->sellt] = &foo (an empty, floating layout) */
        self.layouts.push(Layout { symbol: String::new(), arrange: None });
        let empty = self.layouts.len() - 1;
        let sellt = self.mons[self.selmon].sellt;
        self.mons[self.selmon].lt[sellt] = empty;
        for m in 0..self.mons.len() {
            while let Some(c) = self.mons[m].stack {
                self.unmanage(c, false);
            }
        }
        // SAFETY: plain Xlib calls on an open display.
        unsafe { XUngrabKey(self.dpy, AnyKey, AnyModifier, self.root) };
        while !self.mons.is_empty() {
            self.cleanupmon(0);
        }
        for cur in self.cursor.iter_mut() {
            self.drw.cur_free(cur);
        }
        for scm in self.scheme.iter_mut() {
            self.drw.scm_free(scm);
        }
        self.scheme.clear();
        // SAFETY: as above; wmcheckwin was created in setup().
        unsafe { XDestroyWindow(self.dpy, self.wmcheckwin) };
        self.drw.free();
        // SAFETY: as above.
        unsafe {
            XSync(self.dpy, False);
            XSetInputFocus(self.dpy, PointerRoot as Window, RevertToPointerRoot, CurrentTime);
            XDeleteProperty(self.dpy, self.root, self.netatom[NET_ACTIVE_WINDOW]);
        }
    }

    fn cleanupmon(&mut self, mon: MonId) {
        /* unlink: monitors are kept in list order, so removing is enough */
        let m = self.mons.remove(mon);
        // SAFETY: barwin was created by updatebars() and is destroyed once.
        unsafe {
            XUnmapWindow(self.dpy, m.barwin);
            XDestroyWindow(self.dpy, m.barwin);
        }
    }

    fn clientmessage(&mut self, e: &XEvent) {
        let cme: XClientMessageEvent = e.into();
        let Some(c) = self.wintoclient(cme.window) else {
            return;
        };

        if cme.message_type == self.netatom[NET_WM_STATE] {
            if cme.data.get_long(1) as Atom == self.netatom[NET_WM_FULLSCREEN]
                || cme.data.get_long(2) as Atom == self.netatom[NET_WM_FULLSCREEN]
            {
                let action = cme.data.get_long(0);
                let fullscreen = action == 1 /* _NET_WM_STATE_ADD    */
                    || (action == 2 /* _NET_WM_STATE_TOGGLE */ && !self.clients[c].isfullscreen);
                self.setfullscreen(c, fullscreen);
            }

            if cme.data.get_long(1) as Atom == self.netatom[NET_WM_STICKY]
                || cme.data.get_long(2) as Atom == self.netatom[NET_WM_STICKY]
            {
                let action = cme.data.get_long(0);
                let sticky = action == 1 || (action == 2 && !self.clients[c].issticky);
                self.setsticky(c, sticky);
            }
        } else if cme.message_type == self.netatom[NET_ACTIVE_WINDOW]
            && Some(c) != self.mons[self.selmon].sel
            && !self.clients[c].isurgent
        {
            self.seturgent(c, true);
        }
    }

    fn configure(&mut self, c: ClientId) {
        let cl = &self.clients[c];
        let ce = XConfigureEvent {
            type_: ConfigureNotify,
            serial: 0,
            send_event: 0,
            display: self.dpy,
            event: cl.win,
            window: cl.win,
            x: cl.x,
            y: cl.y,
            width: cl.w,
            height: cl.h,
            border_width: cl.bw,
            above: 0,
            override_redirect: False,
        };
        let mut ev = XEvent { configure: ce };
        // SAFETY: ev is a fully initialised event.
        unsafe { XSendEvent(self.dpy, cl.win, False, StructureNotifyMask, &mut ev) };
    }

    fn configurenotify(&mut self, e: &XEvent) {
        let ev: XConfigureEvent = e.into();

        /* TODO: updategeom handling sucks, needs to be simplified */
        if ev.window == self.root {
            let dirty = self.sw != ev.width || self.sh != ev.height;
            self.sw = ev.width;
            self.sh = ev.height;
            if self.updategeom() || dirty {
                self.drw.resize(self.sw.max(1) as u32, self.bh.max(1) as u32);
                self.updatebars();
                for m in 0..self.mons.len() {
                    let mut c = self.mons[m].clients;
                    while let Some(i) = c {
                        if self.clients[i].isfullscreen {
                            let (mx, my, mw, mh) = (self.mons[m].mx, self.mons[m].my, self.mons[m].mw, self.mons[m].mh);
                            self.resizeclient(i, mx, my, mw, mh);
                        }
                        c = self.clients[i].next;
                    }
                    let mon = &self.mons[m];
                    // SAFETY: barwin is a valid window.
                    unsafe { XMoveResizeWindow(self.dpy, mon.barwin, mon.wx, mon.by, mon.ww as c_uint, self.bh as c_uint) };
                }
                self.focus(None);
                self.arrange(None);
            }
        }
    }

    fn configurerequest(&mut self, e: &XEvent) {
        let ev: XConfigureRequestEvent = e.into();

        if let Some(c) = self.wintoclient(ev.window) {
            if ev.value_mask & CWBorderWidth as c_ulong != 0 {
                self.clients[c].bw = ev.border_width;
            } else if self.clients[c].isfloating || self.arrange_fn(self.selmon).is_none() {
                let m = self.clients[c].mon;
                let (mx, my, mw, mh) = (self.mons[m].mx, self.mons[m].my, self.mons[m].mw, self.mons[m].mh);
                let cl = &mut self.clients[c];
                if ev.value_mask & CWX as c_ulong != 0 {
                    cl.oldx = cl.x;
                    cl.x = mx + ev.x;
                }
                if ev.value_mask & CWY as c_ulong != 0 {
                    cl.oldy = cl.y;
                    cl.y = my + ev.y;
                }
                if ev.value_mask & CWWidth as c_ulong != 0 {
                    cl.oldw = cl.w;
                    cl.w = ev.width;
                }
                if ev.value_mask & CWHeight as c_ulong != 0 {
                    cl.oldh = cl.h;
                    cl.h = ev.height;
                }
                if (cl.x + cl.w) > mx + mw && cl.isfloating {
                    cl.x = mx + (mw / 2 - width(cl) / 2); /* center in x direction */
                }
                if (cl.y + cl.h) > my + mh && cl.isfloating {
                    cl.y = my + (mh / 2 - height(cl) / 2); /* center in y direction */
                }
                if (ev.value_mask & (CWX | CWY) as c_ulong) != 0 && (ev.value_mask & (CWWidth | CWHeight) as c_ulong) == 0 {
                    self.configure(c);
                }
                if self.isvisible(c) {
                    let cl = &self.clients[c];
                    // SAFETY: cl.win is a managed window.
                    unsafe { XMoveResizeWindow(self.dpy, cl.win, cl.x, cl.y, cl.w as c_uint, cl.h as c_uint) };
                }
            } else {
                self.configure(c);
            }
        } else {
            let mut wc = XWindowChanges {
                x: ev.x,
                y: ev.y,
                width: ev.width,
                height: ev.height,
                border_width: ev.border_width,
                sibling: ev.above,
                stack_mode: ev.detail,
            };
            // SAFETY: wc is fully initialised; the window may be gone, which
            // is handled by xerror().
            unsafe { XConfigureWindow(self.dpy, ev.window, ev.value_mask as c_uint, &mut wc) };
        }
        // SAFETY: plain Xlib call.
        unsafe { XSync(self.dpy, False) };
    }

    fn createmon(&self) -> Monitor {
        let config = &self.config;
        let mut ltsymbol = self.layouts[0].symbol.clone();
        truncate_utf8(&mut ltsymbol, LTSYMBOL_SIZE - 1);
        Monitor {
            tagset: [1, 1],
            mfact: config.mfact,
            nmaster: config.nmaster,
            showbar: config.showbar,
            topbar: config.topbar,
            gappih: config.gappih as i32,
            gappiv: config.gappiv as i32,
            gappoh: config.gappoh as i32,
            gappov: config.gappov as i32,
            lt: [0, 1 % self.layouts.len()],
            ltsymbol,
            ..Default::default()
        }
    }

    fn destroynotify(&mut self, e: &XEvent) {
        let ev: XDestroyWindowEvent = e.into();

        if let Some(c) = self.wintoclient(ev.window) {
            self.unmanage(c, true);
        }
    }

    fn detach(&mut self, c: ClientId) {
        let m = self.clients[c].mon;
        let next = self.clients[c].next;

        if self.mons[m].clients == Some(c) {
            self.mons[m].clients = next;
            return;
        }
        let mut tc = self.mons[m].clients;
        while let Some(i) = tc {
            if self.clients[i].next == Some(c) {
                self.clients[i].next = next;
                return;
            }
            tc = self.clients[i].next;
        }
    }

    fn detachstack(&mut self, c: ClientId) {
        let m = self.clients[c].mon;
        let snext = self.clients[c].snext;

        if self.mons[m].stack == Some(c) {
            self.mons[m].stack = snext;
        } else {
            let mut tc = self.mons[m].stack;
            while let Some(i) = tc {
                if self.clients[i].snext == Some(c) {
                    self.clients[i].snext = snext;
                    break;
                }
                tc = self.clients[i].snext;
            }
        }

        if Some(c) == self.mons[m].sel {
            let mut t = self.mons[m].stack;
            while let Some(i) = t {
                if self.isvisible(i) {
                    break;
                }
                t = self.clients[i].snext;
            }
            self.mons[m].sel = t;
        }
    }

    fn dirtomon(&self, dir: i32) -> MonId {
        let n = self.mons.len();
        if dir > 0 {
            if self.selmon + 1 < n {
                self.selmon + 1
            } else {
                0
            }
        } else if self.selmon == 0 {
            n - 1
        } else {
            self.selmon - 1
        }
    }

    fn drawbar(&mut self, m: MonId) {
        let config = Rc::clone(&self.config);
        let mut tw = 0;
        let (mut occ, mut urg) = (0u32, 0u32);
        let tagbits = self.tagbits();
        let (bh, lrpad) = (self.bh, self.lrpad);

        if !self.mons[m].showbar {
            return;
        }

        /* draw status first so it can be overdrawn by tags later */
        if m == self.selmon {
            /* status is only drawn on selected monitor */
            self.drw.setscheme(&self.scheme[SCHEME_NORM]);
            tw = Self::textw(&mut self.drw, lrpad, &self.stext) - lrpad + 2; /* 2px right padding */
            self.drw.text(self.mons[m].ww - tw, 0, tw as u32, bh as u32, 0, &self.stext, false);
        }

        let mut c = self.mons[m].clients;
        while let Some(i) = c {
            occ |= if self.clients[i].tags == tagbits { 0 } else { self.clients[i].tags };
            if self.clients[i].isurgent {
                urg |= self.clients[i].tags;
            }
            c = self.clients[i].next;
        }
        let mut x = 0;
        for (i, tag) in config.tags.iter().enumerate() {
            let mon = &self.mons[m];
            /* Do not draw vacant tags */
            if !(occ & 1 << i != 0 || mon.tagset[mon.seltags] & 1 << i != 0) {
                continue;
            }
            let w = Self::textw(&mut self.drw, lrpad, tag);
            let scheme = if mon.tagset[mon.seltags] & 1 << i != 0 { SCHEME_SEL } else { SCHEME_NORM };
            self.drw.setscheme(&self.scheme[scheme]);
            self.drw.text(x, 0, w as u32, bh as u32, (lrpad / 2) as u32, tag, urg & 1 << i != 0);
            x += w;
        }
        let w = Self::textw(&mut self.drw, lrpad, &self.mons[m].ltsymbol);
        self.drw.setscheme(&self.scheme[SCHEME_NORM]);
        x = self.drw.text(x, 0, w as u32, bh as u32, (lrpad / 2) as u32, &self.mons[m].ltsymbol, false);

        let w = self.mons[m].ww - tw - x;
        if w > bh {
            /* notitle: no window title, just clear the rest of the bar */
            self.drw.setscheme(&self.scheme[SCHEME_NORM]);
            self.drw.rect(x, 0, w as u32, bh as u32, true, true);
        }
        self.drw.map(self.mons[m].barwin, 0, 0, self.mons[m].ww as u32, bh as u32);
    }

    fn drawbars(&mut self) {
        for m in 0..self.mons.len() {
            self.drawbar(m);
        }
    }

    fn expose(&mut self, e: &XEvent) {
        let ev: XExposeEvent = e.into();

        if ev.count == 0 {
            let m = self.wintomon(ev.window);
            self.drawbar(m);
        }
    }

    /// `focus(c)`; `None` focuses the first visible client of the stack (`focus(NULL)`).
    fn focus(&mut self, c: Option<ClientId>) {
        let mut c = c;
        if c.is_none_or(|c| !self.isvisible(c)) {
            c = self.mons[self.selmon].stack;
            while let Some(i) = c {
                if self.isvisible(i) {
                    break;
                }
                c = self.clients[i].snext;
            }
        }
        let sel = self.mons[self.selmon].sel;
        if sel.is_some() && sel != c {
            self.unfocus(sel, false);
        }
        if let Some(c) = c {
            if self.clients[c].mon != self.selmon {
                self.selmon = self.clients[c].mon;
            }
            if self.clients[c].isurgent {
                self.seturgent(c, false);
            }
            self.detachstack(c);
            self.attachstack(c);
            self.grabbuttons(c, true);
            // SAFETY: win is a managed window; the scheme exists after setup().
            unsafe { XSetWindowBorder(self.dpy, self.clients[c].win, self.scheme[SCHEME_SEL][COL_BORDER].pixel) };
            self.setfocus(c);
        } else {
            // SAFETY: plain Xlib calls on the root window.
            unsafe {
                XSetInputFocus(self.dpy, self.root, RevertToPointerRoot, CurrentTime);
                XDeleteProperty(self.dpy, self.root, self.netatom[NET_ACTIVE_WINDOW]);
            }
        }
        self.mons[self.selmon].sel = c;
        self.drawbars();
    }

    /* there are some broken focus acquiring clients needing extra handling */
    fn focusin(&mut self, e: &XEvent) {
        let ev: XFocusChangeEvent = e.into();

        if let Some(sel) = self.mons[self.selmon].sel {
            if ev.window != self.clients[sel].win {
                self.setfocus(sel);
            }
        }
    }

    pub fn focusmon(&mut self, arg: &Arg) {
        if self.mons.len() < 2 {
            return;
        }
        let m = self.dirtomon(arg.i());
        if m == self.selmon {
            return;
        }
        let sel = self.mons[self.selmon].sel;
        self.unfocus(sel, false);
        self.selmon = m;
        self.focus(None);
    }

    pub fn focusstack(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let mut i = self.stackpos(arg);

        if i < 0 || self.mons[selmon].sel.is_some_and(|sel| self.clients[sel].isfullscreen && self.config.lockfullscreen) {
            return;
        }

        let mut p = None;
        let mut c = self.mons[selmon].clients;
        while let Some(k) = c {
            if i == 0 && self.isvisible(k) {
                break;
            }
            if self.isvisible(k) {
                i -= 1;
            }
            p = c;
            c = self.clients[k].next;
        }
        self.focus(c.or(p));
        self.restack(selmon);
    }

    fn getatomprop(&self, c: ClientId, prop: Atom) -> Atom {
        let mut di: c_int = 0;
        let mut dl: c_ulong = 0;
        let mut nitems: c_ulong = 0;
        let mut p: *mut c_uchar = ptr::null_mut();
        let mut da: Atom = 0;
        let mut atom: Atom = 0;

        // SAFETY: all out-pointers are valid; p is freed with XFree.
        unsafe {
            if XGetWindowProperty(
                self.dpy,
                self.clients[c].win,
                prop,
                0,
                mem::size_of::<Atom>() as c_long,
                False,
                XA_ATOM,
                &mut da,
                &mut di,
                &mut nitems,
                &mut dl,
                &mut p,
            ) == Success as c_int
                && !p.is_null()
            {
                if nitems > 0 && di == 32 {
                    atom = ptr::read_unaligned(p as *const c_long) as Atom;
                }
                XFree(p as *mut _);
            }
        }
        atom
    }

    /// `getrootptr(&x, &y)`
    fn getrootptr(&self) -> Option<(i32, i32)> {
        let mut di: c_int = 0;
        let mut dui: c_uint = 0;
        let mut dummy: Window = 0;
        let (mut x, mut y) = (0, 0);

        // SAFETY: all out-pointers are valid.
        let ok = unsafe { XQueryPointer(self.dpy, self.root, &mut dummy, &mut dummy, &mut x, &mut y, &mut di, &mut di, &mut dui) };
        (ok != 0).then_some((x, y))
    }

    fn getstate(&self, w: Window) -> c_long {
        let mut format: c_int = 0;
        let mut result: c_long = -1;
        let mut p: *mut c_uchar = ptr::null_mut();
        let (mut n, mut extra): (c_ulong, c_ulong) = (0, 0);
        let mut real: Atom = 0;

        // SAFETY: all out-pointers are valid; p is freed with XFree.
        unsafe {
            if XGetWindowProperty(
                self.dpy,
                w,
                self.wmatom[WM_STATE],
                0,
                2,
                False,
                self.wmatom[WM_STATE],
                &mut real,
                &mut format,
                &mut n,
                &mut extra,
                &mut p,
            ) != Success as c_int
            {
                return -1;
            }
            if n != 0 && format == 32 && !p.is_null() {
                result = ptr::read_unaligned(p as *const c_long);
            }
            if !p.is_null() {
                XFree(p as *mut _);
            }
        }
        result
    }

    /// Read a text property into `text` (at most `size - 1` bytes, like the
    /// `char text[size]` buffers in dwm). Takes the display explicitly so it
    /// can write into a field of `Dwm`.
    fn gettextprop(dpy: *mut Display, w: Window, atom: Atom, text: &mut String, size: usize) -> bool {
        let mut list: *mut *mut c_char = ptr::null_mut();
        let mut n: c_int = 0;
        let mut name = XTextProperty { value: ptr::null_mut(), encoding: 0, format: 0, nitems: 0 };

        if size == 0 {
            return false;
        }
        text.clear();
        // SAFETY: name.value is only read for name.nitems bytes and freed once.
        unsafe {
            if XGetTextProperty(dpy, w, &mut name, atom) == 0 || name.nitems == 0 || name.value.is_null() {
                return false;
            }
            if name.encoding == XA_STRING {
                let bytes = std::slice::from_raw_parts(name.value, name.nitems as usize);
                Self::copy_text(text, bytes, size);
            } else if XmbTextPropertyToTextList(dpy, &name, &mut list, &mut n) >= Success as c_int
                && n > 0
                && !list.is_null()
                && !(*list).is_null()
            {
                Self::copy_text(text, std::ffi::CStr::from_ptr(*list).to_bytes(), size);
                XFreeStringList(list);
            }
            XFree(name.value as *mut _);
        }
        true
    }

    /// `strncpy(text, bytes, size - 1)` with UTF-8 awareness.
    fn copy_text(text: &mut String, bytes: &[u8], size: usize) {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        *text = String::from_utf8_lossy(&bytes[..end]).into_owned();
        truncate_utf8(text, size - 1);
    }

    fn grabbuttons(&mut self, c: ClientId, focused: bool) {
        self.updatenumlockmask();
        let modifiers = [0, LockMask, self.numlockmask, self.numlockmask | LockMask];
        let win = self.clients[c].win;
        // SAFETY: plain Xlib grabs on a managed window.
        unsafe {
            XUngrabButton(self.dpy, AnyButton as c_uint, AnyModifier, win);
            if !focused {
                XGrabButton(self.dpy, AnyButton as c_uint, AnyModifier, win, False, BUTTONMASK as c_uint, GrabModeSync, GrabModeSync, 0, 0);
            }
            for b in &self.config.buttons {
                if b.click == CLK_CLIENT_WIN {
                    for modifier in modifiers {
                        XGrabButton(
                            self.dpy,
                            b.button,
                            b.mask | modifier,
                            win,
                            False,
                            BUTTONMASK as c_uint,
                            GrabModeAsync,
                            GrabModeSync,
                            0,
                            0,
                        );
                    }
                }
            }
        }
    }

    fn grabkeys(&mut self) {
        self.updatenumlockmask();
        let modifiers = [0, LockMask, self.numlockmask, self.numlockmask | LockMask];
        let (mut start, mut end, mut skip): (c_int, c_int, c_int) = (0, 0, 0);

        // SAFETY: syms points to (end - start + 1) * skip KeySyms until XFree.
        unsafe {
            XUngrabKey(self.dpy, AnyKey, AnyModifier, self.root);
            XDisplayKeycodes(self.dpy, &mut start, &mut end);
            let syms = XGetKeyboardMapping(self.dpy, start as KeyCode, end - start + 1, &mut skip);
            if syms.is_null() {
                return;
            }
            for k in start..=end {
                for key in &self.config.keys {
                    /* skip modifier codes, we do that ourselves */
                    if key.keysym == *syms.add(((k - start) * skip) as usize) {
                        for modifier in modifiers {
                            XGrabKey(self.dpy, k, key.mod_ | modifier, self.root, True, GrabModeAsync, GrabModeAsync);
                        }
                    }
                }
            }
            XFree(syms as *mut _);
        }
    }

    pub fn incnmaster(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        self.mons[selmon].nmaster = (self.mons[selmon].nmaster + arg.i()).max(0);
        self.arrange(Some(selmon));
    }

    fn keypress(&mut self, e: &XEvent) {
        let config = Rc::clone(&self.config);
        let ev: XKeyEvent = e.into();

        // SAFETY: plain Xlib call.
        let keysym = unsafe { XKeycodeToKeysym(self.dpy, ev.keycode as KeyCode, 0) };
        for key in &config.keys {
            if keysym == key.keysym && self.cleanmask(key.mod_) == self.cleanmask(ev.state) {
                (key.func)(self, &key.arg);
            }
        }
    }

    pub fn killclient(&mut self, _arg: &Arg) {
        let Some(sel) = self.mons[self.selmon].sel else {
            return;
        };
        if !self.sendevent(sel, self.wmatom[WM_DELETE]) {
            // SAFETY: the error handler swap protects against a vanished window.
            unsafe {
                XGrabServer(self.dpy);
                XSetErrorHandler(Some(xerrordummy));
                XSetCloseDownMode(self.dpy, DestroyAll);
                XKillClient(self.dpy, self.clients[sel].win);
                XSync(self.dpy, False);
                XSetErrorHandler(Some(xerror));
                XUngrabServer(self.dpy);
            }
        }
    }

    fn manage(&mut self, w: Window, wa: &XWindowAttributes) {
        let mut trans: Window = 0;

        let c = self.alloc_client(Client {
            win: w,
            /* geometry */
            x: wa.x,
            oldx: wa.x,
            y: wa.y,
            oldy: wa.y,
            w: wa.width,
            oldw: wa.width,
            h: wa.height,
            oldh: wa.height,
            oldbw: wa.border_width,
            mon: self.selmon,
            ..Default::default()
        });

        self.updatetitle(c);
        // SAFETY: trans is a valid out-pointer.
        let t = if unsafe { XGetTransientForHint(self.dpy, w, &mut trans) } != 0 { self.wintoclient(trans) } else { None };
        if let Some(t) = t {
            self.clients[c].mon = self.clients[t].mon;
            self.clients[c].tags = self.clients[t].tags;
        } else {
            self.clients[c].mon = self.selmon;
            self.applyrules(c);
        }

        {
            let m = self.clients[c].mon;
            let (wx, wy, ww, wh) = (self.mons[m].wx, self.mons[m].wy, self.mons[m].ww, self.mons[m].wh);
            let cl = &mut self.clients[c];
            if cl.x + width(cl) > wx + ww {
                cl.x = wx + ww - width(cl);
            }
            if cl.y + height(cl) > wy + wh {
                cl.y = wy + wh - height(cl);
            }
            cl.x = cl.x.max(wx);
            cl.y = cl.y.max(wy);
            cl.bw = self.config.borderpx as i32;
        }

        let mut wc = XWindowChanges { x: 0, y: 0, width: 0, height: 0, border_width: self.clients[c].bw, sibling: 0, stack_mode: 0 };
        // SAFETY: w is the window being managed; wc is initialised.
        unsafe {
            XConfigureWindow(self.dpy, w, CWBorderWidth as c_uint, &mut wc);
            XSetWindowBorder(self.dpy, w, self.scheme[SCHEME_NORM][COL_BORDER].pixel);
        }
        self.configure(c); /* propagates border_width, if size doesn't change */
        self.updatewindowtype(c);
        self.updatesizehints(c);
        self.updatewmhints(c);
        {
            let cl = &mut self.clients[c];
            cl.sfx = cl.x;
            cl.sfy = cl.y;
            cl.sfw = cl.w;
            cl.sfh = cl.h;
        }
        // SAFETY: plain Xlib call.
        unsafe { XSelectInput(self.dpy, w, EnterWindowMask | FocusChangeMask | PropertyChangeMask | StructureNotifyMask) };
        self.grabbuttons(c, false);
        if !self.clients[c].isfloating {
            let floating = trans != 0 || self.clients[c].isfixed;
            self.clients[c].isfloating = floating;
            self.clients[c].oldstate = floating;
        }
        if self.clients[c].isfloating {
            // SAFETY: plain Xlib call.
            unsafe { XRaiseWindow(self.dpy, self.clients[c].win) };
        }
        self.attach(c);
        self.attachstack(c);
        let win = self.clients[c].win;
        // SAFETY: the property data is a single Window.
        unsafe {
            XChangeProperty(
                self.dpy,
                self.root,
                self.netatom[NET_CLIENT_LIST],
                XA_WINDOW,
                32,
                PropModeAppend,
                &win as *const Window as *const c_uchar,
                1,
            );
            let cl = &self.clients[c];
            XMoveResizeWindow(self.dpy, cl.win, cl.x + 2 * self.sw, cl.y, cl.w as c_uint, cl.h as c_uint); /* some windows require this */
        }
        self.setclientstate(c, NORMAL_STATE);
        let m = self.clients[c].mon;
        if m == self.selmon {
            let sel = self.mons[self.selmon].sel;
            self.unfocus(sel, false);
        }
        self.mons[m].sel = Some(c);
        self.arrange(Some(m));
        // SAFETY: plain Xlib call.
        unsafe { XMapWindow(self.dpy, self.clients[c].win) };
        self.focus(None);
    }

    fn mappingnotify(&mut self, e: &XEvent) {
        let mut ev: XMappingEvent = e.into();

        // SAFETY: ev is a copy of a valid mapping event.
        unsafe { XRefreshKeyboardMapping(&mut ev) };
        if ev.request == MappingKeyboard {
            self.grabkeys();
        }
    }

    fn maprequest(&mut self, e: &XEvent) {
        let ev: XMapRequestEvent = e.into();
        // SAFETY: XWindowAttributes is plain data; it is filled by Xlib.
        let mut wa: XWindowAttributes = unsafe { mem::zeroed() };

        // SAFETY: wa is a valid out-pointer.
        if unsafe { XGetWindowAttributes(self.dpy, ev.window, &mut wa) } == 0 || wa.override_redirect != 0 {
            return;
        }
        if self.wintoclient(ev.window).is_none() {
            self.manage(ev.window, &wa);
        }
    }

    pub fn monocle(&mut self, m: MonId) {
        let mut n = 0u32;

        let mut c = self.mons[m].clients;
        while let Some(i) = c {
            if self.isvisible(i) {
                n += 1;
            }
            c = self.clients[i].next;
        }
        if n > 0 {
            /* override layout symbol */
            let mut symbol = format!("[{}]", n);
            truncate_utf8(&mut symbol, LTSYMBOL_SIZE - 1);
            self.mons[m].ltsymbol = symbol;
        }
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(i) = c {
            let (wx, wy, ww, wh) = (self.mons[m].wx, self.mons[m].wy, self.mons[m].ww, self.mons[m].wh);
            let bw = self.clients[i].bw;
            self.resize(i, wx, wy, ww - 2 * bw, wh - 2 * bw, false);
            c = self.nexttiled(self.clients[i].next);
        }
    }

    pub fn movemouse(&mut self, _arg: &Arg) {
        let snap = self.config.snap as i32;
        // SAFETY: XEvent is plain data, filled by XMaskEvent before use.
        let mut ev: XEvent = unsafe { mem::zeroed() };
        let mut lasttime: Time = 0;

        let Some(c) = self.mons[self.selmon].sel else {
            return;
        };
        if self.clients[c].isfullscreen {
            /* no support moving fullscreen windows by mouse */
            return;
        }
        self.restack(self.selmon);
        let ocx = self.clients[c].x;
        let ocy = self.clients[c].y;
        // SAFETY: plain Xlib grab on the root window.
        let grab = unsafe {
            XGrabPointer(
                self.dpy,
                self.root,
                False,
                MOUSEMASK as c_uint,
                GrabModeAsync,
                GrabModeAsync,
                0,
                self.cursor[CUR_MOVE].cursor,
                CurrentTime,
            )
        };
        if grab != GrabSuccess {
            return;
        }
        let Some((x, y)) = self.getrootptr() else {
            return;
        };
        loop {
            // SAFETY: ev is a valid out-pointer.
            unsafe { XMaskEvent(self.dpy, MOUSEMASK | ExposureMask | SubstructureRedirectMask, &mut ev) };
            match ev.get_type() {
                ConfigureRequest | Expose | MapRequest => {
                    if let Some(h) = handler(ev.get_type()) {
                        h(self, &ev);
                    }
                }
                MotionNotify => {
                    let mev: XMotionEvent = (&ev).into();
                    if mev.time.wrapping_sub(lasttime) <= (1000 / self.config.refreshrate) as Time {
                        continue;
                    }
                    lasttime = mev.time;

                    let selmon = &self.mons[self.selmon];
                    let cl = &self.clients[c];
                    let mut nx = ocx + (mev.x - x);
                    let mut ny = ocy + (mev.y - y);
                    if (selmon.wx - nx).abs() < snap {
                        nx = selmon.wx;
                    } else if ((selmon.wx + selmon.ww) - (nx + width(cl))).abs() < snap {
                        nx = selmon.wx + selmon.ww - width(cl);
                    }
                    if (selmon.wy - ny).abs() < snap {
                        ny = selmon.wy;
                    } else if ((selmon.wy + selmon.wh) - (ny + height(cl))).abs() < snap {
                        ny = selmon.wy + selmon.wh - height(cl);
                    }
                    if !cl.isfloating
                        && self.arrange_fn(self.selmon).is_some()
                        && ((nx - cl.x).abs() > snap || (ny - cl.y).abs() > snap)
                    {
                        self.togglefloating(&Arg::None);
                    }
                    if self.arrange_fn(self.selmon).is_none() || self.clients[c].isfloating {
                        let (w, h) = (self.clients[c].w, self.clients[c].h);
                        self.resize(c, nx, ny, w, h, true);
                    }
                }
                _ => {}
            }
            if ev.get_type() == ButtonRelease {
                break;
            }
        }
        // SAFETY: plain Xlib call.
        unsafe { XUngrabPointer(self.dpy, CurrentTime) };
        let cl = &self.clients[c];
        let m = self.recttomon(cl.x, cl.y, cl.w, cl.h);
        if m != self.selmon {
            self.sendmon(c, m);
            self.selmon = m;
            self.focus(None);
        }
    }

    fn nexttiled(&self, c: Option<ClientId>) -> Option<ClientId> {
        let mut c = c;
        while let Some(i) = c {
            if !(self.clients[i].isfloating || !self.isvisible(i)) {
                break;
            }
            c = self.clients[i].next;
        }
        c
    }

    fn pop(&mut self, c: ClientId) {
        self.detach(c);
        self.attach(c);
        self.focus(Some(c));
        let m = self.clients[c].mon;
        self.arrange(Some(m));
    }

    fn propertynotify(&mut self, e: &XEvent) {
        let mut trans: Window = 0;
        let ev: XPropertyEvent = e.into();

        if ev.window == self.root && ev.atom == XA_WM_NAME {
            self.updatestatus();
        } else if ev.state == PropertyDelete {
            /* ignore */
        } else if let Some(c) = self.wintoclient(ev.window) {
            match ev.atom {
                XA_WM_TRANSIENT_FOR => {
                    // SAFETY: trans is a valid out-pointer.
                    if !self.clients[c].isfloating && unsafe { XGetTransientForHint(self.dpy, self.clients[c].win, &mut trans) } != 0 {
                        let floating = self.wintoclient(trans).is_some();
                        self.clients[c].isfloating = floating;
                        if floating {
                            let m = self.clients[c].mon;
                            self.arrange(Some(m));
                        }
                    }
                }
                XA_WM_NORMAL_HINTS => {
                    self.clients[c].hintsvalid = false;
                }
                XA_WM_HINTS => {
                    self.updatewmhints(c);
                    self.drawbars();
                }
                _ => {}
            }
            if ev.atom == XA_WM_NAME || ev.atom == self.netatom[NET_WM_NAME] {
                self.updatetitle(c); /* notitle: the bar does not show it */
            }
            if ev.atom == self.netatom[NET_WM_WINDOW_TYPE] {
                self.updatewindowtype(c);
            }
        }
    }

    pub fn pushstack(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let mut i = self.stackpos(arg);
        let Some(sel) = self.mons[selmon].sel else {
            return;
        };

        if i < 0 {
            return;
        } else if i == 0 {
            self.detach(sel);
            self.attach(sel);
        } else {
            let mut p = None;
            let mut c = self.mons[selmon].clients;
            while let Some(k) = c {
                if self.isvisible(k) && k != sel {
                    i -= 1;
                    if i == 0 {
                        break;
                    }
                }
                p = c;
                c = self.clients[k].next;
            }
            /* c is Some here: the list is not empty, so p is the last client if
             * the walk ran out; pushing sel after itself is a no-op */
            if let Some(c) = c.or(p) {
                self.detach(sel);
                self.clients[sel].next = self.clients[c].next;
                self.clients[c].next = Some(sel);
            }
        }
        self.arrange(Some(selmon));
    }

    pub fn quit(&mut self, _arg: &Arg) {
        self.running = false;
    }

    fn recttomon(&self, x: i32, y: i32, w: i32, h: i32) -> MonId {
        let mut r = self.selmon;
        let mut area = 0;

        for (i, m) in self.mons.iter().enumerate() {
            let a = intersect(x, y, w, h, m);
            if a > area {
                area = a;
                r = i;
            }
        }
        r
    }

    fn resize(&mut self, c: ClientId, mut x: i32, mut y: i32, mut w: i32, mut h: i32, interact: bool) {
        if self.applysizehints(c, &mut x, &mut y, &mut w, &mut h, interact) {
            self.resizeclient(c, x, y, w, h);
        }
    }

    fn resizeclient(&mut self, c: ClientId, x: i32, y: i32, w: i32, h: i32) {
        let cl = &mut self.clients[c];
        cl.oldx = cl.x;
        cl.x = x;
        cl.oldy = cl.y;
        cl.y = y;
        cl.oldw = cl.w;
        cl.w = w;
        cl.oldh = cl.h;
        cl.h = h;
        let (m, bw) = (cl.mon, cl.bw);
        let mut wc = XWindowChanges { x, y, width: w, height: h, border_width: bw, sibling: 0, stack_mode: 0 };
        /* noborder: the only visible tiled client, or any client in monocle,
         * fills the space the border would take; c.bw itself is kept */
        let mon = &self.mons[m];
        let monocle = self.layouts.get(mon.lt[mon.sellt]).and_then(|l| l.arrange);
        let cl = &self.clients[c];
        if ((self.nexttiled(mon.clients) == Some(c) && self.nexttiled(cl.next).is_none())
            || monocle.is_some_and(|f| std::ptr::fn_addr_eq(f, Dwm::monocle as ArrangeFn)))
            && !cl.isfullscreen
            && !cl.isfloating
        {
            wc.width += bw * 2;
            wc.height += bw * 2;
            wc.border_width = 0;
            let cl = &mut self.clients[c];
            cl.w = wc.width;
            cl.h = wc.height;
        }
        let win = self.clients[c].win;
        // SAFETY: wc is initialised; win is a managed window.
        unsafe {
            XConfigureWindow(self.dpy, win, (CWX | CWY | CWWidth | CWHeight | CWBorderWidth) as c_uint, &mut wc);
        }
        self.configure(c);
        // SAFETY: plain Xlib call.
        unsafe { XSync(self.dpy, False) };
    }

    pub fn resizemouse(&mut self, _arg: &Arg) {
        let snap = self.config.snap as i32;
        // SAFETY: XEvent is plain data, filled by XMaskEvent before use.
        let mut ev: XEvent = unsafe { mem::zeroed() };
        let mut lasttime: Time = 0;

        let Some(c) = self.mons[self.selmon].sel else {
            return;
        };
        if self.clients[c].isfullscreen {
            /* no support resizing fullscreen windows by mouse */
            return;
        }
        self.restack(self.selmon);
        let ocx = self.clients[c].x;
        let ocy = self.clients[c].y;
        // SAFETY: plain Xlib grab on the root window.
        let grab = unsafe {
            XGrabPointer(
                self.dpy,
                self.root,
                False,
                MOUSEMASK as c_uint,
                GrabModeAsync,
                GrabModeAsync,
                0,
                self.cursor[CUR_RESIZE].cursor,
                CurrentTime,
            )
        };
        if grab != GrabSuccess {
            return;
        }
        {
            let cl = &self.clients[c];
            // SAFETY: plain Xlib call.
            unsafe { XWarpPointer(self.dpy, 0, cl.win, 0, 0, 0, 0, cl.w + cl.bw - 1, cl.h + cl.bw - 1) };
        }
        loop {
            // SAFETY: ev is a valid out-pointer.
            unsafe { XMaskEvent(self.dpy, MOUSEMASK | ExposureMask | SubstructureRedirectMask, &mut ev) };
            match ev.get_type() {
                ConfigureRequest | Expose | MapRequest => {
                    if let Some(h) = handler(ev.get_type()) {
                        h(self, &ev);
                    }
                }
                MotionNotify => {
                    let mev: XMotionEvent = (&ev).into();
                    if mev.time.wrapping_sub(lasttime) <= (1000 / self.config.refreshrate) as Time {
                        continue;
                    }
                    lasttime = mev.time;

                    let cl = &self.clients[c];
                    let selmon = &self.mons[self.selmon];
                    let cmon = &self.mons[cl.mon];
                    let nw = (mev.x - ocx - 2 * cl.bw + 1).max(1);
                    let nh = (mev.y - ocy - 2 * cl.bw + 1).max(1);
                    if cmon.wx + nw >= selmon.wx
                        && cmon.wx + nw <= selmon.wx + selmon.ww
                        && cmon.wy + nh >= selmon.wy
                        && cmon.wy + nh <= selmon.wy + selmon.wh
                        && !cl.isfloating
                        && self.arrange_fn(self.selmon).is_some()
                        && ((nw - cl.w).abs() > snap || (nh - cl.h).abs() > snap)
                    {
                        self.togglefloating(&Arg::None);
                    }
                    if self.arrange_fn(self.selmon).is_none() || self.clients[c].isfloating {
                        let (x, y) = (self.clients[c].x, self.clients[c].y);
                        self.resize(c, x, y, nw, nh, true);
                    }
                }
                _ => {}
            }
            if ev.get_type() == ButtonRelease {
                break;
            }
        }
        {
            let cl = &self.clients[c];
            // SAFETY: plain Xlib calls; ev is a valid out-pointer.
            unsafe {
                XWarpPointer(self.dpy, 0, cl.win, 0, 0, 0, 0, cl.w + cl.bw - 1, cl.h + cl.bw - 1);
                XUngrabPointer(self.dpy, CurrentTime);
                while XCheckMaskEvent(self.dpy, EnterWindowMask, &mut ev) != 0 {}
            }
        }
        let cl = &self.clients[c];
        let m = self.recttomon(cl.x, cl.y, cl.w, cl.h);
        if m != self.selmon {
            self.sendmon(c, m);
            self.selmon = m;
            self.focus(None);
        }
    }

    fn restack(&mut self, m: MonId) {
        // SAFETY: XEvent is plain data, only written by XCheckMaskEvent.
        let mut ev: XEvent = unsafe { mem::zeroed() };

        self.drawbar(m);
        let Some(sel) = self.mons[m].sel else {
            return;
        };
        let arrange = self.arrange_fn(m);
        if self.clients[sel].isfloating || arrange.is_none() {
            // SAFETY: plain Xlib call.
            unsafe { XRaiseWindow(self.dpy, self.clients[sel].win) };
        }
        if arrange.is_some() {
            let mut wc = XWindowChanges {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                border_width: 0,
                sibling: self.mons[m].barwin,
                stack_mode: Below,
            };
            let mut c = self.mons[m].stack;
            while let Some(i) = c {
                if !self.clients[i].isfloating && self.isvisible(i) {
                    // SAFETY: wc is initialised; win is a managed window.
                    unsafe { XConfigureWindow(self.dpy, self.clients[i].win, (CWSibling | CWStackMode) as c_uint, &mut wc) };
                    wc.sibling = self.clients[i].win;
                }
                c = self.clients[i].snext;
            }
        }
        // SAFETY: plain Xlib calls; ev is a valid out-pointer.
        unsafe {
            XSync(self.dpy, False);
            while XCheckMaskEvent(self.dpy, EnterWindowMask, &mut ev) != 0 {}
        }
    }

    pub fn run(&mut self) {
        // SAFETY: XEvent is plain data, filled by XNextEvent before use.
        let mut ev: XEvent = unsafe { mem::zeroed() };
        /* main event loop */
        // SAFETY: plain Xlib call.
        unsafe { XSync(self.dpy, False) };
        // SAFETY: ev is a valid out-pointer.
        while self.running && unsafe { XNextEvent(self.dpy, &mut ev) } == 0 {
            if let Some(h) = handler(ev.get_type()) {
                h(self, &ev); /* call handler */
            }
        }
    }

    pub fn scan(&mut self) {
        let mut num: c_uint = 0;
        let (mut d1, mut d2): (Window, Window) = (0, 0);
        let mut wins: *mut Window = ptr::null_mut();
        // SAFETY: XWindowAttributes is plain data; it is filled by Xlib.
        let mut wa: XWindowAttributes = unsafe { mem::zeroed() };

        // SAFETY: wins points to num Windows until XFree.
        unsafe {
            if XQueryTree(self.dpy, self.root, &mut d1, &mut d2, &mut wins, &mut num) != 0 {
                let wins_slice: Vec<Window> =
                    if wins.is_null() { Vec::new() } else { std::slice::from_raw_parts(wins, num as usize).to_vec() };
                for &w in &wins_slice {
                    if XGetWindowAttributes(self.dpy, w, &mut wa) == 0
                        || wa.override_redirect != 0
                        || XGetTransientForHint(self.dpy, w, &mut d1) != 0
                    {
                        continue;
                    }
                    if wa.map_state == IsViewable || self.getstate(w) == ICONIC_STATE {
                        self.manage(w, &wa);
                    }
                }
                for &w in &wins_slice {
                    /* now the transients */
                    if XGetWindowAttributes(self.dpy, w, &mut wa) == 0 {
                        continue;
                    }
                    if XGetTransientForHint(self.dpy, w, &mut d1) != 0
                        && (wa.map_state == IsViewable || self.getstate(w) == ICONIC_STATE)
                    {
                        self.manage(w, &wa);
                    }
                }
                if !wins.is_null() {
                    XFree(wins as *mut _);
                }
            }
        }
    }

    fn sendmon(&mut self, c: ClientId, m: MonId) {
        if self.clients[c].mon == m {
            return;
        }
        self.unfocus(Some(c), true);
        self.detach(c);
        self.detachstack(c);
        self.clients[c].mon = m;
        /* assign tags of target monitor, without any visible scratchpad tags */
        let tags = self.mons[m].tagset[self.mons[m].seltags] & !self.sptagmask();
        self.clients[c].tags = if tags != 0 { tags } else { 1 };
        self.attach(c);
        self.attachstack(c);
        if self.clients[c].isfullscreen {
            let (mx, my, mw, mh) = (self.mons[m].mx, self.mons[m].my, self.mons[m].mw, self.mons[m].mh);
            self.resizeclient(c, mx, my, mw, mh);
        }
        self.focus(None);
        self.arrange(None);
    }

    fn setclientstate(&mut self, c: ClientId, state: c_long) {
        let data: [c_long; 2] = [state, 0];

        // SAFETY: data holds two 32-bit-format longs, as XChangeProperty expects.
        unsafe {
            XChangeProperty(
                self.dpy,
                self.clients[c].win,
                self.wmatom[WM_STATE],
                self.wmatom[WM_STATE],
                32,
                PropModeReplace,
                data.as_ptr() as *const c_uchar,
                2,
            );
        }
    }

    fn sendevent(&mut self, c: ClientId, proto: Atom) -> bool {
        let mut n: c_int = 0;
        let mut protocols: *mut Atom = ptr::null_mut();
        let mut exists = false;
        let win = self.clients[c].win;

        // SAFETY: protocols points to n Atoms until XFree; ev is initialised
        // before it is sent.
        unsafe {
            if XGetWMProtocols(self.dpy, win, &mut protocols, &mut n) != 0 {
                while !exists && n > 0 {
                    n -= 1;
                    exists = *protocols.add(n as usize) == proto;
                }
                XFree(protocols as *mut _);
            }
            if exists {
                let mut ev: XEvent = mem::zeroed();
                ev.client_message.type_ = ClientMessage;
                ev.client_message.window = win;
                ev.client_message.message_type = self.wmatom[WM_PROTOCOLS];
                ev.client_message.format = 32;
                let data = ev.client_message.data.as_longs_mut();
                data[0] = proto as c_long;
                data[1] = CurrentTime as c_long;
                XSendEvent(self.dpy, win, False, NoEventMask, &mut ev);
            }
        }
        exists
    }

    fn setfocus(&mut self, c: ClientId) {
        let win = self.clients[c].win;
        // SAFETY: win is a managed window; the property data is one Window.
        unsafe {
            if !self.clients[c].neverfocus {
                XSetInputFocus(self.dpy, win, RevertToPointerRoot, CurrentTime);
            }
            XChangeProperty(
                self.dpy,
                self.root,
                self.netatom[NET_ACTIVE_WINDOW],
                XA_WINDOW,
                32,
                PropModeReplace,
                &win as *const Window as *const c_uchar,
                1,
            );
        }
        self.sendevent(c, self.wmatom[WM_TAKE_FOCUS]);
    }

    fn setfullscreen(&mut self, c: ClientId, fullscreen: bool) {
        if fullscreen && !self.clients[c].isfullscreen {
            self.clients[c].isfullscreen = true;
            self.updatenetwmstate(c);
            let cl = &mut self.clients[c];
            cl.oldstate = cl.isfloating;
            cl.oldbw = cl.bw;
            cl.bw = 0;
            cl.isfloating = true;
            let m = cl.mon;
            let (mx, my, mw, mh) = (self.mons[m].mx, self.mons[m].my, self.mons[m].mw, self.mons[m].mh);
            self.resizeclient(c, mx, my, mw, mh);
            // SAFETY: plain Xlib call.
            unsafe { XRaiseWindow(self.dpy, self.clients[c].win) };
        } else if !fullscreen && self.clients[c].isfullscreen {
            self.clients[c].isfullscreen = false;
            self.updatenetwmstate(c);
            let cl = &mut self.clients[c];
            cl.isfloating = cl.oldstate;
            cl.bw = cl.oldbw;
            cl.x = cl.oldx;
            cl.y = cl.oldy;
            cl.w = cl.oldw;
            cl.h = cl.oldh;
            let (x, y, w, h, m) = (cl.x, cl.y, cl.w, cl.h, cl.mon);
            self.resizeclient(c, x, y, w, h);
            self.arrange(Some(m));
        }
    }

    fn setsticky(&mut self, c: ClientId, sticky: bool) {
        if sticky && !self.clients[c].issticky {
            self.clients[c].issticky = true;
            self.updatenetwmstate(c);
        } else if !sticky && self.clients[c].issticky {
            self.clients[c].issticky = false;
            self.updatenetwmstate(c);
            let m = self.clients[c].mon;
            self.arrange(Some(m));
        }
    }

    pub fn setlayout(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let v = match arg {
            Arg::Layout(i) if *i < self.layouts.len() => Some(*i),
            _ => None,
        };
        let sellt = self.mons[selmon].sellt;
        if v.is_none() || v != Some(self.mons[selmon].lt[sellt]) {
            self.mons[selmon].sellt ^= 1;
        }
        if let Some(i) = v {
            let sellt = self.mons[selmon].sellt;
            self.mons[selmon].lt[sellt] = i;
        }
        let mut symbol = self.lt_symbol(selmon).to_string();
        truncate_utf8(&mut symbol, LTSYMBOL_SIZE - 1);
        self.mons[selmon].ltsymbol = symbol;
        if self.mons[selmon].sel.is_some() {
            self.arrange(Some(selmon));
        } else {
            self.drawbar(selmon);
        }
    }

    /* arg > 1.0 will set mfact absolutely */
    pub fn setmfact(&mut self, arg: &Arg) {
        let selmon = self.selmon;

        if self.arrange_fn(selmon).is_none() {
            return;
        }
        let f = if arg.f() < 1.0 { arg.f() + self.mons[selmon].mfact } else { arg.f() - 1.0 };
        if !(0.05..=0.95).contains(&f) {
            return;
        }
        self.mons[selmon].mfact = f;
        self.arrange(Some(selmon));
    }

    pub fn setup(&mut self) {
        let config = Rc::clone(&self.config);

        // SAFETY: sigaction with a zeroed, then fully set, struct; waitpid
        // with WNOHANG never blocks.
        unsafe {
            /* do not transform children into zombies when they terminate */
            let mut sa: libc::sigaction = mem::zeroed();
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = libc::SA_NOCLDSTOP | libc::SA_NOCLDWAIT | libc::SA_RESTART;
            sa.sa_sigaction = libc::SIG_IGN;
            libc::sigaction(libc::SIGCHLD, &sa, ptr::null_mut());

            /* clean up any zombies (inherited from .xinitrc etc) immediately */
            while libc::waitpid(-1, ptr::null_mut(), libc::WNOHANG) > 0 {}
        }

        /* init screen: screen, sw, sh, root and drw were set up in new() */
        if !self.drw.fontset_create(&config.fonts) {
            die("no fonts could be loaded.");
        }
        self.lrpad = self.drw.fonts[0].h as i32;
        self.bh = self.drw.fonts[0].h as i32 + 2;
        self.updategeom();
        /* init atoms */
        let atom = |name: &std::ffi::CStr| -> Atom {
            // SAFETY: name is NUL terminated.
            unsafe { XInternAtom(self.dpy, name.as_ptr(), False) }
        };
        let utf8string = atom(c"UTF8_STRING");
        self.wmatom[WM_PROTOCOLS] = atom(c"WM_PROTOCOLS");
        self.wmatom[WM_DELETE] = atom(c"WM_DELETE_WINDOW");
        self.wmatom[WM_STATE] = atom(c"WM_STATE");
        self.wmatom[WM_TAKE_FOCUS] = atom(c"WM_TAKE_FOCUS");
        self.netatom[NET_ACTIVE_WINDOW] = atom(c"_NET_ACTIVE_WINDOW");
        self.netatom[NET_SUPPORTED] = atom(c"_NET_SUPPORTED");
        self.netatom[NET_WM_NAME] = atom(c"_NET_WM_NAME");
        self.netatom[NET_WM_STATE] = atom(c"_NET_WM_STATE");
        self.netatom[NET_WM_CHECK] = atom(c"_NET_SUPPORTING_WM_CHECK");
        self.netatom[NET_WM_FULLSCREEN] = atom(c"_NET_WM_STATE_FULLSCREEN");
        self.netatom[NET_WM_STICKY] = atom(c"_NET_WM_STATE_STICKY");
        self.netatom[NET_WM_WINDOW_TYPE] = atom(c"_NET_WM_WINDOW_TYPE");
        self.netatom[NET_WM_WINDOW_TYPE_DIALOG] = atom(c"_NET_WM_WINDOW_TYPE_DIALOG");
        self.netatom[NET_CLIENT_LIST] = atom(c"_NET_CLIENT_LIST");
        /* init cursors */
        self.cursor[CUR_NORMAL] = self.drw.cur_create(XC_LEFT_PTR);
        self.cursor[CUR_RESIZE] = self.drw.cur_create(XC_SIZING);
        self.cursor[CUR_MOVE] = self.drw.cur_create(XC_FLEUR);
        /* init appearance */
        self.scheme = config.colors.iter().map(|clrnames| self.drw.scm_create(clrnames)).collect();
        if self.scheme.len() < 2 || self.scheme.iter().any(|s| s.len() < 3) {
            die("dwmr: colors must define the SchemeNorm and SchemeSel schemes with fg, bg and border.");
        }
        /* init bars */
        self.updatebars();
        self.updatestatus();
        /* supporting window for NetWMCheck */
        // SAFETY: plain Xlib property setup on freshly created windows.
        unsafe {
            self.wmcheckwin = XCreateSimpleWindow(self.dpy, self.root, 0, 0, 1, 1, 0, 0, 0);
            XChangeProperty(
                self.dpy,
                self.wmcheckwin,
                self.netatom[NET_WM_CHECK],
                XA_WINDOW,
                32,
                PropModeReplace,
                &self.wmcheckwin as *const Window as *const c_uchar,
                1,
            );
            XChangeProperty(
                self.dpy,
                self.wmcheckwin,
                self.netatom[NET_WM_NAME],
                utf8string,
                8,
                PropModeReplace,
                b"dwmr".as_ptr(),
                4,
            );
            XChangeProperty(
                self.dpy,
                self.root,
                self.netatom[NET_WM_CHECK],
                XA_WINDOW,
                32,
                PropModeReplace,
                &self.wmcheckwin as *const Window as *const c_uchar,
                1,
            );
            /* EWMH support per view */
            XChangeProperty(
                self.dpy,
                self.root,
                self.netatom[NET_SUPPORTED],
                XA_ATOM,
                32,
                PropModeReplace,
                self.netatom.as_ptr() as *const c_uchar,
                NET_LAST as c_int,
            );
            XDeleteProperty(self.dpy, self.root, self.netatom[NET_CLIENT_LIST]);
            /* select events */
            let mut wa: XSetWindowAttributes = mem::zeroed();
            wa.cursor = self.cursor[CUR_NORMAL].cursor;
            wa.event_mask = SubstructureRedirectMask
                | SubstructureNotifyMask
                | ButtonPressMask
                | PointerMotionMask
                | EnterWindowMask
                | LeaveWindowMask
                | StructureNotifyMask
                | PropertyChangeMask;
            XChangeWindowAttributes(self.dpy, self.root, CWEventMask | CWCursor, &mut wa);
            XSelectInput(self.dpy, self.root, wa.event_mask);
        }
        self.grabkeys();
        self.focus(None);
    }

    fn seturgent(&mut self, c: ClientId, urg: bool) {
        self.clients[c].isurgent = urg;
        // SAFETY: wmh is checked for NULL and freed with XFree.
        unsafe {
            let wmh = XGetWMHints(self.dpy, self.clients[c].win);
            if wmh.is_null() {
                return;
            }
            (*wmh).flags = if urg { (*wmh).flags | XUrgencyHint } else { (*wmh).flags & !XUrgencyHint };
            XSetWMHints(self.dpy, self.clients[c].win, wmh);
            XFree(wmh as *mut _);
        }
    }

    /// The circular shift of `tags` by `arg.i` within the normal tags (the
    /// shift-tools patch, scratchpads variant): i > 0 is a left, i < 0 a
    /// right circular shift, and the result is masked to the normal tag
    /// bits. dwm shifts by arg->i as given; reducing it modulo LENGTH(tags)
    /// gives the same rotation and keeps every shift count below 32 for any
    /// configured value.
    fn shifttags(&self, tags: u32, i: i32) -> u32 {
        let n = self.config.tags.len() as u32; /* 1..=31, checked by the config loader */
        let tagbits = self.tagbits();
        let k = i.rem_euclid(n as i32) as u32;
        if k == 0 {
            return tags & tagbits;
        }
        ((tags << k) | (tags >> (n - k))) & tagbits
    }

    /* Sends a window to the next/prev tag */
    pub fn shifttag(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let seltags = self.mons[selmon].seltags;
        let shifted = self.mons[selmon].tagset[seltags] & !self.sptagmask();
        let shifted = Arg::Ui(self.shifttags(shifted, arg.i()));
        self.tag(&shifted);
    }

    /* Navigate to the next/prev tag */
    pub fn shiftview(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let seltags = self.mons[selmon].seltags;
        let shifted = self.mons[selmon].tagset[seltags] & !self.sptagmask();
        let shifted = Arg::Ui(self.shifttags(shifted, arg.i()));
        self.view(&shifted);
    }

    /* Navigate to the next/prev tag that has a client, else moves it to the next/prev tag */
    pub fn shiftviewclients(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let sptagmask = self.sptagmask();
        let mut tagmask = 0;
        let seltags = self.mons[selmon].seltags;
        let mut shifted = self.mons[selmon].tagset[seltags] & !sptagmask;

        let mut c = self.mons[selmon].clients;
        while let Some(k) = c {
            if self.clients[k].tags & sptagmask == 0 {
                tagmask |= self.clients[k].tags;
            }
            c = self.clients[k].next;
        }

        /* dwm shifts until the result hits an occupied tag. The rotation
         * repeats after LENGTH(tags) steps, so stop there instead of spinning
         * when it never hits one: a view of only a scratchpad tag shifts to
         * nothing, and a shift by a multiple of LENGTH(tags) stays put. */
        for _ in 0..self.config.tags.len() {
            shifted = self.shifttags(shifted, arg.i());
            if tagmask == 0 || shifted & tagmask != 0 {
                break;
            }
        }
        self.view(&Arg::Ui(shifted));
    }

    fn showhide(&mut self, c: Option<ClientId>) {
        let Some(c) = c else {
            return;
        };
        if self.isvisible(c) {
            if self.clients[c].tags & self.sptagmask() != 0 && self.clients[c].isfloating {
                let m = &self.mons[self.clients[c].mon];
                let (wx, wy, ww, wh) = (m.wx, m.wy, m.ww, m.wh);
                let cl = &mut self.clients[c];
                cl.x = wx + (ww / 2 - width(cl) / 2);
                cl.y = wy + (wh / 2 - height(cl) / 2);
            }
            /* show clients top down */
            let cl = &self.clients[c];
            // SAFETY: plain Xlib call on a managed window.
            unsafe { XMoveWindow(self.dpy, cl.win, cl.x, cl.y) };
            let m = cl.mon;
            if (self.arrange_fn(m).is_none() || cl.isfloating) && !cl.isfullscreen {
                let (x, y, w, h) = (cl.x, cl.y, cl.w, cl.h);
                self.resize(c, x, y, w, h, false);
            }
            let snext = self.clients[c].snext;
            self.showhide(snext);
        } else {
            /* hide clients bottom up */
            let snext = self.clients[c].snext;
            self.showhide(snext);
            let cl = &self.clients[c];
            // SAFETY: plain Xlib call on a managed window.
            unsafe { XMoveWindow(self.dpy, cl.win, width(cl) * -2, cl.y) };
        }
    }

    pub fn spawn(&mut self, arg: &Arg) {
        let Arg::V(cmd) = arg else {
            return;
        };
        let mut argv = cmd.argv.clone();
        if cmd.name == "dmenucmd" {
            /* dmenumon: the argument after "-m" is the selected monitor */
            if let Some(i) = argv.iter().position(|a| a == "-m") {
                if i + 1 < argv.len() {
                    argv[i + 1] = self.mons[self.selmon].num.to_string();
                }
            }
        }
        let Ok(cargs) = argv.iter().map(|a| CString::new(a.as_str())).collect::<Result<Vec<_>, _>>() else {
            eprintln!("dwmr: spawn: command contains a NUL byte");
            return;
        };
        let Some(first) = cargs.first() else {
            return;
        };
        let mut ptrs: Vec<*const c_char> = cargs.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(ptr::null());

        // SAFETY: after fork() the child only makes async-signal-safe calls
        // (close, setsid, sigaction, execvp) on data prepared before the fork,
        // and never returns.
        unsafe {
            if libc::fork() == 0 {
                if !self.dpy.is_null() {
                    libc::close(XConnectionNumber(self.dpy));
                }
                libc::setsid();

                let mut sa: libc::sigaction = mem::zeroed();
                libc::sigemptyset(&mut sa.sa_mask);
                sa.sa_flags = 0;
                sa.sa_sigaction = libc::SIG_DFL;
                libc::sigaction(libc::SIGCHLD, &sa, ptr::null_mut());

                libc::execvp(first.as_ptr(), ptrs.as_ptr());
                die(&format!("dwmr: execvp '{}' failed:", argv[0]));
            }
        }
    }

    fn stackpos(&self, arg: &Arg) -> i32 {
        let selmon = self.selmon;
        let arg = arg.i();

        if self.mons[selmon].clients.is_none() {
            return -1;
        }

        if isinc(arg) {
            let Some(sel) = self.mons[selmon].sel else {
                return -1;
            };
            let mut i = 0;
            let mut c = self.mons[selmon].clients;
            while let Some(k) = c {
                if k == sel {
                    break;
                }
                if self.isvisible(k) {
                    i += 1;
                }
                c = self.clients[k].next;
            }
            let mut n = i;
            while let Some(k) = c {
                if self.isvisible(k) {
                    n += 1;
                }
                c = self.clients[k].next;
            }
            if n == 0 {
                /* sel is never invisible; guards the division anyway */
                return -1;
            }
            modulo(i + getinc(arg), n)
        } else if arg < 0 {
            let mut i = 0;
            let mut c = self.mons[selmon].clients;
            while let Some(k) = c {
                if self.isvisible(k) {
                    i += 1;
                }
                c = self.clients[k].next;
            }
            (i + arg).max(0)
        } else {
            arg
        }
    }

    pub fn tag(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let tagmask = self.tagmask();
        if let Some(sel) = self.mons[selmon].sel {
            if arg.ui() & tagmask != 0 {
                self.clients[sel].tags = arg.ui() & tagmask;
                self.focus(None);
                self.arrange(Some(selmon));
            }
        }
    }

    pub fn tagmon(&mut self, arg: &Arg) {
        let Some(sel) = self.mons[self.selmon].sel else {
            return;
        };
        if self.mons.len() < 2 {
            return;
        }
        let m = self.dirtomon(arg.i());
        self.sendmon(sel, m);
    }

    pub fn togglebar(&mut self, _arg: &Arg) {
        let selmon = self.selmon;
        self.mons[selmon].showbar = !self.mons[selmon].showbar;
        self.updatebarpos(selmon);
        let m = &self.mons[selmon];
        // SAFETY: barwin is a valid window.
        unsafe { XMoveResizeWindow(self.dpy, m.barwin, m.wx, m.by, m.ww as c_uint, self.bh as c_uint) };
        self.arrange(Some(selmon));
    }

    pub fn togglefloating(&mut self, _arg: &Arg) {
        let selmon = self.selmon;
        let Some(sel) = self.mons[selmon].sel else {
            return;
        };
        if self.clients[sel].isfullscreen {
            /* no support for fullscreen windows */
            return;
        }
        let c = sel;
        let cl = &mut self.clients[c];
        cl.isfloating = !cl.isfloating || cl.isfixed;

        if cl.isfloating {
            /* center if never floated, or if the stored geometry is on
             * another monitor (the client was moved since) */
            let (sfx, sfy, sfw, sfh) = (cl.sfx, cl.sfy, cl.sfw, cl.sfh);
            let (mon, bw) = (cl.mon, cl.bw);
            if sfx == 0 || self.recttomon(sfx, sfy, sfw, sfh) != mon {
                let m = &self.mons[mon];
                let cl = &mut self.clients[c];
                cl.sfx = m.mx + (m.mw - sfw - 2 * bw) / 2;
                cl.sfy = m.my + (m.mh - sfh - 2 * bw) / 2;
            }
            /* restore last known float dimensions */
            let cl = &self.clients[c];
            let (sfx, sfy, sfw, sfh) = (cl.sfx, cl.sfy, cl.sfw, cl.sfh);
            self.resize(c, sfx, sfy, sfw, sfh, false);
        } else {
            /* save last known float dimensions */
            cl.sfx = cl.x;
            cl.sfy = cl.y;
            cl.sfw = cl.w;
            cl.sfh = cl.h;
        }

        self.arrange(Some(selmon));
    }

    pub fn togglefullscr(&mut self, _arg: &Arg) {
        let selmon = self.selmon;
        if let Some(sel) = self.mons[selmon].sel {
            let fullscreen = !self.clients[sel].isfullscreen;
            self.setfullscreen(sel, fullscreen);
        }
    }

    pub fn togglescratch(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let i = arg.ui() as usize;
        let Some(sp) = self.config.scratchpads.get(i) else {
            return;
        };
        let scratchtag = self.sptag(i);
        let sparg = Arg::V(Rc::clone(&sp.cmd));

        let mut found = None;
        let mut c = self.mons[selmon].clients;
        while let Some(k) = c {
            if self.clients[k].tags & scratchtag != 0 {
                found = Some(k);
                break;
            }
            c = self.clients[k].next;
        }
        let seltags = self.mons[selmon].seltags;
        if let Some(c) = found {
            let newtagset = self.mons[selmon].tagset[seltags] ^ scratchtag;
            if newtagset != 0 {
                self.mons[selmon].tagset[seltags] = newtagset;
                self.focus(None);
                self.arrange(Some(selmon));
            }
            if self.isvisible(c) {
                self.focus(Some(c));
                self.restack(selmon);
            }
        } else {
            self.mons[selmon].tagset[seltags] |= scratchtag;
            self.spawn(&sparg);
        }
    }

    pub fn togglesticky(&mut self, _arg: &Arg) {
        let selmon = self.selmon;
        let Some(sel) = self.mons[selmon].sel else {
            return;
        };
        let sticky = !self.clients[sel].issticky;
        self.setsticky(sel, sticky);
        self.arrange(Some(selmon));
    }

    pub fn toggletag(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let Some(sel) = self.mons[selmon].sel else {
            return;
        };
        let newtags = self.clients[sel].tags ^ (arg.ui() & self.tagmask());
        if newtags != 0 {
            self.clients[sel].tags = newtags;
            self.focus(None);
            self.arrange(Some(selmon));
        }
    }

    pub fn toggleview(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let seltags = self.mons[selmon].seltags;
        let newtagset = self.mons[selmon].tagset[seltags] ^ (arg.ui() & self.tagmask());

        if newtagset != 0 {
            self.mons[selmon].tagset[seltags] = newtagset;
            self.focus(None);
            self.arrange(Some(selmon));
        }
    }

    fn unfocus(&mut self, c: Option<ClientId>, setfocus: bool) {
        let Some(c) = c else {
            return;
        };
        self.grabbuttons(c, false);
        // SAFETY: plain Xlib calls on managed/root windows.
        unsafe {
            XSetWindowBorder(self.dpy, self.clients[c].win, self.scheme[SCHEME_NORM][COL_BORDER].pixel);
            if setfocus {
                XSetInputFocus(self.dpy, self.root, RevertToPointerRoot, CurrentTime);
                XDeleteProperty(self.dpy, self.root, self.netatom[NET_ACTIVE_WINDOW]);
            }
        }
    }

    fn unmanage(&mut self, c: ClientId, destroyed: bool) {
        let m = self.clients[c].mon;

        self.detach(c);
        self.detachstack(c);
        if !destroyed {
            let win = self.clients[c].win;
            let mut wc = XWindowChanges {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                border_width: self.clients[c].oldbw,
                sibling: 0,
                stack_mode: 0,
            };
            // SAFETY: the error handler swap protects against a vanished window.
            unsafe {
                XGrabServer(self.dpy); /* avoid race conditions */
                XSetErrorHandler(Some(xerrordummy));
                XSelectInput(self.dpy, win, NoEventMask);
                XConfigureWindow(self.dpy, win, CWBorderWidth as c_uint, &mut wc); /* restore border */
                XUngrabButton(self.dpy, AnyButton as c_uint, AnyModifier, win);
                self.setclientstate(c, WITHDRAWN_STATE);
                XSync(self.dpy, False);
                XSetErrorHandler(Some(xerror));
                XUngrabServer(self.dpy);
            }
        }
        self.free_client(c);
        self.focus(None);
        self.updateclientlist();
        self.arrange(Some(m));
    }

    fn unmapnotify(&mut self, e: &XEvent) {
        let ev: XUnmapEvent = e.into();

        if let Some(c) = self.wintoclient(ev.window) {
            if ev.send_event != 0 {
                self.setclientstate(c, WITHDRAWN_STATE);
            } else {
                self.unmanage(c, false);
            }
        }
    }

    fn updatebars(&mut self) {
        // SAFETY: XSetWindowAttributes is plain data; only the fields named
        // in the value mask are read.
        let mut wa: XSetWindowAttributes = unsafe { mem::zeroed() };
        wa.override_redirect = True;
        wa.background_pixmap = ParentRelative as Pixmap;
        wa.event_mask = ButtonPressMask | ExposureMask;
        let class = c"dwmr";
        let mut ch = XClassHint { res_name: class.as_ptr() as *mut c_char, res_class: class.as_ptr() as *mut c_char };
        for m in self.mons.iter_mut() {
            if m.barwin != 0 {
                continue;
            }
            // SAFETY: wa/ch are initialised; XSetClassHint only reads ch.
            unsafe {
                m.barwin = XCreateWindow(
                    self.dpy,
                    self.root,
                    m.wx,
                    m.by,
                    m.ww as c_uint,
                    self.bh as c_uint,
                    0,
                    XDefaultDepth(self.dpy, self.screen),
                    CopyFromParent as c_uint,
                    XDefaultVisual(self.dpy, self.screen),
                    CWOverrideRedirect | CWBackPixmap | CWEventMask,
                    &mut wa,
                );
                XDefineCursor(self.dpy, m.barwin, self.cursor[CUR_NORMAL].cursor);
                XMapRaised(self.dpy, m.barwin);
                XSetClassHint(self.dpy, m.barwin, &mut ch);
            }
        }
    }

    fn updatebarpos(&mut self, m: MonId) {
        let bh = self.bh;
        let m = &mut self.mons[m];
        m.wy = m.my;
        m.wh = m.mh;
        if m.showbar {
            m.wh -= bh;
            m.by = if m.topbar { m.wy } else { m.wy + m.wh };
            m.wy = if m.topbar { m.wy + bh } else { m.wy };
        } else {
            m.by = -bh;
        }
    }

    fn updateclientlist(&mut self) {
        // SAFETY: plain Xlib property calls; each append is one Window.
        unsafe {
            XDeleteProperty(self.dpy, self.root, self.netatom[NET_CLIENT_LIST]);
            for m in &self.mons {
                let mut c = m.clients;
                while let Some(i) = c {
                    let win = self.clients[i].win;
                    XChangeProperty(
                        self.dpy,
                        self.root,
                        self.netatom[NET_CLIENT_LIST],
                        XA_WINDOW,
                        32,
                        PropModeAppend,
                        &win as *const Window as *const c_uchar,
                        1,
                    );
                    c = self.clients[i].next;
                }
            }
        }
    }

    fn updategeom(&mut self) -> bool {
        let mut dirty = false;

        #[cfg(feature = "xinerama")]
        let xinerama = self.updategeom_xinerama(&mut dirty);
        #[cfg(not(feature = "xinerama"))]
        let xinerama = false;

        if !xinerama {
            /* default monitor setup */
            if self.mons.is_empty() {
                let m = self.createmon();
                self.mons.push(m);
            }
            if self.mons[0].mw != self.sw || self.mons[0].mh != self.sh {
                dirty = true;
                self.mons[0].mw = self.sw;
                self.mons[0].ww = self.sw;
                self.mons[0].mh = self.sh;
                self.mons[0].wh = self.sh;
                self.updatebarpos(0);
            }
        }
        if dirty {
            self.selmon = 0;
            self.selmon = self.wintomon(self.root);
        }
        dirty
    }

    /// The `#ifdef XINERAMA` half of updategeom(). Returns false when
    /// Xinerama is not active (or reports no screens), in which case the
    /// caller falls back to the single monitor setup.
    #[cfg(feature = "xinerama")]
    fn updategeom_xinerama(&mut self, dirty: &mut bool) -> bool {
        // SAFETY: info points to nn screen infos until XFree.
        let unique = unsafe {
            if XineramaIsActive(self.dpy) == 0 {
                return false;
            }
            let mut nn: c_int = 0;
            let info = XineramaQueryScreens(self.dpy, &mut nn);
            /* only consider unique geometries as separate screens */
            let mut unique: Vec<XineramaScreenInfo> = Vec::with_capacity(nn.max(0) as usize);
            if !info.is_null() {
                for i in 0..nn.max(0) as usize {
                    let inf = *info.add(i);
                    if isuniquegeom(&unique, &inf) {
                        unique.push(inf);
                    }
                }
                XFree(info as *mut _);
            }
            unique
        };
        let n = self.mons.len();
        let nn = unique.len();
        if nn == 0 {
            /* dwm would dereference a NULL monitor here; treat it as no Xinerama */
            return false;
        }

        /* new monitors if nn > n */
        for _ in n..nn {
            let m = self.createmon();
            self.mons.push(m);
        }
        for (i, u) in unique.iter().enumerate().take(self.mons.len()) {
            let m = &self.mons[i];
            if i >= n
                || u.x_org as i32 != m.mx
                || u.y_org as i32 != m.my
                || u.width as i32 != m.mw
                || u.height as i32 != m.mh
            {
                *dirty = true;
                let m = &mut self.mons[i];
                m.num = i as i32;
                m.mx = u.x_org as i32;
                m.wx = m.mx;
                m.my = u.y_org as i32;
                m.wy = m.my;
                m.mw = u.width as i32;
                m.ww = m.mw;
                m.mh = u.height as i32;
                m.wh = m.mh;
                self.updatebarpos(i);
            }
        }
        /* removed monitors if n > nn */
        for _ in nn..n {
            let m = self.mons.len() - 1;
            while let Some(c) = self.mons[m].clients {
                *dirty = true;
                self.mons[m].clients = self.clients[c].next;
                self.detachstack(c);
                self.clients[c].mon = 0;
                self.attach(c);
                self.attachstack(c);
            }
            if m == self.selmon {
                self.selmon = 0;
            }
            self.cleanupmon(m);
        }
        true
    }

    /// Write `_NET_WM_STATE` as the list of the states dwm manages, so that
    /// setting one state does not clear the other.
    fn updatenetwmstate(&mut self, c: ClientId) {
        let mut state = [0 as Atom; 2];
        let mut n = 0;

        if self.clients[c].isfullscreen {
            state[n] = self.netatom[NET_WM_FULLSCREEN];
            n += 1;
        }
        if self.clients[c].issticky {
            state[n] = self.netatom[NET_WM_STICKY];
            n += 1;
        }
        // SAFETY: state holds n initialised Atoms (n <= 2); an empty property
        // (n == 0) is allowed by XChangeProperty.
        unsafe {
            XChangeProperty(
                self.dpy,
                self.clients[c].win,
                self.netatom[NET_WM_STATE],
                XA_ATOM,
                32,
                PropModeReplace,
                state.as_ptr() as *const c_uchar,
                n as c_int,
            );
        }
    }

    fn updatenumlockmask(&mut self) {
        self.numlockmask = 0;
        // SAFETY: modmap is checked for NULL, read within its bounds and freed once.
        unsafe {
            let modmap = XGetModifierMapping(self.dpy);
            if modmap.is_null() {
                return;
            }
            let max_keypermod = (*modmap).max_keypermod.max(0) as usize;
            let numlock = XKeysymToKeycode(self.dpy, XK_Num_Lock as KeySym);
            for i in 0..8 {
                for j in 0..max_keypermod {
                    if *(*modmap).modifiermap.add(i * max_keypermod + j) == numlock {
                        self.numlockmask = 1 << i;
                    }
                }
            }
            XFreeModifiermap(modmap);
        }
    }

    fn updatesizehints(&mut self, c: ClientId) {
        let mut msize: c_long = 0;
        // SAFETY: XSizeHints is plain data; it is filled by Xlib or flagged
        // as unset below.
        let mut size: XSizeHints = unsafe { mem::zeroed() };

        // SAFETY: size/msize are valid out-pointers.
        if unsafe { XGetWMNormalHints(self.dpy, self.clients[c].win, &mut size, &mut msize) } == 0 {
            /* size is uninitialized, ensure that size.flags aren't used */
            size.flags = PSize;
        }
        let cl = &mut self.clients[c];
        if size.flags & PBaseSize != 0 {
            cl.basew = size.base_width;
            cl.baseh = size.base_height;
        } else if size.flags & PMinSize != 0 {
            cl.basew = size.min_width;
            cl.baseh = size.min_height;
        } else {
            cl.basew = 0;
            cl.baseh = 0;
        }
        if size.flags & PResizeInc != 0 {
            cl.incw = size.width_inc;
            cl.inch = size.height_inc;
        } else {
            cl.incw = 0;
            cl.inch = 0;
        }
        if size.flags & PMaxSize != 0 {
            cl.maxw = size.max_width;
            cl.maxh = size.max_height;
        } else {
            cl.maxw = 0;
            cl.maxh = 0;
        }
        if size.flags & PMinSize != 0 {
            cl.minw = size.min_width;
            cl.minh = size.min_height;
        } else if size.flags & PBaseSize != 0 {
            cl.minw = size.base_width;
            cl.minh = size.base_height;
        } else {
            cl.minw = 0;
            cl.minh = 0;
        }
        if size.flags & PAspect != 0 {
            cl.mina = size.min_aspect.y as f32 / size.min_aspect.x as f32;
            cl.maxa = size.max_aspect.x as f32 / size.max_aspect.y as f32;
        } else {
            cl.maxa = 0.0;
            cl.mina = 0.0;
        }
        cl.isfixed = cl.maxw != 0 && cl.maxh != 0 && cl.maxw == cl.minw && cl.maxh == cl.minh;
        cl.hintsvalid = true;
    }

    fn updatestatus(&mut self) {
        if !Self::gettextprop(self.dpy, self.root, XA_WM_NAME, &mut self.stext, NAME_SIZE) {
            self.stext = format!("dwmr-{}", VERSION);
        }
        self.drawbar(self.selmon);
    }

    fn updatetitle(&mut self, c: ClientId) {
        let win = self.clients[c].win;
        if !Self::gettextprop(self.dpy, win, self.netatom[NET_WM_NAME], &mut self.clients[c].name, NAME_SIZE) {
            Self::gettextprop(self.dpy, win, XA_WM_NAME, &mut self.clients[c].name, NAME_SIZE);
        }
        if self.clients[c].name.is_empty() {
            /* hack to mark broken clients */
            self.clients[c].name = BROKEN.to_string();
        }
    }

    fn updatewindowtype(&mut self, c: ClientId) {
        let state = self.getatomprop(c, self.netatom[NET_WM_STATE]);
        let wtype = self.getatomprop(c, self.netatom[NET_WM_WINDOW_TYPE]);

        if state == self.netatom[NET_WM_FULLSCREEN] {
            self.setfullscreen(c, true);
        }
        if state == self.netatom[NET_WM_STICKY] {
            self.setsticky(c, true);
        }
        if wtype == self.netatom[NET_WM_WINDOW_TYPE_DIALOG] {
            self.clients[c].isfloating = true;
        }
    }

    fn updatewmhints(&mut self, c: ClientId) {
        let win = self.clients[c].win;
        // SAFETY: wmh is checked for NULL and freed with XFree.
        unsafe {
            let wmh = XGetWMHints(self.dpy, win);
            if !wmh.is_null() {
                if Some(c) == self.mons[self.selmon].sel && (*wmh).flags & XUrgencyHint != 0 {
                    (*wmh).flags &= !XUrgencyHint;
                    XSetWMHints(self.dpy, win, wmh);
                } else {
                    self.clients[c].isurgent = (*wmh).flags & XUrgencyHint != 0;
                }
                if (*wmh).flags & InputHint != 0 {
                    self.clients[c].neverfocus = (*wmh).input == 0;
                } else {
                    self.clients[c].neverfocus = false;
                }
                XFree(wmh as *mut _);
            }
        }
    }

    pub fn view(&mut self, arg: &Arg) {
        let selmon = self.selmon;
        let tagmask = self.tagmask();
        let seltags = self.mons[selmon].seltags;
        if (arg.ui() & tagmask) == self.mons[selmon].tagset[seltags] {
            return;
        }
        self.mons[selmon].seltags ^= 1; /* toggle sel tagset */
        if arg.ui() & tagmask != 0 {
            let seltags = self.mons[selmon].seltags;
            self.mons[selmon].tagset[seltags] = arg.ui() & tagmask;
        }
        self.focus(None);
        self.arrange(Some(selmon));
    }

    fn wintoclient(&self, w: Window) -> Option<ClientId> {
        for m in &self.mons {
            let mut c = m.clients;
            while let Some(i) = c {
                if self.clients[i].win == w {
                    return Some(i);
                }
                c = self.clients[i].next;
            }
        }
        None
    }

    fn wintomon(&self, w: Window) -> MonId {
        if w == self.root {
            if let Some((x, y)) = self.getrootptr() {
                return self.recttomon(x, y, 1, 1);
            }
        }
        for (i, m) in self.mons.iter().enumerate() {
            if w == m.barwin {
                return i;
            }
        }
        if let Some(c) = self.wintoclient(w) {
            return self.clients[c].mon;
        }
        self.selmon
    }

    pub fn zoom(&mut self, _arg: &Arg) {
        let selmon = self.selmon;
        let Some(mut c) = self.mons[selmon].sel else {
            return;
        };

        if self.arrange_fn(selmon).is_none() || self.clients[c].isfloating {
            return;
        }
        if Some(c) == self.nexttiled(self.mons[selmon].clients) {
            match self.nexttiled(self.clients[c].next) {
                Some(next) => c = next,
                None => return,
            }
        }
        self.pop(c);
    }
}

/* There's no way to check accesses to destroyed windows, thus those cases are
 * ignored (especially on UnmapNotify's). Other types of errors call Xlibs
 * default error handler, which may call exit. */
unsafe extern "C" fn xerror(dpy: *mut Display, ee: *mut XErrorEvent) -> c_int {
    let e = &*ee;
    if e.error_code == BadWindow
        || (e.request_code == X_SET_INPUT_FOCUS && e.error_code == BadMatch)
        || (e.request_code == X_POLY_TEXT8 && e.error_code == BadDrawable)
        || (e.request_code == X_POLY_FILL_RECTANGLE && e.error_code == BadDrawable)
        || (e.request_code == X_POLY_SEGMENT && e.error_code == BadDrawable)
        || (e.request_code == X_CONFIGURE_WINDOW && e.error_code == BadMatch)
        || (e.request_code == X_GRAB_BUTTON && e.error_code == BadAccess)
        || (e.request_code == X_GRAB_KEY && e.error_code == BadAccess)
        || (e.request_code == X_COPY_AREA && e.error_code == BadDrawable)
    {
        return 0;
    }
    eprintln!("dwmr: fatal error: request code={}, error code={}", e.request_code, e.error_code);
    match XERRORXLIB.get().copied().flatten() {
        Some(xerrorxlib) => xerrorxlib(dpy, ee), /* may call exit */
        None => 0,
    }
}

unsafe extern "C" fn xerrordummy(_dpy: *mut Display, _ee: *mut XErrorEvent) -> c_int {
    0
}

/* Startup Error handler to check if another window manager
 * is already running. */
unsafe extern "C" fn xerrorstart(_dpy: *mut Display, _ee: *mut XErrorEvent) -> c_int {
    die("dwmr: another window manager is already running");
}
