# dwmr upstream sync ledger

Maintained by the `dwmr-upstream-sync` skill. A hint, not the source of truth - the code is.

Source: the user's dwm fork, `~/.config/dwm` `master` (<https://github.com/ornfelt/dwm>).
Baseline: `723a850` - Status bar font Propo, start dwmblocks with sb-start (2026-10-31), dwmr `ffeda55`.
Everything up to and including the baseline was ported before this ledger existed.
High-water mark: `723a850` `Status bar font Propo, start dwmblocks with sb-start` (the baseline; no rows yet)
dwm HEAD when last checked: `723a850` `Status bar font Propo, start dwmblocks with sb-start` (2026-11-02)
Remaining: `0` (`pending` and `asked` rows)

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

None yet.

## Flagged for dwm

None yet.

## Open questions

None yet.
