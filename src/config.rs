/* See LICENSE file for copyright and license details. */
//! Configuration.
//!
//! The compiled-in defaults ([`Config::default`]) are dwm's config.def.h. At
//! startup `$XDG_CONFIG_HOME/dwmr/config.toml` (usually
//! `~/.config/dwmr/config.toml`) is read on top of them; every key in it is
//! optional and falls back to the default. A config file that cannot be parsed
//! is reported on stderr and ignored so that the window manager still starts.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;

use serde::Deserialize;
use x11::keysym::*;
use x11::xlib::{self, KeySym, XStringToKeysym};

use crate::dwm::vanitygaps::GAP_MAX;
use crate::dwm::{inc, Dwm, MonId};

/* enums */
pub const SCHEME_NORM: usize = 0;
pub const SCHEME_SEL: usize = 1; /* color schemes */
pub const CLK_TAG_BAR: u32 = 0;
pub const CLK_LT_SYMBOL: u32 = 1;
pub const CLK_STATUS_TEXT: u32 = 2;
pub const CLK_CLIENT_WIN: u32 = 3;
pub const CLK_ROOT_WIN: u32 = 4; /* clicks */

/// A key/button action, e.g. `Dwm::spawn`.
pub type KeyFn = fn(&mut Dwm, &Arg);
/// A layout's arrange function, e.g. `Dwm::tile`.
pub type ArrangeFn = fn(&mut Dwm, MonId);

/// A command to `spawn`: the `argv` vector plus the name it was given in the
/// config (`dmenucmd`, `termcmd`, ...).
#[derive(Debug, PartialEq)]
pub struct Command {
    pub name: String,
    pub argv: Vec<String>,
}

/// The argument passed to a key/button function; dwm's `Arg` union.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Arg {
    /// `{0}`
    #[default]
    None,
    /// `.i`
    I(i32),
    /// `.ui`
    Ui(u32),
    /// `.f`
    F(f32),
    /// `.v = cmd`
    V(Rc<Command>),
    /// `.v = &layouts[n]`
    Layout(usize),
}

impl Arg {
    pub fn i(&self) -> i32 {
        match self {
            Arg::I(i) => *i,
            _ => 0,
        }
    }

    pub fn ui(&self) -> u32 {
        match self {
            Arg::Ui(ui) => *ui,
            _ => 0,
        }
    }

    pub fn f(&self) -> f32 {
        match self {
            Arg::F(f) => *f,
            _ => 0.0,
        }
    }

    /// `arg.i == 0` for a `{0}` argument.
    pub fn is_zero(&self) -> bool {
        matches!(self, Arg::None | Arg::I(0) | Arg::Ui(0))
    }
}

pub struct Button {
    pub click: u32,
    pub mask: u32,
    pub button: u32,
    pub func: KeyFn,
    pub arg: Arg,
}

pub struct Key {
    pub mod_: u32,
    pub keysym: KeySym,
    pub func: KeyFn,
    pub arg: Arg,
}

#[derive(Clone)]
pub struct Layout {
    pub symbol: String,
    pub arrange: Option<ArrangeFn>,
}

/// A scratchpad (dwm's scratchpads patch): a named command whose window is
/// tagged with its own tag bit, `SPTAG(i)`, above the normal tags.
#[derive(Clone, Debug, PartialEq)]
pub struct Scratchpad {
    pub name: String,
    pub cmd: Rc<Command>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub class: Option<String>,
    pub instance: Option<String>,
    pub title: Option<String>,
    pub tags: u32,
    pub isfloating: bool,
    pub isterminal: bool,
    pub noswallow: bool,
    pub monitor: i32,
}

/// The colors of the status text codes (status2d): the text starts in col1,
/// ^3^..^6^ select col3..col6 and ^2^ the weather color from the temperature
/// that follows it: +20 and above col21, below +20 col22, negative col23,
/// none col24.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusColors {
    pub col1: String,
    pub col21: String,
    pub col22: String,
    pub col23: String,
    pub col24: String,
    pub col3: String,
    pub col4: String,
    pub col5: String,
    pub col6: String,
}

pub struct Config {
    /* appearance */
    pub borderpx: u32, /* border pixel of windows */
    pub gappih: u32,   /* horiz inner gap between windows */
    pub gappiv: u32,   /* vert inner gap between windows */
    pub gappoh: u32,   /* horiz outer gap between windows and screen edge */
    pub gappov: u32,   /* vert outer gap between windows and screen edge */
    pub smartgaps: bool, /* true means no outer gap when there is only one window */
    pub browsergaps: bool, /* false means no outer gap when there is only one window and it is firefox */
    pub snap: u32,     /* snap pixel */
    pub swallowfloating: bool, /* true means swallow floating windows by default */
    pub showbar: bool, /* false means no bar */
    pub topbar: bool,  /* false means bottom bar */
    pub focusonwheel: bool, /* false allows the user to scroll window without changing focus */
    pub fonts: Vec<String>,
    /// Bigger font for status text between ^B^ and ^N^, e.g. a block's icon
    /// (status2d); empty means none.
    pub statusbigfonts: Vec<String>,
    /// `[SchemeNorm, SchemeSel]`, each `[fg, bg, border]`.
    pub colors: Vec<Vec<String>>,
    /// The colors the status text codes select (status2d).
    pub statuscolors: StatusColors,

    /* tagging */
    pub tags: Vec<String>,
    pub scratchpads: Vec<Scratchpad>,
    pub rules: Vec<Rule>,

    /* layout(s) */
    pub mfact: f32,           /* factor of master area size [0.05..0.95] */
    pub nmaster: i32,         /* number of clients in master area */
    pub resizehints: bool,    /* true means respect size hints in tiled resizals */
    pub lockfullscreen: bool, /* true will force focus on the fullscreen window */
    pub refreshrate: u32,     /* refresh rate (per second) for client move/resize */
    pub layouts: Vec<Layout>,

    /* key definitions */
    pub modkey: u32,
    pub commands: Vec<Rc<Command>>,
    /// The status bar program that sigstatusbar() signals (statuscmd's
    /// `STATUSBAR`), found by process name.
    pub statusbar: String,
    /// The command runautostart() runs once at startup, after scan(); empty
    /// runs none.
    pub autostart: Vec<String>,
    pub keys: Vec<Key>,
    pub buttons: Vec<Button>,
}

/// `TAGKEYS(KEY, TAG)`.
fn tagkeys(keys: &mut Vec<Key>, modkey: u32, key: KeySym, tag: u32) {
    let tag = Arg::Ui(1 << tag);
    keys.push(Key { mod_: modkey, keysym: key, func: Dwm::view, arg: tag.clone() });
    keys.push(Key { mod_: modkey | xlib::ControlMask, keysym: key, func: Dwm::tag, arg: tag.clone() });
    keys.push(Key { mod_: modkey | xlib::ShiftMask, keysym: key, func: Dwm::tagview, arg: tag.clone() });
    keys.push(Key { mod_: modkey | xlib::ControlMask | xlib::ShiftMask, keysym: key, func: Dwm::toggleview, arg: tag });
}

impl Default for Config {
    /// dwm's config.def.h.
    fn default() -> Self {
        /* appearance */
        let dmenufont = "monospace:size=10";
        let col_gray1 = "#222222";
        let col_gray2 = "#444444";
        let col_gray3 = "#bbbbbb";
        let col_gray4 = "#eeeeee";
        let col_cyan = "#005577";
        let colors = vec![
            /*               fg         bg         border   */
            /* SchemeNorm */ vec![col_gray3.into(), col_gray1.into(), col_gray2.into()],
            /* SchemeSel  */ vec![col_gray4.into(), col_cyan.into(), col_cyan.into()],
        ];

        /* tagging */
        let tags: Vec<String> = ["1", "2", "3", "4", "5", "6", "7", "8", "9"].iter().map(|s| s.to_string()).collect();

        let sp = |name: &str, argv: &[&str]| Scratchpad {
            name: name.to_string(),
            cmd: Rc::new(Command { name: name.to_string(), argv: argv.iter().map(|s| s.to_string()).collect() }),
        };
        let scratchpads = vec![
            /* name        cmd */
            sp("spterm", &["st", "-n", "spterm", "-e", "python3"]),
            sp("spcalc", &["st", "-n", "spcalc"]),
        ];
        /* SPTAG(i) */
        let sptag = |i: u32| (1u32 << tags.len()) << i;

        let rules = vec![
            /* xprop(1):
             *	WM_CLASS(STRING) = instance, class
             *	WM_NAME(STRING) = title
             */
            /* class      instance    title       tags mask     isfloating   isterminal   noswallow   monitor */
            Rule { class: Some("Gimp".into()), instance: None, title: None, tags: 0, isfloating: true, isterminal: false, noswallow: false, monitor: -1 },
            Rule { class: Some("Firefox".into()), instance: None, title: None, tags: 1 << 8, isfloating: false, isterminal: false, noswallow: false, monitor: -1 },
            Rule { class: Some("wezterm".into()), instance: None, title: None, tags: 0, isfloating: false, isterminal: true, noswallow: false, monitor: -1 },
            Rule { class: None, instance: None, title: Some("Event Tester".into()), tags: 0, isfloating: false, isterminal: false, noswallow: true, monitor: -1 }, /* xev */
            Rule { class: None, instance: Some("spterm".into()), title: None, tags: sptag(0), isfloating: true, isterminal: true, noswallow: true, monitor: -1 },
            Rule { class: None, instance: Some("spcalc".into()), title: None, tags: sptag(1), isfloating: true, isterminal: true, noswallow: false, monitor: -1 },
        ];

        /* layout(s) */
        let layouts = vec![
            /* symbol     arrange function */
            Layout { symbol: "[@]".into(), arrange: Some(Dwm::spiral) }, /* first entry is default */
            Layout { symbol: "[]=".into(), arrange: Some(Dwm::tile) },
            Layout { symbol: "TTT".into(), arrange: Some(Dwm::bstack) },
            Layout { symbol: "[\\]".into(), arrange: Some(Dwm::dwindle) },
            Layout { symbol: "[D]".into(), arrange: Some(Dwm::deck) },
            Layout { symbol: "[M]".into(), arrange: Some(Dwm::monocle) },
            Layout { symbol: "|M|".into(), arrange: Some(Dwm::centeredmaster) },
            Layout { symbol: ">M>".into(), arrange: Some(Dwm::centeredfloatingmaster) },
            Layout { symbol: "><>".into(), arrange: None }, /* no layout function means floating behavior */
        ];

        /* key definitions */
        let modkey = xlib::Mod1Mask;

        /* commands */
        let dmenucmd = Rc::new(Command {
            name: "dmenucmd".into(),
            argv: ["dmenu_run", "-m", "0", "-fn", dmenufont, "-nb", col_gray1, "-nf", col_gray3, "-sb", col_cyan, "-sf", col_gray4]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        });
        let termcmd = Rc::new(Command { name: "termcmd".into(), argv: vec!["st".into()] });

        let k = |mod_: u32, keysym: u32, func: KeyFn, arg: Arg| Key { mod_, keysym: keysym as KeySym, func, arg };
        let mut keys = vec![
            /* modifier                     key        function        argument */
            k(modkey, XK_p, Dwm::spawn, Arg::V(dmenucmd.clone())),
            k(modkey | xlib::ShiftMask, XK_Return, Dwm::spawn, Arg::V(termcmd.clone())),
            k(modkey, XK_b, Dwm::togglebar, Arg::None),
            k(modkey, XK_j, Dwm::focusstack, Arg::I(inc(1))),
            k(modkey, XK_k, Dwm::focusstack, Arg::I(inc(-1))),
            k(modkey | xlib::ControlMask, XK_j, Dwm::focusstack, Arg::I(-1)),
            k(modkey | xlib::ControlMask, XK_k, Dwm::focusstack, Arg::I(0)),
            k(modkey | xlib::ShiftMask, XK_j, Dwm::pushstack, Arg::I(inc(1))),
            k(modkey | xlib::ShiftMask, XK_k, Dwm::pushstack, Arg::I(inc(-1))),
            k(modkey | xlib::ShiftMask | xlib::ControlMask, XK_j, Dwm::pushstack, Arg::I(-1)),
            k(modkey | xlib::ShiftMask | xlib::ControlMask, XK_k, Dwm::pushstack, Arg::I(0)),
            k(modkey, XK_i, Dwm::incnmaster, Arg::I(1)),
            k(modkey, XK_d, Dwm::incnmaster, Arg::I(-1)),
            k(modkey, XK_y, Dwm::setmfact, Arg::F(-0.05)),
            k(modkey, XK_o, Dwm::setmfact, Arg::F(0.05)),
            k(modkey, XK_x, Dwm::defaultgaps, Arg::None),
            k(modkey, XK_z, Dwm::togglegaps, Arg::None),
            k(modkey | xlib::ControlMask, XK_z, Dwm::togglebgaps, Arg::None),
            k(modkey, XK_plus, Dwm::incrgaps, Arg::I(3)),
            k(modkey, XK_minus, Dwm::incrgaps, Arg::I(-3)),
            k(modkey | xlib::ShiftMask, XK_plus, Dwm::incrgaps, Arg::I(1)),
            k(modkey | xlib::ShiftMask, XK_minus, Dwm::incrgaps, Arg::I(-1)),
            k(modkey, XK_Return, Dwm::zoom, Arg::None),
            k(modkey, XK_Tab, Dwm::shiftviewclients, Arg::I(1)),
            k(modkey | xlib::ShiftMask, XK_Tab, Dwm::shiftviewclients, Arg::I(-1)),
            k(modkey | xlib::ShiftMask, XK_c, Dwm::killclient, Arg::None),
            k(modkey, XK_t, Dwm::setlayout, Arg::Layout(1)),
            k(modkey, XK_f, Dwm::setlayout, Arg::Layout(8)),
            k(modkey, XK_m, Dwm::setlayout, Arg::Layout(5)),
            k(modkey, XK_space, Dwm::setlayout, Arg::None),
            k(modkey | xlib::ShiftMask, XK_space, Dwm::togglefloating, Arg::None),
            k(modkey | xlib::ShiftMask, XK_f, Dwm::togglefullscr, Arg::None),
            k(modkey | xlib::ShiftMask, XK_less, Dwm::togglesticky, Arg::None),
            k(modkey | xlib::ShiftMask, XK_y, Dwm::shifttag, Arg::I(1)),
            k(modkey | xlib::ShiftMask, XK_o, Dwm::shifttag, Arg::I(-1)),
            k(modkey, XK_0, Dwm::view, Arg::Ui(!0)),
            k(modkey | xlib::ShiftMask, XK_0, Dwm::tag, Arg::Ui(!0)),
            k(modkey, XK_comma, Dwm::focusmon, Arg::I(-1)),
            k(modkey, XK_period, Dwm::focusmon, Arg::I(1)),
            k(modkey | xlib::ShiftMask, XK_comma, Dwm::tagmon, Arg::I(-1)),
            k(modkey | xlib::ShiftMask, XK_period, Dwm::tagmon, Arg::I(1)),
            k(modkey, XK_h, Dwm::focusmon, Arg::I(-1)),
            k(modkey, XK_l, Dwm::focusmon, Arg::I(1)),
            k(modkey | xlib::ShiftMask, XK_h, Dwm::tagmonview, Arg::I(-1)),
            k(modkey | xlib::ShiftMask, XK_l, Dwm::tagmonview, Arg::I(1)),
            k(modkey | xlib::ControlMask, XK_h, Dwm::tagmon, Arg::I(-1)),
            k(modkey | xlib::ControlMask, XK_l, Dwm::tagmon, Arg::I(1)),
            k(modkey, XK_Left, Dwm::focusmon, Arg::I(-1)),
            k(modkey, XK_Right, Dwm::focusmon, Arg::I(1)),
            k(modkey | xlib::ShiftMask, XK_Left, Dwm::tagmon, Arg::I(-1)),
            k(modkey | xlib::ShiftMask, XK_Right, Dwm::tagmon, Arg::I(1)),
            k(modkey | xlib::ShiftMask, XK_p, Dwm::togglebars, Arg::None),
            k(modkey | xlib::ControlMask | xlib::ShiftMask, XK_p, Dwm::togglebar, Arg::None),
            k(modkey, XK_apostrophe, Dwm::togglescratch, Arg::Ui(0)),
            k(modkey | xlib::ShiftMask, XK_apostrophe, Dwm::togglescratch, Arg::Ui(1)),
        ];
        for (tag, key) in [XK_1, XK_2, XK_3, XK_4, XK_5, XK_6, XK_7, XK_8, XK_9].iter().enumerate() {
            tagkeys(&mut keys, modkey, *key as KeySym, tag as u32);
        }
        keys.push(k(modkey | xlib::ShiftMask, XK_q, Dwm::quit, Arg::None));

        /* button definitions */
        /* click can be ClkTagBar, ClkLtSymbol, ClkStatusText, ClkClientWin, or ClkRootWin */
        let b = |click: u32, mask: u32, button: u32, func: KeyFn, arg: Arg| Button { click, mask, button, func, arg };
        let buttons = vec![
            /* click                event mask      button          function        argument */
            b(CLK_LT_SYMBOL, 0, xlib::Button1, Dwm::setlayout, Arg::None),
            b(CLK_LT_SYMBOL, 0, xlib::Button3, Dwm::setlayout, Arg::Layout(5)),
            b(CLK_STATUS_TEXT, 0, xlib::Button1, Dwm::sigstatusbar, Arg::I(1)),
            b(CLK_STATUS_TEXT, 0, xlib::Button2, Dwm::sigstatusbar, Arg::I(2)),
            b(CLK_STATUS_TEXT, 0, xlib::Button3, Dwm::sigstatusbar, Arg::I(3)),
            b(CLK_CLIENT_WIN, modkey, xlib::Button1, Dwm::movemouse, Arg::None),
            b(CLK_CLIENT_WIN, modkey, xlib::Button2, Dwm::defaultgaps, Arg::None),
            b(CLK_CLIENT_WIN, modkey, xlib::Button3, Dwm::resizemouse, Arg::None),
            b(CLK_CLIENT_WIN, modkey, xlib::Button4, Dwm::incrgaps, Arg::I(1)),
            b(CLK_CLIENT_WIN, modkey, xlib::Button5, Dwm::incrgaps, Arg::I(-1)),
            b(CLK_ROOT_WIN, 0, xlib::Button2, Dwm::togglebar, Arg::None),
            b(CLK_TAG_BAR, 0, xlib::Button1, Dwm::view, Arg::None),
            b(CLK_TAG_BAR, 0, xlib::Button3, Dwm::toggleview, Arg::None),
            b(CLK_TAG_BAR, modkey, xlib::Button1, Dwm::tag, Arg::None),
            b(CLK_TAG_BAR, modkey, xlib::Button3, Dwm::toggletag, Arg::None),
            b(CLK_TAG_BAR, 0, xlib::Button4, Dwm::shiftview, Arg::I(-1)),
            b(CLK_TAG_BAR, 0, xlib::Button5, Dwm::shiftview, Arg::I(1)),
        ];

        Config {
            borderpx: 1,
            gappih: 20,
            gappiv: 20,
            gappoh: 20,
            gappov: 20,
            smartgaps: false,
            browsergaps: false,
            snap: 32,
            swallowfloating: false,
            showbar: true,
            topbar: true,
            focusonwheel: false,
            fonts: vec!["monospace:size=10".into()],
            statusbigfonts: vec!["monospace:size=14".into()],
            colors,
            statuscolors: StatusColors {
                col1: "#98971a".into(),
                col21: "#fb4934".into(),
                col22: "#ebdbb2".into(),
                col23: "#458588".into(),
                col24: "#ebdbb2".into(),
                col3: "#fabd2f".into(),
                col4: "#83a598".into(),
                col5: "#d3869b".into(),
                col6: "#8ec07c".into(),
            },
            tags,
            scratchpads,
            rules,
            mfact: 0.55,
            nmaster: 1,
            resizehints: true,
            lockfullscreen: true,
            refreshrate: 120,
            layouts,
            modkey,
            commands: vec![dmenucmd, termcmd],
            statusbar: "dwmblocksr".into(),
            autostart: ["sh", "-c", "killall -q dwmblocksr; dwmblocksr &"].iter().map(|s| s.to_string()).collect(),
            keys,
            buttons,
        }
    }
}

/* ---------------------------------------------------------------------- */
/* config file loading                                                    */

/// `$XDG_CONFIG_HOME/dwmr/config.toml`, or `$HOME/.config/dwmr/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("dwmr").join("config.toml"))
}

/// Load the configuration: the config file if it exists and is valid, the
/// built-in defaults otherwise.
pub fn load() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    if !path.is_file() {
        return Config::default();
    }
    let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| parse(&text).map_err(String::from));
    match result {
        Ok(config) => config,
        Err(err) => {
            eprintln!("dwmr: {}: {}", path.display(), err);
            eprintln!("dwmr: using the built-in default configuration");
            Config::default()
        }
    }
}

#[derive(Debug)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<ConfigError> for String {
    fn from(e: ConfigError) -> String {
        e.0
    }
}

fn err<T>(msg: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError(msg.into()))
}

/* The raw (serde) shape of config.toml. Everything is optional; missing
 * values keep the default. */

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawConfig {
    /* appearance */
    borderpx: Option<u32>,
    gappih: Option<u32>,
    gappiv: Option<u32>,
    gappoh: Option<u32>,
    gappov: Option<u32>,
    smartgaps: Option<bool>,
    browsergaps: Option<bool>,
    snap: Option<u32>,
    swallowfloating: Option<bool>,
    showbar: Option<bool>,
    topbar: Option<bool>,
    focusonwheel: Option<bool>,
    fonts: Option<Vec<String>>,
    statusbigfonts: Option<Vec<String>>,
    colors: Option<RawColors>,
    statuscolors: Option<RawStatusColors>,
    /* tagging */
    tags: Option<Vec<String>>,
    scratchpads: Option<Vec<RawScratchpad>>,
    rules: Option<Vec<RawRule>>,
    /* layout(s) */
    mfact: Option<f32>,
    nmaster: Option<i32>,
    resizehints: Option<bool>,
    lockfullscreen: Option<bool>,
    refreshrate: Option<u32>,
    layouts: Option<Vec<RawLayout>>,
    /* key definitions */
    modkey: Option<String>,
    /* commands */
    commands: Option<BTreeMap<String, Vec<String>>>,
    statusbar: Option<String>,
    autostart: Option<Vec<String>>,
    keys: Option<Vec<RawKeyEntry>>,
    /* button definitions */
    buttons: Option<Vec<RawButton>>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawColors {
    norm: Option<[String; 3]>,
    sel: Option<[String; 3]>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawStatusColors {
    col1: Option<String>,
    col21: Option<String>,
    col22: Option<String>,
    col23: Option<String>,
    col24: Option<String>,
    col3: Option<String>,
    col4: Option<String>,
    col5: Option<String>,
    col6: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScratchpad {
    name: String,
    cmd: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawRule {
    class: Option<String>,
    instance: Option<String>,
    title: Option<String>,
    tags: Option<UintExpr>,
    isfloating: Option<bool>,
    isterminal: Option<bool>,
    noswallow: Option<bool>,
    monitor: Option<i32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLayout {
    symbol: String,
    arrange: Option<String>,
}

/// An entry of `keys`: either a plain key binding or a `TAGKEYS(KEY, TAG)`
/// expansion (`{ tagkeys = "1", tag = 0 }`).
#[derive(Deserialize)]
#[serde(untagged)]
enum RawKeyEntry {
    Key(RawKey),
    TagKeys(RawTagKeys),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKey {
    #[serde(rename = "mod")]
    mod_: Option<String>,
    key: String,
    func: String,
    arg: Option<RawArg>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTagKeys {
    tagkeys: String,
    tag: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawButton {
    click: String,
    mask: Option<String>,
    button: UintExpr,
    func: String,
    arg: Option<RawArg>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawArg {
    i: Option<IntExpr>,
    ui: Option<UintExpr>,
    f: Option<f32>,
    v: Option<String>,
}

/// A signed integer, either a TOML integer or a string such as `"-1"` or
/// `"INC(+1)"` (a relative stack position, dwm's stacker `INC(X)` macro).
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum IntExpr {
    Int(i64),
    Str(String),
}

impl IntExpr {
    fn to_i32(&self) -> Result<i32, ConfigError> {
        match self {
            IntExpr::Int(i) => i32::try_from(*i).or_else(|_| err(format!("integer out of range: {}", i))),
            IntExpr::Str(s) => parse_int_expr(s),
        }
    }
}

/// Parse a decimal integer with an optional sign, or `INC(n)`, into an i32.
pub fn parse_int_expr(s: &str) -> Result<i32, ConfigError> {
    let t = s.trim();
    if let Some(n) = t.strip_prefix("INC(").and_then(|r| r.strip_suffix(')')) {
        let n = n.trim().trim_start_matches('+');
        let n: i32 = n.parse().or_else(|_| err(format!("invalid number in '{}'", t)))?;
        if !(-999..=999).contains(&n) {
            return err(format!("INC() argument out of range in '{}' (-999..=999)", t));
        }
        return Ok(inc(n));
    }
    t.trim_start_matches('+').parse::<i32>().or_else(|_| err(format!("invalid number '{}'", t)))
}

/// An unsigned integer, either a TOML integer or a C-like expression string
/// such as `"1 << 8"`, `"~0"`, `"0x1ff"`, `"1 | 2"` or `"Button1"`.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum UintExpr {
    Int(i64),
    Str(String),
}

impl UintExpr {
    fn to_u32(&self) -> Result<u32, ConfigError> {
        match self {
            UintExpr::Int(i) => u32::try_from(*i).or_else(|_| err(format!("integer out of range: {}", i))),
            UintExpr::Str(s) => parse_uint_expr(s),
        }
    }

    /// Like `to_u32`, also accepting `SPTAG(i)` for the given tag counts.
    fn to_u32_tags(&self, tc: &TagCount) -> Result<u32, ConfigError> {
        match self {
            UintExpr::Int(i) => u32::try_from(*i).or_else(|_| err(format!("integer out of range: {}", i))),
            UintExpr::Str(s) => parse_uint_expr_tags(s, tc),
        }
    }
}

/// The tag counts `SPTAG(i)` needs: `SPTAG(i) = (1 << ntags) << i`.
struct TagCount {
    ntags: usize,
    nscratchpads: usize,
}

/// Parse `a | b`, `a << b`, `~a`, decimal and hex numbers and the `ButtonN`
/// names into a u32, with C semantics (wrapping).
pub fn parse_uint_expr(s: &str) -> Result<u32, ConfigError> {
    parse_uint_expr_tags(s, &TagCount { ntags: 0, nscratchpads: 0 })
}

/// `parse_uint_expr` that also accepts `SPTAG(i)`, a scratchpad tag.
fn parse_uint_expr_tags(s: &str, tc: &TagCount) -> Result<u32, ConfigError> {
    let atom = |t: &str| -> Result<u32, ConfigError> {
        let t = t.trim();
        if let Some(rest) = t.strip_prefix('~') {
            return Ok(!parse_uint_expr_tags(rest, tc)?);
        }
        if let Some(i) = t.strip_prefix("SPTAG(").and_then(|r| r.strip_suffix(')')) {
            let i: usize = i.trim().parse().or_else(|_| err(format!("invalid scratchpad index in '{}'", t)))?;
            if i >= tc.nscratchpads {
                return err(format!("'{}': there are {} scratchpads", t, tc.nscratchpads));
            }
            return Ok((1u32 << tc.ntags) << i);
        }
        if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            return u32::from_str_radix(hex, 16).or_else(|_| err(format!("invalid number '{}'", t)));
        }
        if let Some(n) = t.strip_prefix("Button") {
            return n.parse::<u32>().or_else(|_| err(format!("invalid button '{}'", t)));
        }
        t.parse::<u32>().or_else(|_| err(format!("invalid number '{}'", t)))
    };
    let shift = |t: &str| -> Result<u32, ConfigError> {
        let mut parts = t.split("<<");
        let mut v = atom(parts.next().unwrap_or(""))?;
        for p in parts {
            v = v.checked_shl(atom(p)?).unwrap_or(0);
        }
        Ok(v)
    };
    let mut v = 0u32;
    for term in s.split('|') {
        v |= shift(term)?;
    }
    Ok(v)
}

/// Parse a modifier mask such as `"MODKEY|ShiftMask"`, `"Mod4Mask"` or `"0"`.
fn parse_mask(s: &str, modkey: u32) -> Result<u32, ConfigError> {
    let mut mask = 0;
    for tok in s.split('|') {
        mask |= match tok.trim() {
            "" | "0" => 0,
            "MODKEY" => modkey,
            "ShiftMask" | "Shift" => xlib::ShiftMask,
            "LockMask" | "Lock" => xlib::LockMask,
            "ControlMask" | "Control" => xlib::ControlMask,
            "Mod1Mask" | "Mod1" => xlib::Mod1Mask,
            "Mod2Mask" | "Mod2" => xlib::Mod2Mask,
            "Mod3Mask" | "Mod3" => xlib::Mod3Mask,
            "Mod4Mask" | "Mod4" => xlib::Mod4Mask,
            "Mod5Mask" | "Mod5" => xlib::Mod5Mask,
            other => return err(format!("unknown modifier '{}'", other)),
        };
    }
    Ok(mask)
}

/// `"XK_Return"` / `"Return"` / `"p"` -> KeySym, via XStringToKeysym.
fn parse_keysym(s: &str) -> Result<KeySym, ConfigError> {
    let name = s.trim().strip_prefix("XK_").unwrap_or(s.trim());
    let Ok(cname) = CString::new(name) else {
        return err(format!("invalid key name '{}'", s));
    };
    // SAFETY: XStringToKeysym needs no display and takes a NUL terminated string.
    let sym = unsafe { XStringToKeysym(cname.as_ptr()) };
    if sym == 0 {
        /* NoSymbol */
        return err(format!("unknown key '{}'", s));
    }
    Ok(sym)
}

fn parse_func(name: &str) -> Result<KeyFn, ConfigError> {
    Ok(match name.trim() {
        "defaultgaps" => Dwm::defaultgaps,
        "focusmon" => Dwm::focusmon,
        "focusnthmon" => Dwm::focusnthmon,
        "focusstack" => Dwm::focusstack,
        "incnmaster" => Dwm::incnmaster,
        "incrgaps" => Dwm::incrgaps,
        "killclient" => Dwm::killclient,
        "movemouse" => Dwm::movemouse,
        "pushstack" => Dwm::pushstack,
        "quit" => Dwm::quit,
        "resizemouse" => Dwm::resizemouse,
        "setlayout" => Dwm::setlayout,
        "setmfact" => Dwm::setmfact,
        "shifttag" => Dwm::shifttag,
        "shiftview" => Dwm::shiftview,
        "shiftviewclients" => Dwm::shiftviewclients,
        "sigstatusbar" => Dwm::sigstatusbar,
        "spawn" => Dwm::spawn,
        "tag" => Dwm::tag,
        "tagmon" => Dwm::tagmon,
        "tagmonview" => Dwm::tagmonview,
        "tagnthmonview" => Dwm::tagnthmonview,
        "tagview" => Dwm::tagview,
        "togglebar" => Dwm::togglebar,
        "togglebars" => Dwm::togglebars,
        "togglebgaps" => Dwm::togglebgaps,
        "togglefloating" => Dwm::togglefloating,
        "togglefullscr" => Dwm::togglefullscr,
        "togglegaps" => Dwm::togglegaps,
        "togglescratch" => Dwm::togglescratch,
        "togglesticky" => Dwm::togglesticky,
        "toggletag" => Dwm::toggletag,
        "toggleview" => Dwm::toggleview,
        "view" => Dwm::view,
        "zoom" => Dwm::zoom,
        other => return err(format!("unknown function '{}'", other)),
    })
}

fn parse_arrange(name: Option<&str>) -> Result<Option<ArrangeFn>, ConfigError> {
    Ok(match name.map(str::trim) {
        None | Some("") | Some("none") | Some("NULL") => None,
        Some("tile") => Some(Dwm::tile),
        Some("monocle") => Some(Dwm::monocle),
        Some("bstack") => Some(Dwm::bstack),
        Some("centeredmaster") => Some(Dwm::centeredmaster),
        Some("centeredfloatingmaster") => Some(Dwm::centeredfloatingmaster),
        Some("deck") => Some(Dwm::deck),
        Some("dwindle") => Some(Dwm::dwindle),
        Some("spiral") => Some(Dwm::spiral),
        Some(other) => return err(format!("unknown layout function '{}'", other)),
    })
}

fn parse_click(name: &str) -> Result<u32, ConfigError> {
    Ok(match name.trim() {
        "ClkTagBar" => CLK_TAG_BAR,
        "ClkLtSymbol" => CLK_LT_SYMBOL,
        "ClkStatusText" => CLK_STATUS_TEXT,
        "ClkClientWin" => CLK_CLIENT_WIN,
        "ClkRootWin" => CLK_ROOT_WIN,
        other => return err(format!("unknown click '{}'", other)),
    })
}

fn parse_arg(raw: Option<&RawArg>, commands: &[Rc<Command>], nlayouts: usize, tc: &TagCount) -> Result<Arg, ConfigError> {
    let Some(raw) = raw else {
        return Ok(Arg::None);
    };
    let set = [raw.i.is_some(), raw.ui.is_some(), raw.f.is_some(), raw.v.is_some()].iter().filter(|b| **b).count();
    if set > 1 {
        return err("an argument can only have one of i, ui, f, v");
    }
    if let Some(i) = &raw.i {
        return Ok(Arg::I(i.to_i32()?));
    }
    if let Some(ui) = &raw.ui {
        return Ok(Arg::Ui(ui.to_u32_tags(tc)?));
    }
    if let Some(f) = raw.f {
        return Ok(Arg::F(f));
    }
    if let Some(v) = &raw.v {
        let v = v.trim();
        if let Some(cmd) = commands.iter().find(|c| c.name == v) {
            return Ok(Arg::V(Rc::clone(cmd)));
        }
        if let Some(idx) = v.strip_prefix("layouts[").and_then(|r| r.strip_suffix(']')) {
            let idx: usize = idx.trim().parse().or_else(|_| err(format!("invalid layout reference '{}'", v)))?;
            if idx >= nlayouts {
                return err(format!("layout index out of range: '{}' (there are {} layouts)", v, nlayouts));
            }
            return Ok(Arg::Layout(idx));
        }
        return err(format!("unknown command or layout '{}' (define it under [commands] or use \"layouts[N]\")", v));
    }
    Ok(Arg::None)
}

/// `togglescratch` indexes the scratchpads with `arg.ui`; reject an index
/// that has no scratchpad (togglescratch() also checks, but a config error
/// is more helpful than a key that does nothing).
fn check_scratch_index(func: KeyFn, arg: &Arg, nscratchpads: usize) -> Result<(), ConfigError> {
    if std::ptr::fn_addr_eq(func, Dwm::togglescratch as KeyFn) && arg.ui() as usize >= nscratchpads {
        return err(format!("togglescratch: scratchpad {} does not exist (there are {})", arg.ui(), nscratchpads));
    }
    Ok(())
}

/// Parse the text of a config.toml into a [`Config`], on top of the defaults.
pub fn parse(text: &str) -> Result<Config, ConfigError> {
    let raw: RawConfig = toml::from_str(text).or_else(|e| err(e.to_string()))?;
    let mut config = Config::default();

    /* appearance */
    if let Some(v) = raw.borderpx {
        config.borderpx = v;
    }
    for (name, v, gap) in [
        ("gappih", raw.gappih, &mut config.gappih),
        ("gappiv", raw.gappiv, &mut config.gappiv),
        ("gappoh", raw.gappoh, &mut config.gappoh),
        ("gappov", raw.gappov, &mut config.gappov),
    ] {
        if let Some(v) = v {
            if v > GAP_MAX as u32 {
                return err(format!("{} must be at most {}", name, GAP_MAX));
            }
            *gap = v;
        }
    }
    if let Some(v) = raw.smartgaps {
        config.smartgaps = v;
    }
    if let Some(v) = raw.browsergaps {
        config.browsergaps = v;
    }
    if let Some(v) = raw.snap {
        config.snap = v;
    }
    if let Some(v) = raw.swallowfloating {
        config.swallowfloating = v;
    }
    if let Some(v) = raw.showbar {
        config.showbar = v;
    }
    if let Some(v) = raw.topbar {
        config.topbar = v;
    }
    if let Some(v) = raw.focusonwheel {
        config.focusonwheel = v;
    }
    if let Some(v) = raw.fonts {
        if v.is_empty() {
            return err("fonts must not be empty");
        }
        config.fonts = v;
    }
    if let Some(v) = raw.statusbigfonts {
        config.statusbigfonts = v;
    }
    if let Some(colors) = raw.colors {
        if let Some(norm) = colors.norm {
            config.colors[SCHEME_NORM] = norm.to_vec();
        }
        if let Some(sel) = colors.sel {
            config.colors[SCHEME_SEL] = sel.to_vec();
        }
    }
    if let Some(raw) = raw.statuscolors {
        let sc = &mut config.statuscolors;
        for (v, c) in [
            (raw.col1, &mut sc.col1),
            (raw.col21, &mut sc.col21),
            (raw.col22, &mut sc.col22),
            (raw.col23, &mut sc.col23),
            (raw.col24, &mut sc.col24),
            (raw.col3, &mut sc.col3),
            (raw.col4, &mut sc.col4),
            (raw.col5, &mut sc.col5),
            (raw.col6, &mut sc.col6),
        ] {
            if let Some(v) = v {
                *c = v;
            }
        }
    }

    /* tagging */
    if let Some(tags) = raw.tags {
        /* all tags must fit into an unsigned int bit array (dwm: LENGTH(tags) > 31 fails to compile) */
        if tags.is_empty() || tags.len() > 31 {
            return err(format!("tags: expected between 1 and 31 tags, got {}", tags.len()));
        }
        config.tags = tags;
    }
    if let Some(scratchpads) = raw.scratchpads {
        config.scratchpads = scratchpads
            .into_iter()
            .map(|sp| {
                if sp.cmd.is_empty() {
                    return err(format!("scratchpads: '{}' must have a command", sp.name));
                }
                let cmd = Rc::new(Command { name: sp.name.clone(), argv: sp.cmd });
                Ok(Scratchpad { name: sp.name, cmd })
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
    }
    /* NUMTAGS: the tags and the scratchpad tags share one 32 bit tag mask */
    if config.tags.len() + config.scratchpads.len() > 31 {
        return err(format!(
            "tags and scratchpads together must not exceed 31, got {} tags and {} scratchpads",
            config.tags.len(),
            config.scratchpads.len()
        ));
    }
    let tc = TagCount { ntags: config.tags.len(), nscratchpads: config.scratchpads.len() };
    if let Some(rules) = raw.rules {
        config.rules = rules
            .iter()
            .map(|r| {
                Ok(Rule {
                    class: r.class.clone(),
                    instance: r.instance.clone(),
                    title: r.title.clone(),
                    tags: r.tags.as_ref().map_or(Ok(0), |t| t.to_u32_tags(&tc))?,
                    isfloating: r.isfloating.unwrap_or(false),
                    isterminal: r.isterminal.unwrap_or(false),
                    noswallow: r.noswallow.unwrap_or(false),
                    monitor: r.monitor.unwrap_or(-1),
                })
            })
            .collect::<Result<Vec<_>, ConfigError>>()
            .map_err(|e| ConfigError(format!("rules: {}", e)))?;
    }

    /* layout(s) */
    if let Some(v) = raw.mfact {
        config.mfact = v;
    }
    if let Some(v) = raw.nmaster {
        config.nmaster = v;
    }
    if let Some(v) = raw.resizehints {
        config.resizehints = v;
    }
    if let Some(v) = raw.lockfullscreen {
        config.lockfullscreen = v;
    }
    if let Some(v) = raw.refreshrate {
        if v == 0 {
            return err("refreshrate must be at least 1");
        }
        config.refreshrate = v;
    }
    if let Some(layouts) = raw.layouts {
        if layouts.is_empty() {
            return err("layouts must not be empty");
        }
        config.layouts = layouts
            .iter()
            .map(|l| Ok(Layout { symbol: l.symbol.clone(), arrange: parse_arrange(l.arrange.as_deref())? }))
            .collect::<Result<Vec<_>, ConfigError>>()
            .map_err(|e| ConfigError(format!("layouts: {}", e)))?;
    }

    /* key definitions */
    if let Some(modkey) = &raw.modkey {
        config.modkey = parse_mask(modkey, 0).map_err(|e| ConfigError(format!("modkey: {}", e)))?;
    }
    let modkey = config.modkey;

    /* commands */
    if let Some(commands) = raw.commands {
        config.commands = commands
            .into_iter()
            .map(|(name, argv)| {
                if argv.is_empty() {
                    return err(format!("commands: '{}' must not be empty", name));
                }
                Ok(Rc::new(Command { name, argv }))
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
    }
    if let Some(v) = raw.statusbar {
        /* getstatusbarpid() matches it against process names */
        if !v.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
            return err("statusbar must be a program name (letters, digits, '.', '_', '-') or \"\"");
        }
        config.statusbar = v;
    }
    if let Some(v) = raw.autostart {
        /* an argv like the commands; [] runs nothing */
        config.autostart = v;
    }
    let nlayouts = config.layouts.len();

    if let Some(keys) = raw.keys {
        let mut out = Vec::with_capacity(keys.len() * 2);
        for (n, entry) in keys.iter().enumerate() {
            let ctx = |e: ConfigError| ConfigError(format!("keys[{}]: {}", n, e));
            match entry {
                RawKeyEntry::Key(k) => {
                    let func = parse_func(&k.func).map_err(ctx)?;
                    let arg = parse_arg(k.arg.as_ref(), &config.commands, nlayouts, &tc).map_err(ctx)?;
                    check_scratch_index(func, &arg, tc.nscratchpads).map_err(ctx)?;
                    out.push(Key {
                        mod_: parse_mask(k.mod_.as_deref().unwrap_or("0"), modkey).map_err(ctx)?,
                        keysym: parse_keysym(&k.key).map_err(ctx)?,
                        func,
                        arg,
                    })
                }
                RawKeyEntry::TagKeys(t) => {
                    if t.tag as usize >= config.tags.len() {
                        return err(format!("keys[{}]: tagkeys: tag {} does not exist", n, t.tag));
                    }
                    tagkeys(&mut out, modkey, parse_keysym(&t.tagkeys).map_err(ctx)?, t.tag);
                }
            }
        }
        config.keys = out;
    } else if raw.modkey.is_some() {
        /* the default keys were built with the default modkey; rebuild them
         * with the configured one so that `modkey = "Mod4Mask"` alone works */
        let default_modkey = Config::default().modkey;
        for key in &mut config.keys {
            if key.mod_ & default_modkey != 0 {
                key.mod_ = (key.mod_ & !default_modkey) | modkey;
            }
        }
    }

    /* button definitions */
    if let Some(buttons) = raw.buttons {
        config.buttons = buttons
            .iter()
            .enumerate()
            .map(|(n, b)| {
                let ctx = |e: ConfigError| ConfigError(format!("buttons[{}]: {}", n, e));
                let func = parse_func(&b.func).map_err(ctx)?;
                let arg = parse_arg(b.arg.as_ref(), &config.commands, nlayouts, &tc).map_err(ctx)?;
                check_scratch_index(func, &arg, tc.nscratchpads).map_err(ctx)?;
                Ok(Button {
                    click: parse_click(&b.click).map_err(ctx)?,
                    mask: parse_mask(b.mask.as_deref().unwrap_or("0"), modkey).map_err(ctx)?,
                    button: b.button.to_u32().map_err(ctx)?,
                    func,
                    arg,
                })
            })
            .collect::<Result<Vec<_>, ConfigError>>()?;
    } else if raw.modkey.is_some() {
        let default_modkey = Config::default().modkey;
        for button in &mut config.buttons {
            if button.mask & default_modkey != 0 {
                button.mask = (button.mask & !default_modkey) | modkey;
            }
        }
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_TOML: &str = include_str!("../config/config.toml");

    #[test]
    fn int_expr() {
        assert_eq!(parse_int_expr("1").unwrap(), 1);
        assert_eq!(parse_int_expr("-1").unwrap(), -1);
        assert_eq!(parse_int_expr("+2").unwrap(), 2);
        assert_eq!(parse_int_expr("INC(+1)").unwrap(), inc(1));
        assert_eq!(parse_int_expr(" INC( -1 ) ").unwrap(), inc(-1));
        assert!(parse_int_expr("INC(1000)").is_err());
        assert!(parse_int_expr("INC(x)").is_err());
        assert!(parse_int_expr("x").is_err());
    }

    #[test]
    fn uint_expr() {
        assert_eq!(parse_uint_expr("1 << 8").unwrap(), 256);
        assert_eq!(parse_uint_expr("~0").unwrap(), u32::MAX);
        assert_eq!(parse_uint_expr("0x1ff").unwrap(), 0x1ff);
        assert_eq!(parse_uint_expr("1 | 2 | 1 << 4").unwrap(), 19);
        assert_eq!(parse_uint_expr("Button3").unwrap(), 3);
        assert!(parse_uint_expr("foo").is_err());
    }

    #[test]
    fn masks_and_keys() {
        assert_eq!(parse_mask("MODKEY|ShiftMask", xlib::Mod4Mask).unwrap(), xlib::Mod4Mask | xlib::ShiftMask);
        assert_eq!(parse_mask("0", xlib::Mod1Mask).unwrap(), 0);
        assert!(parse_mask("Bogus", 0).is_err());
        assert_eq!(parse_keysym("XK_Return").unwrap(), XK_Return as KeySym);
        assert_eq!(parse_keysym("comma").unwrap(), XK_comma as KeySym);
        assert!(parse_keysym("no_such_key_xyz").is_err());
    }

    #[test]
    fn empty_file_is_default() {
        let c = parse("").unwrap();
        let d = Config::default();
        assert_eq!(c.keys.len(), d.keys.len());
        assert_eq!(c.buttons.len(), d.buttons.len());
        assert_eq!(c.tags, d.tags);
    }

    /// The shipped config/config.toml is my config.h, on top of config.def.h.
    #[test]
    fn shipped_config_is_config_h() {
        let c = parse(DEFAULT_TOML).expect("config/config.toml parses");
        assert_eq!((c.borderpx, c.snap), (2, 32));
        assert!(c.showbar && c.topbar);
        assert_eq!(c.fonts, ["JetBrainsMono Nerd Font:size=11:style=bold"]);
        assert_eq!(c.colors[SCHEME_NORM], ["#ebdbb2", "#282828", "#282828"]);
        assert_eq!(c.colors[SCHEME_SEL], ["#ebdbb2", "#282828", "#ebdbb2"]);
        assert!(!c.resizehints);
        assert_eq!(c.rules.len(), 8);
        assert_eq!(c.modkey, xlib::Mod4Mask);
        let tab = parse_keysym("Tab").unwrap();
        assert!(c.keys.iter().any(|k| k.keysym == tab && k.mod_ == xlib::Mod1Mask));
        /* no quit binding: I exit through the powermenu */
        assert!(!c.keys.iter().any(|k| std::ptr::fn_addr_eq(k.func, Dwm::quit as KeyFn)));
        let mute = parse_keysym("XF86AudioMute").unwrap();
        let Some(Arg::V(cmd)) = c.keys.iter().find(|k| k.keysym == mute && k.mod_ == 0).map(|k| &k.arg) else {
            panic!("XF86AudioMute is not bound to a command");
        };
        assert!(cmd.argv[2].ends_with("kill -44 $(pidof dwmblocksr)"));
        /* every mapping from config.h: STACKKEYS, TAGKEYS (4 keys each) and the rest */
        assert_eq!(c.keys.len(), 8 + 9 * 4 + 100);
        assert_eq!(c.buttons.len(), 19);
    }

    #[test]
    fn rejects_bad_values() {
        assert!(parse("refreshrate = 0").is_err());
        assert!(parse("layouts = []").is_err());
        assert!(parse("tags = []").is_err());
        assert!(parse("gappih = 1001").is_err());
        assert!(parse("layouts = [ { symbol = \"x\", arrange = \"fibonacci\" } ]").is_err());
        assert!(parse("nonsense = 1").is_err());
        assert!(parse("statusbar = \"dwmblocksr; rm -rf ~\"").is_err());
        assert!(parse("statusbar = \"/usr/bin/dwmblocksr\"").is_err());
        assert!(parse("statusbar = \"\"").is_ok());
        assert!(parse("keys = [ { mod = \"MODKEY\", key = \"p\", func = \"spawn\", arg = { v = \"nope\" } } ]").is_err());
        assert!(parse("keys = [ { tagkeys = \"1\", tag = 40 } ]").is_err());
        /* scratchpads */
        assert!(parse("scratchpads = [ { name = \"x\", cmd = [] } ]").is_err());
        assert!(parse("scratchpads = [ { name = \"x\", cmd = [\"x\"] } ]\nrules = [ { instance = \"x\", tags = \"SPTAG(1)\" } ]").is_err());
        assert!(parse("keys = [ { mod = \"MODKEY\", key = \"y\", func = \"togglescratch\", arg = { ui = 2 } } ]").is_err());
        let many = (0..30).map(|i| format!("\"{}\"", i)).collect::<Vec<_>>().join(", ");
        assert!(parse(&format!("tags = [{}]", many)).is_err()); /* 30 tags + 2 scratchpads > 31 */
    }

    #[test]
    fn scratchpad_tags() {
        let c = parse("scratchpads = [ { name = \"a\", cmd = [\"a\"] }, { name = \"b\", cmd = [\"b\"] } ]\nrules = [ { instance = \"b\", tags = \"SPTAG(1)\", isfloating = true } ]").unwrap();
        assert_eq!(c.scratchpads.len(), 2);
        assert_eq!(c.rules[0].tags, 1 << 10);
        assert!(parse_uint_expr("SPTAG(0)").is_err()); /* no scratchpad context */
    }

    #[test]
    fn modkey_alone_rebinds_defaults() {
        let c = parse("modkey = \"Mod4Mask\"").unwrap();
        assert!(c.keys.iter().all(|k| k.mod_ & xlib::Mod1Mask == 0));
        assert!(c.keys.iter().any(|k| k.mod_ & xlib::Mod4Mask != 0));
        assert!(c.buttons.iter().filter(|b| b.mask != 0).all(|b| b.mask == xlib::Mod4Mask));
    }
}
