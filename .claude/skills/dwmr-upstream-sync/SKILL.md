---
name: dwmr-upstream-sync
description: Port new commits from the user's dwm fork (~/.config/dwm, the C dwm that dwmr was ported from) into dwmr, one at a time and in order - but only those that are applicable and worth having in dwmr, judged by how dwmr differs from dwm. Every dwm commit after the baseline has a row in UPSTREAM_SYNC.md - pending until looked at - with its origin (the user's own or a suckless commit), kind, status, handling and the dwmr commit, and upstream_info.txt names the dwm commit dwmr is synced through. Ports each commit by intent following the dwmr-dev skill, verifies with cargo, commits each with the dwm subject and a dwm-commit trailer, followed by a ledger commit. Asks before anything that would change a deliberate dwmr deviation or when it is unclear whether a commit belongs in dwmr.
disable-model-invocation: true
argument-hint: "[empty means every remaining commit within the run budget, or: setup, status, rebuild, <dwm-commit>, skip <dwm-commit> <reason>, replay <dwm-commit>]"
---

# Sync dwm commits into dwmr

dwmr (`~/Code2/Rust/dwmr`, branch `main`, remote `origin` = `https://github.com/ornfelt/dwmr`)
is a Rust port of the user's patched dwm. The **source** of this sync is that dwm:

| | |
| --- | --- |
| Source repo | `~/.config/dwm` (branch `master`, `https://github.com/ornfelt/dwm`) |
| What it is | suckless dwm 6.8 plus the user's patches and config - the C that dwmr was ported from |
| How it gets suckless fixes | its own `dwm-upstream-sync` skill lands suckless dwm commits there, with their upstream subject |
| Read with | `git -C ~/.config/dwm ...` - no remote is added, nothing is fetched into dwmr |

**Read `.claude/skills/dwmr-dev/SKILL.md` before porting anything.** It is the authority on how a
dwm change becomes Rust: the file mapping, how C constructs are represented, the safety rules,
the four places a config option lives, and the verification steps. This skill only adds *which*
dwm commits to port, in which order, and how they are tracked.

dwmr is not a mechanical copy of dwm. It is a function-for-function replica with deliberate
differences (Rust safety, a TOML config on top of compiled-in defaults, bug fixes dwm does not
have, its own program names). So a dwm commit is ported only when it is **applicable** to dwmr
and **beneficial** there - see "Deciding whether a commit belongs in dwmr". Every dwm commit
still gets a row, so `UPSTREAM_SYNC.md` always shows what was ported and why the rest was not.

The dwm commits cannot be cherry-picked (different language and files): each one is read, its
intent understood, and the equivalent change made by hand in the dwmr function of the same name.

**Never write to `~/.config/dwm`.** If a dwm commit looks wrong, port it right and flag it (see
"Flagged for dwm") - the user fixes dwm with its own tooling.

## Arguments

- **none** - port the remaining dwm commits, oldest first, within the per-run budget below.
- `setup` - or the ledger does not exist yet: create `UPSTREAM_SYNC.md` with the baseline and a
  row for every dwm commit after it, filled per "Picking up the current state", update
  `upstream_info.txt`, commit both (see "Committing the ledger"), then continue as **none**.
- `status` - only report the baseline, the high-water mark, the remaining list, the open questions
  and the open "Flagged for dwm" entries. When the ledger disagrees with the tree, report what
  "Picking up the current state" would change. Write nothing.
- `rebuild` - redo the ledger per "Picking up the current state", keeping the notes, Special
  handling entries, flags and open questions of rows the rebuild confirms. A ledger-only run.
- `<dwm-commit>` - port just that dwm commit, even out of order.
- `skip <dwm-commit> <reason>` - record the commit as `skipped` (reason `declined`) without
  porting it, Handling `requested`, with the reason under "Special handling" in the user's words.
  Use this when the user answered "no" to an earlier question.
- `replay <dwm-commit>` - redo a commit that already has a row, because the earlier port was
  wrong or incomplete, or because the user changed their mind about a skip. See "Replay".

## 1. Make sure both trees are ready

```bash
cd ~/Code2/Rust/dwmr
git status --short             # must be clean before starting - otherwise stop and ask
git -C ~/.config/dwm log -1 --format='%h %cs %s' master    # the source's current HEAD
```

Only **committed** dwm history on `~/.config/dwm`'s local `master` counts (it may be ahead of
`origin`; that is fine). Uncommitted changes in `~/.config/dwm` are ignored - mention them in the
report if `git -C ~/.config/dwm status --short` shows any in `dwm.c`, `drw.c`, `config.h` or
`config.def.h`, since the user may have meant to commit them first.

## 2. The baseline

The baseline is **`723a850`** (`723a850aea08d0460f3cb94c6ba5a67c7216ddc4`) - "Status bar font
Propo, start dwmblocks with sb-start" (2026-10-31), ported as dwmr `ffeda55`. Everything at or
before it was mirrored into dwmr by hand before this skill existed (the port itself on
2026-09-24, then one dwmr commit per dwm change) and never comes up again. The baseline is fixed
and recorded in the ledger header; `upstream_info.txt` started there but moves on with the
high-water mark, so it is not the baseline.

```bash
BASE=723a850aea08d0460f3cb94c6ba5a67c7216ddc4
git -C ~/.config/dwm log --reverse --date-order --format='%h %cs %p | %s' "$BASE..master"
```

If `$BASE` is ever not an ancestor of dwm's `master` (history rewritten there), stop and ask.

## 3. The ledger

`UPSTREAM_SYNC.md` in the repo root (tracked) is the progress file. Every run reads it first and
updates it last. It is a hint that makes orientation cheap, not the source of truth - the code is.

Shape:

```markdown
# dwmr upstream sync ledger

Maintained by the `dwmr-upstream-sync` skill. A hint, not the source of truth - the code is.

Source: the user's dwm fork, `~/.config/dwm` `master` (<https://github.com/ornfelt/dwm>).
Baseline: `723a850` - Status bar font Propo, start dwmblocks with sb-start (2026-10-31), dwmr `ffeda55`.
Everything up to and including the baseline was ported before this ledger existed.
High-water mark: `<sha>` `<subject>` (every row up to and including this one is decided)
dwm HEAD when last checked: `<sha>` `<subject>` (<date of the run>)
Remaining: `<n>` (`pending` and `asked` rows)

Statuses: `pending` (not looked at yet), `ported` (the change carried over 1:1 to the matching
dwmr code), `adapted` (ported, but reshaped for dwmr), `partial` (part ported, part skipped - the
notes say which), `skipped` (not ported - the notes start with the reason), `asked` (waiting on
an answer from the user, nothing written).

Origin: `own` (the user's own dwm commit), `suckless` (a suckless dwm commit landed in the fork by
`dwm-upstream-sync`), `merge` (a merge commit).

Kind: `code`, `config`, `docs`, `build`, `housekeeping` (see the skill's file map).

Handling: `as-is` (the dwm change translated 1:1 to Rust per the dwmr-dev skill, a skip for a
standard reason included), `needed` (changed, or skipped, because dwmr differs from dwm),
`requested` (changed, or skipped, because the user asked). Anything but `as-is` has an entry
under Special handling.

Skip reasons: `housekeeping`, `already in dwmr`, `not applicable (rust)`, `not applicable (build)`,
`no counterpart`, `deviation`, `declined`, `merge, changes nothing`.

| # | Date | dwm commit | Origin | Kind | Subject | Status | Handling | dwmr commit | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |

## Special handling

<one bullet per row whose Handling is not `as-is`: the row and commit, `needed` or `requested`,
what was done differently from the dwm diff, and why - the dwmr difference, or the user's request
in their own words>

## Flagged for dwm

<one bullet per evident bug found in a ported commit's own change - see "Flagged for dwm">

## Open questions

<one bullet per `asked` row: the question put to the user and what it is waiting on>
```

Rules for the ledger:

- **One row per dwm commit after the baseline, every one of them** - ported, skipped, `asked` or
  `pending`. Each run first adds `pending` rows for the commits that have none yet (the
  `git log` of section 2, minus the commits that already have a row). Housekeeping commits
  (`update diff files`, `update upstream sync ledger`, skill files, ...) get rows too: they are
  what makes the ledger complete, and they are decided in bulk without a dwmr commit.
- **Chronological order, oldest first**: rows are appended in that `--date-order` order and
  existing rows never move. `#` keeps counting up. `Date` is the commit date.
- **Origin**: `merge` when the commit has more than one parent; `suckless` when its subject is a
  suckless commit's - check with
  `git -C ~/.config/dwm log --format=%h -F --grep='<subject>' upstream/master` (an exact subject
  match), or the commit has a row in `~/.config/dwm/UPSTREAM_SYNC.md`; else `own`.
- **Kind** from the files the commit touches (see the file map); a commit touching code and config
  is `code`.
- **Handling** as above; `requested` wins over `needed`. An `adapted` or `partial` row always has
  `needed` or `requested`. Something the user asks for in the run is done and documented, never
  silently folded into the commit.
- A `skipped` row's notes **start with the skip reason** from the list, then the specifics
  (`not applicable (rust): drw_text's index is bounds-checked by truncate_utf8`). The dwmc sync
  skill reads these reasons to decide whether dwmc follows dwmr's decision, so pick the reason
  precisely.
- `dwmr commit` is the short sha of the dwmr commit carrying the `dwm-commit:` trailer, or `-`.
- The **high-water mark** is the last row before the first row that is still `pending` or `asked`.
  A commit ported out of order (`<dwm-commit>`) gets its row decided but does not move the mark.
- `Remaining` is the `pending` and `asked` rows, recomputed each run, not carried over.
- The `Notes` column is where the real information lives: which dwmr functions changed, what was
  reshaped or dropped and why, what went into the shipped and the user's config, and `flagged`
  when the row has an entry under "Flagged for dwm".
- If the ledger disagrees with dwmr's `main`, **the tree wins** and the rows concerned are
  rebuilt per "Picking up the current state" in the same run.

### upstream_info.txt

`upstream_info.txt` in the repo root (tracked) names the dwm commit dwmr is synced through - the
high-water mark's full hash (the baseline while no row after it is decided):

```text
dwm
  commit <full hash of the high-water mark>
```

It changes whenever the high-water mark moves, and is committed with the ledger.

### Picking up the current state

The ledger can be rebuilt from dwmr's `main` and dwm's `master` - on setup, in `rebuild` mode,
and whenever the ledger is missing rows, has a row for a commit not in `$BASE..master`, or
disagrees with the code. For each dwm commit after the baseline, in ledger order:

1. **An earlier sync**: `git log --format='%h %s' --grep="^dwm-commit: <full sha>" main` finds
   the dwmr commit that carries it. Fall back to the subject (with the program-name swaps of
   section 5) for a commit made by hand without the trailer. Found: `ported`, or `adapted` /
   `partial` with `needed` when the dwmr commit clearly reshaped or left out part of it, naming
   that commit.
2. **Housekeeping**: only files of the housekeeping row of the file map: `skipped`
   (`housekeeping`).
3. **Already in dwmr**: no such commit, but the change is in dwmr's code by intent (check the
   matching function, not the text): `skipped` (`already in dwmr (<how found>)`). Part of it:
   `pending` with a note saying which part is there.
4. **A no-change merge**: `skipped` (`merge, changes nothing`).
5. Otherwise `pending`.

Recompute the high-water mark, `Remaining` and `upstream_info.txt` from the rows. Say in the report
how many rows this added or changed and which commits were found already synced.

## 4. Deciding whether a commit belongs in dwmr

Read the commit (`git -C ~/.config/dwm show <sha>`; a merge with `--remerge-diff`, cross-checked
with `--cc` - empty is `skipped (merge, changes nothing)`) and the dwmr code it would touch. Then
decide, in this order:

1. **Housekeeping** - touches only housekeeping files: `skipped (housekeeping)`, no dwmr commit.
   For a mixed commit, the housekeeping part is ignored and the rest decided on its own.
2. **Already in dwmr** - dwmr already does what the commit makes dwm do (dwmr was ported with many
   fixes dwm got later, and the Rust rules prevent whole classes of bugs by construction): `skipped
   (already in dwmr)`, saying where.
3. **Not applicable (rust)** - the commit fixes something that cannot happen in dwmr: a `free`,
   leak or double free (Rust drops), a buffer overflow or out-of-bounds read (bounds-checked or
   `truncate_utf8`), signed overflow (`wrapping_*`/`saturating_*`), an uninitialized variable,
   a missing NULL check where dwmr has an `Option`, a C-only compiler warning. **Check the
   dwmr code before claiming this** - name the Rust construct that rules the bug out. If dwmr has
   the same logic bug in a different form (e.g. it does not crash, but picks the wrong value or
   skips something dwm now does), it *is* applicable: port the behavioural part.
4. **Not applicable (build)** - `config.mk`/`Makefile` flags, compiler options, `CPPFLAGS`, link
   lines: dwmr builds with Cargo and its own `Makefile`. Take the change only if it has a real
   counterpart (a new library to link - a Cargo dependency or `#[link]`; an install path; a man
   page install step). A version bump in `config.mk` *is* applicable: dwmr's version tracks dwm's
   (`Cargo.toml` `version = "6.8.0"` for dwm 6.8, plus the `dwmr` entry in `Cargo.lock`).
5. **No counterpart** - the commit only touches code dwmr never had or dropped on purpose (dead
   code removal in a function dwmr never ported, a fix inside a patch dwmr does not carry):
   `skipped (no counterpart)`. If dwmr has the same dead code, port the removal.
6. **Deviation** - the commit would undo or conflict with a deliberate dwmr difference (see
   "dwmr's deliberate differences"). Do not decide this alone: **ask** (see section 6), recommend
   a path, and record `asked`. After the answer it is `adapted`/`ported` with `requested`, or
   `skipped (deviation)` / `skipped (declined)` with `requested`.
7. **Otherwise port it.** The user's own dwm commits are the user's intent for their window
   manager - a behaviour change, a new key, a config change, a fix in a patch dwmr carries - and
   dwmr is meant to track it. Suckless commits that land in the fork are bug fixes and small
   improvements to code dwmr replicates - port them unless one of the points above applies.

When a commit is applicable only in part, port that part and record `partial` with the notes
saying which hunks were left out and why (`needed`). When genuinely unsure whether a commit is
beneficial in dwmr - it is not covered by the points above, or only arguably - **ask** rather
than guess.

### dwmr's deliberate differences

Check a commit against these before porting it. Each is a reason to adapt rather than translate,
or to ask:

- **Never crash** (dwmr-dev priority 1): no `unwrap`/`expect` on runtime data, no unchecked
  indexing, no overflow, no division by a value that can be zero. A dwm change is never ported
  with a crash in it, even when dwm has one - port it safely and flag it (see "Flagged for dwm").
- **No allocations in the event path** that dwm does not have; no hash maps where dwm walks a list.
- **Configuration**: `Config::default()` in `src/config.rs` is config.def.h; the TOML loader puts
  `~/.config/dwmr/config.toml` on top, with validation that rejects values that could crash. A
  config.h change becomes a `config/config.toml` change; a new option needs all four places of
  the dwmr-dev skill. Anything dwm only does because config.h is compiled in (macros, `#define`s
  for compile-time choices) becomes config or a plain constant.
- **Program names**: the executable is `dwmr`; the status bar program is `dwmblocksr` (config
  `~/.config/dwmblocksr/config.toml`, not `blocks.h`); the autostart/status commands, the
  `kill -NN $(pidof ...)` lines and the "open the blocks config" command name these.
  **The bar's window class stays `"dwm"`** ("Give the bar dwm's window class"), so window rules
  and scripts keep working.
- **Deviations commented in the code** (`updategeom_xinerama`'s `nn == 0` case and others): find
  them with `grep -n 'dwm ' src/*.rs` / "deviat" / "unlike dwm" near the code a commit touches.
- **Colours** carry an opaque alpha byte (borders under picom).
- **xresources** live in `src/xres.rs` and override the config at startup.
- dwmr's `git log` records further decisions - when in doubt, read the history of the code a
  commit touches (`git log -S '<identifier>' --oneline -- src/`).

### The file map

| dwm (`~/.config/dwm`) | dwmr | Kind |
| --- | --- | --- |
| `dwm.c` | `src/dwm.rs` (`main()`/argv in `src/main.rs`, the xresources code in `src/xres.rs`) | code |
| `vanitygaps.c` | `src/vanitygaps.rs` | code |
| `drw.c`, `drw.h` | `src/drw.rs` (fontconfig FFI in `src/fontconfig.rs`) | code |
| `util.c`, `util.h` | `src/util.rs` | code |
| `transient.c` | `examples/transient.rs` | code |
| `config.def.h` | `Config::default()` in `src/config.rs` (+ `RawConfig`/`parse()`/`parse_func()` etc. for a new option) | config |
| `config.h` | `config/config.toml`, and the user's `~/.config/dwmr/config.toml` (see section 5) | config |
| `dwm.1`, `README` | `dwmr.1`, `README.md` (only content that applies to dwmr) | docs |
| `config.mk`, `Makefile` | `Cargo.toml`, `Makefile` - usually not applicable (see point 4) | build |
| `.claude/`, `patches/`, `*.diff`, `gen_diffs.sh`, `UPSTREAM_SYNC.md`, `upstream_info.txt`, `dwm.png`, `.gitignore`, `LICENSE` | nothing | housekeeping |

A file not in the table: read it and decide; add it to the table in this skill if it recurs.

## 5. Port each commit, in order

The work list is the ledger's `pending` rows, oldest first (for `<dwm-commit>`, just that one).
If nothing is left, say dwmr is caught up and stop.

For each commit in the work list:

1. **Read and decide** per section 4. A skip needs no dwmr commit: fill in the row, then move on
   (it is committed with the next ledger commit, see "Committing the ledger").
2. **Flagged for dwm** - while reading, check the commit's **own change** (not the code around
   it) for an evident bug: a crash, an out-of-bounds access, a leak, a wrong condition, a value
   used before it is set, behaviour that contradicts the commit's own message. Style is not a
   bug. When there is one, port the commit's intent **without** the bug (dwmr must not crash;
   Handling `needed`), carry on, and add an entry under "Flagged for dwm": the row and commit,
   the dwm file and function, what goes wrong, a repro, the suggested fix for dwm, and the
   decision, `open` until the user decides. Put `flagged` in the row's notes. Do not edit dwm.
3. **Port it** following the dwmr-dev skill: map each hunk to the Rust function of the same name,
   keep dwm's comments, naming and order, new functions as `Dwm` methods in dwm's position, new
   globals as `Dwm` fields.
4. **Config** - a `config.def.h` change goes into `Config::default()` (and the TOML shape,
   `parse()` and its validation, `parse_func()`/`parse_arrange()`/`parse_arg()` when new); a
   `config.h` change goes into `config/config.toml`, commented like config.h (key bindings with
   their `/* bind ... */` comment), with the program-name swaps above. Then the **user's copy**,
   `~/.config/dwmr/config.toml`: if it was identical to the shipped `config/config.toml` before
   this change (`git show HEAD:config/config.toml | diff - ~/.config/dwmr/config.toml`), apply the
   same change to it so the two stay identical; otherwise leave it alone and say in the report
   exactly what to add. This standing rule is the user's request and overrides dwmr-dev's "do not
   edit it unless asked". The user's copy is never part of a commit, and no backup copy is made.
5. **Docs** - update `dwmr.1` and `README.md` when user-visible behaviour, keys or config options
   change (dwmr-dev step 4), even when the dwm commit did not touch `dwm.1`.
6. **Ask first** - stop before committing when the port needs a decision that is the user's:
   a deliberate dwmr difference would change, a dwmr feature would behave differently, the change
   is applicable only arguably, or porting it safely needs a restructure dwm's change does not
   have. Explain what the commit does, what would change in dwmr and why, with a recommended path,
   and wait. Leave the tree as it was (`git reset --hard HEAD && git clean -fd` after showing the
   planned change - the tree was clean at the start; restore the user's config copy if it was
   touched), record the row
   as `asked` with the question under "Open questions", and do not continue with later commits.
   Porting it after a "yes" is `requested`.
7. **Verify** (see below). Fix failures caused by this port; report pre-existing ones.
8. **Commit** - one dwmr commit per dwm commit, never squashed:

   ```bash
   git commit -m "<dwm subject>" -m "<body>" -m "dwm-commit: <full dwm sha>"
   ```

   - **Subject**: the dwm commit's subject, with program names swapped (`dwmblocks` ->
     `dwmblocksr`, `blocks.h` -> `config.toml` where it names the blocks config). Keep it
     otherwise unchanged; the only other adjustments are the ones earlier mirrored commits made:
     a `config:` prefix when the dwmr change is config-only, or a function name dwmr spells
     differently. Example: dwm `Status bar font Propo, start dwmblocks with sb-start` -> dwmr
     `config: status bar font Propo, start dwmblocksr with sb-start`.
   - **Body**: what the change does in dwmr and why (dwm's body is the starting point; rewrite
     what does not apply), what was adapted from the dwm diff, and what was tested. Wrap at 72.
   - **Trailer**: the last line is `dwm-commit: <full 40-char dwm sha>`, so a rebuild can match
     the commit exactly. For a `partial` port the trailer is the same; the body says what was
     left out.
   - **No `Co-Authored-By`** or any other attribution line, on this commit or the ledger commit.
   - The ledger is **not** part of this commit: edit `UPSTREAM_SYNC.md` and `upstream_info.txt`
     only after it exists.
9. **Record** the row (status, origin, kind, handling, dwmr commit, notes), its Special handling /
   Flagged for dwm entries, add pending rows for new dwm commits, move the high-water mark,
   update `upstream_info.txt`, and commit the ledger:

   ```bash
   git add UPSTREAM_SYNC.md upstream_info.txt && git commit -m "update upstream sync ledger"
   ```

Then move on to the next commit.

### Committing the ledger

The ledger and `upstream_info.txt` are committed in the `update upstream sync ledger` commit after
each ported commit (step 9). A run that changes them without porting a commit - skips,
`asked`, `skip`, `setup`, `rebuild` - commits them the same way. When such decisions come before a
ported commit in the same run, fold them into that commit's ledger commit instead. A run of
nothing but housekeeping skips makes one ledger commit at the end. No attribution trailer.

### Replay

`replay <dwm-commit>` never rewrites a commit already in `main` (it may be pushed). For a ported
commit, the correction - the rest of a `partial` port, a wrong translation, a fix the user wants
after a flag - is a new dwmr commit on top, named for what it does and the dwm commit it follows
up (e.g. `focusstack: <what was fixed> (follow-up to dwm <short sha>)`), with the same
`dwm-commit:` trailer, verified like any other, followed by the ledger commit. The existing row is
updated in place, never duplicated: Handling becomes `requested` when the user asked for it, the
Special handling entry says what was redone and quotes the request. For a commit not ported yet
(`asked`, `skipped`), replay ports it per this section.

## How much per run

One dwm commit at a time - one dwmr commit (plus its ledger commit) per ported dwm commit - but
keep going through the work list until roughly **500 changed lines** of dwm diff have been ported
in the run (`git -C ~/.config/dwm show --numstat <sha>`, not counting housekeeping files or
skipped commits), or stop earlier at a logical point: a question for the user, a large commit
better done in its own run, or a group of related commits just completed (a fix and its
follow-up). Do not stop in the middle of such a group because the budget is reached; finish it,
then stop. Skips cost nothing against the budget - decide them all the way to the next commit
that needs porting.

Do not push. The user pushes. Do not run `sudo make install`; that is the user's call.

## Verification

dwmr-dev's "Verifying a change", every time code or config changed:

```sh
cargo build && cargo clippy --all-targets   # no new warnings from the changed code
cargo test                                   # includes shipped_config_is_config_h
cargo build --release --no-default-features
cargo build --release
```

For a `code` commit that changes behaviour (events, focus, layout, bar drawing, EWMH, monitors),
also run dwmr-dev's functional test in a local Xvfb and exercise what the commit changed
(e.g. check the property with `xprop`, the geometry with `xwininfo`). A config-only commit needs
`cargo test` passing with the new `config/config.toml`. If a check cannot run, say so; never
report it as passed.

## Report

At the end, list per dwm commit: sha, date, origin, kind, subject, the status and handling with
the skip reason or what was adapted, or the question it stopped on. Give any "Flagged for dwm"
entry with its repro and suggested fix, any special handling with its reason, and what was done to
`~/.config/dwmr/config.toml` (updated, or what the user must add). Say how many `pending` rows were
added and which rows a rebuild changed. Include the checks run and their results, the dwmr commits
created, the new high-water mark and `upstream_info.txt`, and how many rows remain. Remind the
user that the dwmc sync (`/dwmc-upstream-sync` in `~/Code2/C/dwmc`) can now follow the rows decided
in this run. If stopped on a question, name the commit it is waiting on.
