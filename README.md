<p align="center"><img src="dwmr.png" alt="dwmr"></p>

# dwmr - dynamic window manager

dwmr is a port of [dwm](https://dwm.suckless.org) (6.8) to Rust. It is a
function-for-function replica of dwm.c/drw.c with the same behaviour, the
same monocle and floating layouts, the same tags, bar, key and mouse
bindings, plus the gap-aware tiled layouts of dwm's vanitygaps patch and one
addition: the configuration is read from a file at
startup instead of being compiled in.

## Requirements

- Rust (cargo) and a C toolchain for linking
- Xlib, Xft, Xinerama, XRes and fontconfig headers (Debian: `libx11-dev libxft-dev
  libxinerama-dev libxres-dev libfontconfig-dev`, plus `pkg-config`)

## Installation

    make                    # cargo build --release
    sudo make install       # /usr/local/bin/dwmr and the man page
    make install-config     # copy the default config to ~/.config/dwmr/config.toml

`make install PREFIX=$HOME/.local` installs without root. `cargo install
--path .` works too but puts the binary in `~/.cargo/bin`.

## Running

Add this to `~/.xinitrc` (as the last, foreground command) and run `startx`:

    exec dwmr

`dwmr -v` prints the version.

## Configuration

dwmr reads `$XDG_CONFIG_HOME/dwmr/config.toml`, i.e. `~/.config/dwmr/config.toml`.
The file mirrors dwm's config.def.h section by section: appearance, colors,
tags, rules, layouts, the modifier key, commands, keys and buttons. Every key
is optional and falls back to the built-in default, which is dwm's default
configuration. A file that fails to parse is reported on stderr and ignored,
so the window manager always starts. See `config/config.toml` for the
commented default.

A few notes on the format:

- Masks can be written like in C: `"1 << 8"`, `"~0"`, `"0x1ff"`, `"1 | 2"`.
- Keys are keysym names without the `XK_` prefix (`"Return"`, `"space"`,
  `"comma"`, `"p"`). Modifiers are `MODKEY`, `ShiftMask`, `ControlMask`,
  `Mod1Mask`..`Mod5Mask`, joined with `|`.
- `{ tagkeys = "1", tag = 0 }` in `keys` expands like dwm's `TAGKEYS` macro.
- Functions: spawn, togglebar, focusstack, pushstack, incnmaster, setmfact,
  zoom, view, killclient, setlayout, togglefloating, togglefullscr,
  togglesticky, togglescratch, tag, focusmon, tagmon, toggleview, toggletag,
  shifttag, shiftview, shiftviewclients, defaultgaps, incrgaps, togglegaps,
  togglebgaps, sigstatusbar, quit, movemouse, resizemouse.
  Layouts: spiral, tile, bstack, dwindle, deck, monocle, centeredmaster,
  centeredfloatingmaster, none.
- `scratchpads` is a list of `{ name, cmd }` (dwm's scratchpads patch). Each
  scratchpad owns a tag bit above the normal tags, written `"SPTAG(0)"`,
  `"SPTAG(1)"`, ... in a rule's `tags`; the tags and the scratchpads together
  are limited to 31. `togglescratch` with `{ ui = n }` shows or hides the
  window on scratchpad `n`, spawning its `cmd` when there is none yet. A
  floating scratchpad window is centred each time it is shown. The bar shows
  only the normal tags.
- `focusstack` and `pushstack` take a stack position (dwm's stacker patch):
  `{ i = "INC(+1)" }` is relative to the focused window, `{ i = 0 }` is the
  top of the stack, `{ i = -1 }` the bottom, another integer an absolute
  position. `pushstack` moves the focused window to that position.
- `shifttag`, `shiftview` and `shiftviewclients` (dwm's shift-tools patch)
  take `{ i = n }`: the viewed normal tags are circularly shifted by `n`
  positions (left for positive, right for negative, wrapping at the number
  of tags). `shifttag` moves the focused window there, `shiftview` views it
  and `shiftviewclients` keeps shifting until it reaches a tag that has a
  window, ignoring windows on scratchpad tags. Bound by default to
  Mod1-Shift-y/o, the mouse wheel on the tag bar and Mod1-(Shift-)Tab; the
  latter replaces dwm's Mod1-Tab "view previous tags".
- The status text (the root window name, up to 1023 bytes) is shown on every
  monitor and may contain dwm's status2d colour codes, `^...^`, which are
  not drawn: the text starts in `statuscolors.col1`; `^3^`..`^6^` switch to
  `col3`..`col6`, `^c#rrggbb^` to any colour and `^2^` to the weather colour
  chosen from the temperature after it (+20 and above `col21`, below +20
  `col22`, negative `col23`, no sign `col24`). `^B^` switches to the bigger
  `statusbigfonts` (`[]` for none) until `^N^`, e.g. for a block's icon.
  Other codes (`^r`, `^b`, `^d`, `^f`) are ignored; an unterminated `^` ends
  the text.
- Clickable status blocks (dwm's statuscmd patch): a status bar like
  dwmblocksr or dwmblocks puts its block's signal as a byte (1..31) before
  each block. These bytes are not drawn. A click on the status text runs the
  button's function with that block's signal remembered, and `sigstatusbar`
  sends it to the program named `statusbar` (default `"dwmblocksr"`, found
  by process name) as SIGRTMIN+signal with the `arg.i` as value; the program
  then runs the block's command with `BLOCK_BUTTON` set to it. By default
  Button1..3 on the status text send 1..3.
- `arg = { v = "termcmd" }` refers to an entry of `commands`;
  `arg = { v = "layouts[2]" }` to an entry of `layouts`. In `dmenucmd` the
  argument after `-m` is replaced with the selected monitor (dwm's dmenumon).
- The tiled layouts (spiral, tile, bstack, dwindle, deck, centeredmaster,
  centeredfloatingmaster) leave gaps between windows (`gappih`, `gappiv`) and
  at the screen edge (`gappoh`, `gappov`), 20 pixels each by default, at most
  1000 (dwm's vanitygaps patch, without cfacts: windows in an area share it
  evenly). `smartgaps = true` drops the outer gaps for a single window. A
  single Firefox window (class starting with "firefox", any case) gets no
  outer gaps unless `browsergaps = true`. `incrgaps` with `{ i = n }` changes
  all gaps of the selected monitor by `n`, `defaultgaps` resets them,
  `togglegaps` turns gaps off and on and `togglebgaps` toggles `browsergaps`.
  Bound to Mod1-plus/minus (3 px), Mod1-Shift-plus/minus (1 px), Mod1-x,
  Mod1-z and Mod1-Control-z, and Mod1-Button2/4/5 on a window (defaultgaps,
  +1, -1), which replaces dwm's Mod1-Button2 togglefloating. The default
  layout is spiral; Mod1-t/f/m still select tile, floating and monocle.
- Setting only `modkey` rebinds the default keys and buttons to it.
- The bar shows no window title and has no title click area (dwm's notitle
  patch); `ClkWinTitle` is not a valid click.
- Only selected or occupied tags are drawn, without the small occupancy
  squares (dwm's hide_vacant_tags patch). A window on every tag does not
  count as occupying them. Clicks on the tag bar follow the same layout.
- The only tiled window on a monitor, and every window in the monocle layout,
  is drawn without a border (dwm's noborder patch).
- Focus follows mouse clicks only, not pointer movement (dwm's focusonclick
  patch). `focusonwheel = false` (the default) lets the scroll wheel work on
  an unfocused window without focusing it.
- A sticky window is visible on every tag of its monitor (dwm's sticky
  patch). `_NET_WM_STATE_STICKY` is supported, so a client can set the state
  itself, and `_NET_WM_STATE` lists both the fullscreen and the sticky state.
- A window toggled back to floating returns to the position and size it last
  floated with (dwm's savefloats patch). A window that never floated, or whose
  saved position is on another monitor, is centred on its monitor instead
  (dwm's togglefloatingcenter patch).
- A window started from a terminal takes the terminal's place, and the
  terminal comes back when the window closes (dwm's swallow patch). A rule
  marks a window as a terminal with `isterminal = true`; `noswallow = true`
  keeps a window from swallowing its terminal. Floating windows only swallow
  with `swallowfloating = true`. The terminal is found through the window's
  PID (from the X-Resource extension) and its parents in `/proc`.

## Layout of the source

- `src/dwm.rs` - dwm.c: the `Dwm` struct holds what dwm keeps in globals.
  Clients live in a slab and monitors in a `Vec`; the `next`/`snext` lists
  are indices, so a stale reference can never be a memory error.
- `src/drw.rs` - drw.c, the bar drawing and font fallback code.
- `src/config.rs` - config.def.h as `Config::default()` plus the TOML loader.
- `src/util.rs` - util.c. `src/fontconfig.rs` - the fontconfig FFI drw needs.
  `src/xres.rs` - the X-Resource (libXRes) FFI the swallow patch needs.
- `examples/transient.rs` - transient.c, a test client
  (`cargo run --example transient`).

Xinerama support is the `xinerama` cargo feature (on by default), like
`XINERAMAFLAGS` in dwm's config.mk: `cargo build --release --no-default-features`
builds without it.

`cargo test` checks the config parser, including that `config/config.toml`
produces exactly the built-in defaults.

## License

MIT/X Consortium License, see LICENSE. dwm is (c) its authors; see LICENSE
for the full list.
