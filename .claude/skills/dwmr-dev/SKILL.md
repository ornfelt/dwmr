---
name: dwmr-dev
description: How to add features, port dwm patches and fix bugs in dwmr, the Rust port of dwm. Use for any change to this repo.
---

# dwmr development

## What this project is

dwmr is a port of dwm 6.8 (suckless' dynamic window manager, C) to Rust. The
executable is called `dwmr`. It is a function-for-function replica of
dwm.c/drw.c/util.c: same behaviour, same logic, same structure, same function
and field names, and the same comment style, as far as Rust allows. The one
addition over dwm is that the configuration is read at startup from
`$XDG_CONFIG_HOME/dwmr/config.toml` (normally `~/.config/dwmr/config.toml`)
on top of compiled-in defaults that equal dwm's config.def.h.

The priorities, in order:

1. **Safety**: dwmr must never crash. No panics on any input from X or the
   config file: no `unwrap()`/`expect()` on runtime data, no unchecked
   indexing of data that can be stale, no integer overflow in debug builds
   (use `wrapping_*`/`saturating_*` where C relied on wraparound), no
   division by a value that can be zero. Bad config values are rejected at
   load time with a message, and the defaults are used.
2. **Speed and memory efficiency**: no allocations in the event path that dwm
   does not have (clone an `Rc<Config>`, not the config; reuse buffers),
   no extra copies of window lists, no hash maps where a short list walk is
   what dwm does.
3. **Fidelity**: the port should be a full replica of dwm with the same
   logic, structure and comment style as much as possible. However, if
   adaptions are needed to make it work for Rust's ecosystem and rules,
   adapt to it. Use the common code and naming conventions Rust uses
   (snake_case variables and functions, UPPER_CASE constants, `bool` instead
   of `int` flags, `Option` instead of NULL, `&str`/`String` instead of
   `char[]`). Function names stay dwm's (`applyrules`, `focusstack`,
   `updategeom`), unchanged, so a dwm patch maps onto the Rust code 1:1.

## Where things are

| dwm file       | dwmr file             | notes |
|----------------|-----------------------|-------|
| dwm.c          | `src/dwm.rs`          | `struct Dwm` holds every global of dwm.c; every C function is a method in the same order |
| drw.c/drw.h    | `src/drw.rs`          | `Drw`, `Fnt` (in a `Vec`, index 0 is the primary font), `Clr` = `XftColor`, `Cur` |
| util.c/util.h  | `src/util.rs`         | `die()`, `truncate_utf8()` (the `char[N]` buffer limits), `between()` |
| config.def.h   | `src/config.rs`       | `Config::default()` is config.def.h; the rest is the TOML loader |
| config.h       | `config/config.toml`  | the shipped default config; must equal `Config::default()` (a unit test checks this) |
| transient.c    | `examples/transient.rs` | |
| dwm.1, Makefile | `dwmr.1`, `Makefile`  | `make`, `sudo make install`, `make install-config` |
| fontconfig     | `src/fontconfig.rs`   | the FFI declarations drw needs that the `x11` crate lacks |

Reference C source, if needed for comparison: `~/Downloads/dwm` (dwm 6.8).

## How C constructs are represented

- **Globals** (`selmon`, `mons`, `stext`, `bh`, `scheme`, atoms, ...) are
  fields of `Dwm`. The C error handler's `xerrorxlib` is the one true
  static (`XERRORXLIB: OnceLock`), because Xlib calls it with no user data.
- **Clients** live in a slab, `Dwm::clients: Vec<Client>` with a free list;
  `ClientId = usize`. `Client.next`/`snext`, `Monitor.clients`/`stack`/`sel`
  are `Option<ClientId>`. Walk a list with
  `let mut c = self.mons[m].clients; while let Some(i) = c { ...; c = self.clients[i].next; }`.
  `unmanage` resets the freed slot to `Client::default()` so a stale id is harmless.
- **Monitors** are `Dwm::mons: Vec<Monitor>` in list order; `MonId = usize`.
  `mons->next` is `mons.len() > 1`, `for (m = mons; m; m = m->next)` is
  `for m in 0..self.mons.len()`. `updategeom` only ever removes the last one.
- **Layouts** are `Dwm::layouts: Vec<Layout>` (a copy of `config.layouts`,
  because `cleanup()` appends the empty layout); `Monitor.lt` holds indices.
- **Function pointers** in keys/buttons are `fn(&mut Dwm, &Arg)`
  (`KeyFn`), layouts use `fn(&mut Dwm, MonId)` (`ArrangeFn`). In event
  handlers take `let config = Rc::clone(&self.config);` first, then iterate
  `config.keys` while calling methods on `self`.
- **`Arg`** is an enum: `None` (`{0}`), `I`, `Ui`, `F`, `V(Rc<Command>)`
  (`.v = cmd`), `Layout(usize)` (`.v = &layouts[n]`). `arg.i()`, `arg.ui()`,
  `arg.f()` read it like the union; `is_zero()` is the `arg.i == 0` test.
- **`NULL` arguments** become `Option`: `focus(None)`, `arrange(None)`,
  `unfocus(None, ..)`, `showhide(None)`.
- **Macros**: `WIDTH`/`HEIGHT`/`INTERSECT` are free fns, `ISVISIBLE`,
  `CLEANMASK`, `TAGMASK` are `Dwm` methods, `TEXTW` is
  `Self::textw(&mut self.drw, self.lrpad, text)` (takes fields explicitly so
  other fields of `self` may be borrowed in the same expression).
- **Unsafe**: every Xlib/Xft/fontconfig/libc call sits in an `unsafe` block
  with a `// SAFETY:` comment saying why it is sound. Keep them small; do the
  Rust logic outside. Constants the `x11` crate lacks (X request codes,
  `XC_*` cursors, `NormalState`, ...) are defined at the top of dwm.rs from
  the X headers, with the header named.
- **Event dispatch**: `handler(ty)` in dwm.rs is the `handler[LASTEvent]`
  table as a `match`. Events are read with `let ev: XButtonEvent = e.into();`.
- **Fixed buffers**: `char name[256]` / `stext[256]` / `ltsymbol[16]` are
  `String`s truncated with `truncate_utf8(s, N - 1)` at the same places dwm
  uses `strncpy`/`snprintf`.
- **Borrow checker patterns**: copy needed fields to locals first
  (`let m = self.clients[c].mon;`), then call `&mut self` methods; use
  `let cl = &mut self.clients[c];` for a block of field updates; index by id
  rather than holding references across calls.

## Adding a feature or porting a dwm patch

1. Read the patch (a `.diff` against dwm.c/config.def.h/drw.c). Map each hunk
   to the Rust function of the same name in `src/dwm.rs` (or drw.rs) and make
   the equivalent change there, keeping the patch's comments and naming. New
   C functions become new `Dwm` methods in dwm's alphabetical position; new
   globals become `Dwm` fields; new `static` locals become fields too.
2. If the patch touches config.def.h, do all four of:
   - `Config` struct + `Config::default()` in `src/config.rs` (the value from
     config.def.h),
   - the raw TOML shape (`RawConfig` etc.) and the conversion in `parse()`,
     with validation that rejects values that could crash (zero divisors,
     out-of-range indices, empty lists),
   - `config/config.toml` (same value, commented like config.def.h),
   - `~/.config/dwmr/config.toml` is the user's copy; do not edit it unless
     asked, but tell the user what to add.
   New key/button functions must be added to `parse_func()`; new layout
   functions to `parse_arrange()`; new `Arg` kinds to `parse_arg()`.
3. If the patch adds a key binding, add it to both the `keys` vector in
   `Config::default()` and `config/config.toml`; the test
   `shipped_config_matches_defaults` will fail until both agree.
4. Update `dwmr.1` and `README.md` if user-visible behaviour or config keys
   change. The version stays in `Cargo.toml` (`dwmr -v` prints it).

## Fixing bugs

Compare against `~/Downloads/dwm/dwm.c` first: if dwm has the same behaviour
it is probably intended, and a deliberate deviation must be commented at the
spot (see the `nn == 0` case in `updategeom_xinerama` for the style). Prefer
the minimal change in the function where dwm would make it.

## Verifying a change (always, before reporting done)

```sh
cargo build && cargo clippy --all-targets   # must be warning-free
cargo test                                   # config/drw/util unit tests
cargo build --release --no-default-features  # the non-Xinerama variant must still build
cargo build --release                        # what `make install` installs
```

Functional test: there is no Xvfb/Xephyr on this machine and no sudo. Get a
virtual X server without root by extracting the package locally:

```sh
apt-get download xvfb && dpkg -x xvfb_*.deb xvfb   # in a scratch dir
./xvfb/usr/bin/Xvfb :99 -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
DISPLAY=:99 ./target/release/dwmr >dwmr.log 2>&1 &
DISPLAY=:99 xterm & DISPLAY=:99 xdotool key alt+Return   # drive it; check with xwininfo/xprop
```

Run such tests from a script whose background processes redirect stdout and
stderr to files, otherwise the calling shell hangs. Kill by PID (or
`pkill -x Xvfb`, `pkill -x dwmr`), never `pkill -f` a pattern that also
appears in the invoking command line. A test passes only if `dwmr.log` is
empty, the process is still alive, and `xdotool key alt+shift+q` makes it
exit.

Also run `dwmr` on the live display once: it must print
"dwmr: another window manager is already running" and exit 1.

## Install and run

`make` builds, `sudo make install` installs `/usr/local/bin/dwmr` and the man
page (the Makefile runs cargo as `$SUDO_USER`, since rustup is per user),
`make install-config` copies the default config to `~/.config/dwmr/` if none
exists. `~/.xinitrc` starts it with `WM=dwmr startx`.
