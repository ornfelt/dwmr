<p align="center"><img src="dwmr.png" alt="dwmr"></p>

# dwmr - dynamic window manager

dwmr is a port of [dwm](https://dwm.suckless.org) (6.8) to Rust. It is a
function-for-function replica of dwm.c/drw.c with the same behaviour, the
same tiled, monocle and floating layouts, the same tags, bar, key and mouse
bindings, plus one addition: the configuration is read from a file at
startup instead of being compiled in.

## Requirements

- Rust (cargo) and a C toolchain for linking
- Xlib, Xft, Xinerama and fontconfig headers (Debian: `libx11-dev libxft-dev
  libxinerama-dev libfontconfig-dev`, plus `pkg-config`)

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
  togglesticky, tag, focusmon, tagmon, toggleview, toggletag, quit, movemouse,
  resizemouse.
  Layouts: tile, monocle, none.
- `focusstack` and `pushstack` take a stack position (dwm's stacker patch):
  `{ i = "INC(+1)" }` is relative to the focused window, `{ i = 0 }` is the
  top of the stack, `{ i = -1 }` the bottom, another integer an absolute
  position. `pushstack` moves the focused window to that position.
- `arg = { v = "termcmd" }` refers to an entry of `commands`;
  `arg = { v = "layouts[2]" }` to an entry of `layouts`. In `dmenucmd` the
  argument after `-m` is replaced with the selected monitor (dwm's dmenumon).
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

## Layout of the source

- `src/dwm.rs` - dwm.c: the `Dwm` struct holds what dwm keeps in globals.
  Clients live in a slab and monitors in a `Vec`; the `next`/`snext` lists
  are indices, so a stale reference can never be a memory error.
- `src/drw.rs` - drw.c, the bar drawing and font fallback code.
- `src/config.rs` - config.def.h as `Config::default()` plus the TOML loader.
- `src/util.rs` - util.c. `src/fontconfig.rs` - the fontconfig FFI drw needs.
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
