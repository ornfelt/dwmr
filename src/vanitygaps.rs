/* See LICENSE file for copyright and license details. */
//! vanitygaps.c of dwm's cfacts-vanitygaps combo patch: gaps between windows
//! and the screen edge, and the gap-aware layouts. It is a child module of
//! dwm.rs (like `#include "vanitygaps.c"`), so it adds methods to `Dwm`.
//!
//! cfacts: every client has a weight (cfact, 1.0 unless setcfact() changes
//! it), and the clients in an area of a layout share it by weight.

use crate::config::Arg;
use crate::util::truncate_utf8;

use super::{height, width, ClientId, Dwm, MonId, LTSYMBOL_SIZE};

/// The largest gap setgaps() stores and the config accepts. Not in the patch:
/// it keeps the gap arithmetic in the layouts far from overflowing.
pub const GAP_MAX: i32 = 1000;

impl Dwm {
    fn setgaps(&mut self, mut oh: i32, mut ov: i32, mut ih: i32, mut iv: i32) {
        if oh < 0 {
            oh = 0;
        }
        if ov < 0 {
            ov = 0;
        }
        if ih < 0 {
            ih = 0;
        }
        if iv < 0 {
            iv = 0;
        }

        let selmon = self.selmon;
        let m = &mut self.mons[selmon];
        m.gappoh = oh.min(GAP_MAX);
        m.gappov = ov.min(GAP_MAX);
        m.gappih = ih.min(GAP_MAX);
        m.gappiv = iv.min(GAP_MAX);
        self.arrange(Some(selmon));
    }

    pub fn togglegaps(&mut self, _arg: &Arg) {
        self.enablegaps = !self.enablegaps;
        self.arrange(None);
    }

    pub fn togglebgaps(&mut self, _arg: &Arg) {
        self.browsergaps = !self.browsergaps;
        self.arrange(None);
    }

    pub fn defaultgaps(&mut self, _arg: &Arg) {
        let config = &self.config;
        let (oh, ov, ih, iv) = (config.gappoh as i32, config.gappov as i32, config.gappih as i32, config.gappiv as i32);
        self.setgaps(oh, ov, ih, iv);
    }

    pub fn incrgaps(&mut self, arg: &Arg) {
        let m = &self.mons[self.selmon];
        let i = arg.i();
        self.setgaps(
            m.gappoh.saturating_add(i),
            m.gappov.saturating_add(i),
            m.gappih.saturating_add(i),
            m.gappiv.saturating_add(i),
        );
    }

    /// `getgaps(m, &oh, &ov, &ih, &iv, &nc)`: returns `(oh, ov, ih, iv, n)`.
    fn getgaps(&self, m: MonId) -> (i32, i32, i32, i32, i32) {
        let mut oe = self.enablegaps;
        let ie = self.enablegaps;

        let mut n = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(i) = c {
            n += 1;
            c = self.nexttiled(self.clients[i].next);
        }
        if self.config.smartgaps && n == 1 {
            oe = false; // outer gaps disabled when only one client
        }

        if n == 1 && self.nexttiled(self.mons[m].clients).is_some_and(|c| self.clients[c].isbrowser) && !self.browsergaps {
            oe = false; // outer gaps disabled when only one client (and it's Firefox)
        }

        let mon = &self.mons[m];
        (
            if oe { mon.gappoh } else { 0 }, // outer horizontal gap
            if oe { mon.gappov } else { 0 }, // outer vertical gap
            if ie { mon.gappih } else { 0 }, // inner horizontal gap
            if ie { mon.gappiv } else { 0 }, // inner vertical gap
            n,                               // number of clients
        )
    }

    /// The size of client `c` in an area of size `total` shared by clients
    /// whose weights add up to `facts` (so facts > 0). The patch computes
    /// total * (c->cfact / facts); dividing last keeps equal weights at
    /// exactly total / n.
    fn cfactsize(&self, c: ClientId, total: i32, facts: f32) -> i32 {
        (total as f32 * self.clients[c].cfact / facts) as i32
    }

    /// `getfacts(m, msize, ssize, &mf, &sf, &mr, &sr)` of the cfacts patch:
    /// the total weight of the master and of the stack clients, and the
    /// pixels left after each client got its cfactsize() (the first ones get
    /// one more).
    fn getfacts(&self, m: MonId, msize: i32, ssize: i32) -> (f32, f32, i32, i32) {
        let nmaster = self.mons[m].nmaster;
        let (mut mfacts, mut sfacts) = (0.0, 0.0);

        let mut n = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(i) = c {
            if n < nmaster {
                mfacts += self.clients[i].cfact;
            } else {
                sfacts += self.clients[i].cfact;
            }
            c = self.nexttiled(self.clients[i].next);
            n += 1;
        }

        let (mut mtotal, mut stotal) = (0, 0);
        let mut n = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(i) = c {
            if n < nmaster {
                mtotal += self.cfactsize(i, msize, mfacts);
            } else {
                stotal += self.cfactsize(i, ssize, sfacts);
            }
            c = self.nexttiled(self.clients[i].next);
            n += 1;
        }

        (
            mfacts,          // total factor of master area
            sfacts,          // total factor of stack area
            msize - mtotal, // the remainder (rest) of pixels after a cfacts master split
            ssize - stotal, // the remainder (rest) of pixels after a cfacts stack split
        )
    }

    /// The window area, nmaster and mfact of `m` for a layout of `n` > 0
    /// tiled clients. Not in the patch: nmaster is clamped to 0..=n, which
    /// changes no layout (i < nmaster and n > nmaster test the same) but keeps
    /// the gap arithmetic in range for any nmaster.
    fn layoutarea(&self, m: MonId, n: i32) -> (i32, i32, i32, i32, i32, f32) {
        let mon = &self.mons[m];
        (mon.wx, mon.wy, mon.ww, mon.wh, mon.nmaster.clamp(0, n.max(0)), mon.mfact)
    }

    /***
     * Layouts
     */

    /*
     * Bottomstack layout + gaps
     * https://dwm.suckless.org/patches/bottomstack/
     */
    pub fn bstack(&mut self, m: MonId) {
        let (oh, ov, ih, iv, n) = self.getgaps(m);
        if n == 0 {
            return;
        }

        let (wx, wy, ww, wh, nmaster, mfact) = self.layoutarea(m, n);
        let (mut mx, my) = (wx + ov, wy + oh);
        let (mut sx, mut sy) = (mx, my);
        let mut mh = wh - 2 * oh;
        let mut sh = mh;
        let mw = ww - 2 * ov - iv * (n.min(nmaster) - 1);
        let sw = ww - 2 * ov - iv * (n - nmaster - 1);

        if nmaster != 0 && n > nmaster {
            sh = ((mh - ih) as f32 * (1.0 - mfact)) as i32;
            mh = mh - ih - sh;
            sx = mx;
            sy = my + mh + ih;
        }

        let (mf, sf, mrest, srest) = self.getfacts(m, mw, sw);

        let mut i = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(k) = c {
            let bw = self.clients[k].bw;
            if i < nmaster {
                self.resize(k, mx, my, self.cfactsize(k, mw, mf) + (i < mrest) as i32 - (2 * bw), mh - (2 * bw), false);
                mx += width(&self.clients[k]) + iv;
            } else {
                self.resize(k, sx, sy, self.cfactsize(k, sw, sf) + ((i - nmaster) < srest) as i32 - (2 * bw), sh - (2 * bw), false);
                sx += width(&self.clients[k]) + iv;
            }
            c = self.nexttiled(self.clients[k].next);
            i += 1;
        }
    }

    /*
     * Centred master layout + gaps
     * https://dwm.suckless.org/patches/centeredmaster/
     */
    pub fn centeredmaster(&mut self, m: MonId) {
        let (oh, ov, ih, iv, n) = self.getgaps(m);
        if n == 0 {
            return;
        }

        let (wx, wy, ww, wh, nmaster, mfact) = self.layoutarea(m, n);
        let (mut lx, mut ly, mut lw) = (0, 0, 0);
        let (mut rx, mut ry, mut rw) = (0, 0, 0);

        /* initialize areas */
        let mut mx = wx + ov;
        let mut my = wy + oh;
        let mh = wh - 2 * oh - ih * ((if nmaster == 0 { n } else { n.min(nmaster) }) - 1);
        let mut mw = ww - 2 * ov;
        let lh = wh - 2 * oh - ih * (((n - nmaster) / 2) - 1);
        let rh = wh - 2 * oh - ih * (((n - nmaster) / 2) - if (n - nmaster) % 2 != 0 { 0 } else { 1 });

        if nmaster != 0 && n > nmaster {
            /* go mfact box in the center if more than nmaster clients */
            if n - nmaster > 1 {
                /* ||<-S->|<---M--->|<-S->|| */
                mw = ((ww - 2 * ov - 2 * iv) as f32 * mfact) as i32;
                lw = (ww - mw - 2 * ov - 2 * iv) / 2;
                rw = (ww - mw - 2 * ov - 2 * iv) - lw;
                mx += lw + iv;
            } else {
                /* ||<---M--->|<-S->|| */
                mw = ((mw - iv) as f32 * mfact) as i32;
                lw = 0;
                rw = ww - mw - iv - 2 * ov;
            }
            lx = wx + ov;
            ly = wy + oh;
            rx = mx + mw + iv;
            ry = wy + oh;
        }

        /* calculate facts */
        let (mut mfacts, mut lfacts, mut rfacts) = (0.0, 0.0, 0.0);
        let mut k = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(i) = c {
            if nmaster == 0 || k < nmaster {
                mfacts += self.clients[i].cfact;
            } else if (k - nmaster) % 2 != 0 {
                lfacts += self.clients[i].cfact; // total factor of left hand stack area
            } else {
                rfacts += self.clients[i].cfact; // total factor of right hand stack area
            }
            c = self.nexttiled(self.clients[i].next);
            k += 1;
        }

        let (mut mtotal, mut ltotal, mut rtotal) = (0, 0, 0);
        let mut k = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(i) = c {
            if nmaster == 0 || k < nmaster {
                mtotal += self.cfactsize(i, mh, mfacts);
            } else if (k - nmaster) % 2 != 0 {
                ltotal += self.cfactsize(i, lh, lfacts);
            } else {
                rtotal += self.cfactsize(i, rh, rfacts);
            }
            c = self.nexttiled(self.clients[i].next);
            k += 1;
        }
        let mrest = mh - mtotal;
        let lrest = lh - ltotal;
        let rrest = rh - rtotal;

        let mut i = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(k) = c {
            let bw = self.clients[k].bw;
            if nmaster == 0 || i < nmaster {
                /* nmaster clients are stacked vertically, in the center of the screen */
                self.resize(k, mx, my, mw - (2 * bw), self.cfactsize(k, mh, mfacts) + (i < mrest) as i32 - (2 * bw), false);
                my += height(&self.clients[k]) + ih;
            } else {
                /* stack clients are stacked vertically; the patch tests
                 * (i - 2*nmaster) < 2*rest in unsigned arithmetic, which skips
                 * the first clients of a side and loses the rest pixels when
                 * nmaster > 2, so the first rest clients of each side get one
                 * pixel more here: (i - nmaster) / 2 is the index on its side */
                if (i - nmaster) % 2 != 0 {
                    self.resize(k, lx, ly, lw - (2 * bw), self.cfactsize(k, lh, lfacts) + ((i - nmaster) / 2 < lrest) as i32 - (2 * bw), false);
                    ly += height(&self.clients[k]) + ih;
                } else {
                    self.resize(k, rx, ry, rw - (2 * bw), self.cfactsize(k, rh, rfacts) + ((i - nmaster) / 2 < rrest) as i32 - (2 * bw), false);
                    ry += height(&self.clients[k]) + ih;
                }
            }
            c = self.nexttiled(self.clients[k].next);
            i += 1;
        }
    }

    pub fn centeredfloatingmaster(&mut self, m: MonId) {
        let mut mivf: f32 = 1.0; // master inner vertical gap factor

        let (oh, ov, _, iv, n) = self.getgaps(m);
        if n == 0 {
            return;
        }

        let (wx, wy, ww, wh, nmaster, mfact) = self.layoutarea(m, n);
        let (mut mx, mut my) = (wx + ov, wy + oh);
        let (mut sx, mut sy) = (mx, my);
        let mut mh = wh - 2 * oh;
        let mut sh = mh;
        let mut mw = ww - 2 * ov - iv * (n - 1);
        let sw = ww - 2 * ov - iv * (n - nmaster - 1);

        if nmaster != 0 && n > nmaster {
            mivf = 0.8;
            /* go mfact box in the center if more than nmaster clients */
            if ww > wh {
                mw = (ww as f32 * mfact - iv as f32 * mivf * (n.min(nmaster) - 1) as f32) as i32;
                mh = (wh as f64 * 0.9) as i32;
            } else {
                mw = (ww as f64 * 0.9 - (iv as f32 * mivf * (n.min(nmaster) - 1) as f32) as f64) as i32;
                mh = (wh as f32 * mfact) as i32;
            }
            mx = wx + (ww - mw) / 2;
            my = wy + (wh - mh - 2 * oh) / 2;

            sx = wx + ov;
            sy = wy + oh;
            sh = wh - 2 * oh;
        }

        let (mf, sf, mrest, srest) = self.getfacts(m, mw, sw);

        let mut i = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(k) = c {
            let bw = self.clients[k].bw;
            if i < nmaster {
                /* nmaster clients are stacked horizontally, in the center of the screen */
                self.resize(k, mx, my, self.cfactsize(k, mw, mf) + (i < mrest) as i32 - (2 * bw), mh - (2 * bw), false);
                mx = (mx as f32 + (width(&self.clients[k]) as f32 + iv as f32 * mivf)) as i32;
            } else {
                /* stack clients are stacked horizontally */
                self.resize(k, sx, sy, self.cfactsize(k, sw, sf) + ((i - nmaster) < srest) as i32 - (2 * bw), sh - (2 * bw), false);
                sx += width(&self.clients[k]) + iv;
            }
            c = self.nexttiled(self.clients[k].next);
            i += 1;
        }
    }

    /*
     * Deck layout + gaps
     * https://dwm.suckless.org/patches/deck/
     */
    pub fn deck(&mut self, m: MonId) {
        let (oh, ov, ih, iv, n) = self.getgaps(m);
        if n == 0 {
            return;
        }

        let (wx, wy, ww, wh, nmaster, mfact) = self.layoutarea(m, n);
        let (mx, mut my) = (wx + ov, wy + oh);
        let (mut sx, sy) = (mx, my);
        let mh = wh - 2 * oh - ih * (n.min(nmaster) - 1);
        let mut sh = mh;
        let mut mw = ww - 2 * ov;
        let mut sw = mw;

        if nmaster != 0 && n > nmaster {
            sw = ((mw - iv) as f32 * (1.0 - mfact)) as i32;
            mw = mw - iv - sw;
            sx = mx + mw + iv;
            sh = wh - 2 * oh;
        }

        let (mf, _, mrest, _) = self.getfacts(m, mh, sh);

        /* override layout symbol; the patch tests n - nmaster > 0 unsigned,
         * which shows "D -1" when there are fewer clients than nmaster */
        if n - nmaster > 0 {
            let mut symbol = format!("D {}", n - nmaster);
            truncate_utf8(&mut symbol, LTSYMBOL_SIZE - 1);
            self.mons[m].ltsymbol = symbol;
        }

        let mut i = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(k) = c {
            let bw = self.clients[k].bw;
            if i < nmaster {
                self.resize(k, mx, my, mw - (2 * bw), self.cfactsize(k, mh, mf) + (i < mrest) as i32 - (2 * bw), false);
                my += height(&self.clients[k]) + ih;
            } else {
                self.resize(k, sx, sy, sw - (2 * bw), sh - (2 * bw), false);
            }
            c = self.nexttiled(self.clients[k].next);
            i += 1;
        }
    }

    /*
     * Fibonacci layout + gaps
     * https://dwm.suckless.org/patches/fibonacci/
     */
    fn fibonacci(&mut self, m: MonId, s: bool) {
        let (mut hrest, mut wrest, mut r) = (0, 0, true);

        let (oh, ov, ih, iv, n) = self.getgaps(m);
        if n == 0 {
            return;
        }

        let (wx, wy, ww, wh, mfact) = (self.mons[m].wx, self.mons[m].wy, self.mons[m].ww, self.mons[m].wh, self.mons[m].mfact);
        let bh = self.bh;
        let mut nx = wx + ov;
        let mut ny = wy + oh;
        let mut nw = ww - 2 * ov;
        let mut nh = wh - 2 * oh;

        let mut i = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(k) = c {
            let bw = self.clients[k].bw;
            if r {
                if (i % 2 != 0 && (nh - ih) / 2 <= (bh + 2 * bw)) || (i % 2 == 0 && (nw - iv) / 2 <= (bh + 2 * bw)) {
                    r = false;
                }
                if r && i < n - 1 {
                    if i % 2 != 0 {
                        let nv = (nh - ih) / 2;
                        hrest = nh - 2 * nv - ih;
                        nh = nv;
                    } else {
                        let nv = (nw - iv) / 2;
                        wrest = nw - 2 * nv - iv;
                        nw = nv;
                    }

                    if (i % 4) == 2 && !s {
                        nx += nw + iv;
                    } else if (i % 4) == 3 && !s {
                        ny += nh + ih;
                    }
                }

                if (i % 4) == 0 {
                    if s {
                        ny += nh + ih;
                        nh += hrest;
                    } else {
                        nh -= hrest;
                        ny -= nh + ih;
                    }
                } else if (i % 4) == 1 {
                    nx += nw + iv;
                    nw += wrest;
                } else if (i % 4) == 2 {
                    ny += nh + ih;
                    nh += hrest;
                    if i < n - 1 {
                        nw += wrest;
                    }
                } else if (i % 4) == 3 {
                    if s {
                        nx += nw + iv;
                        nw -= wrest;
                    } else {
                        nw -= wrest;
                        nx -= nw + iv;
                        nh += hrest;
                    }
                }
                if i == 0 {
                    if n != 1 {
                        let a = ww - iv - 2 * ov;
                        nw = (a as f32 - a as f32 * (1.0 - mfact)) as i32;
                        wrest = 0;
                    }
                    ny = wy + oh;
                } else if i == 1 {
                    nw = ww - nw - iv - 2 * ov;
                }
                i += 1;
            }

            self.resize(k, nx, ny, nw - (2 * bw), nh - (2 * bw), false);
            c = self.nexttiled(self.clients[k].next);
        }
    }

    pub fn dwindle(&mut self, m: MonId) {
        self.fibonacci(m, true);
    }

    pub fn spiral(&mut self, m: MonId) {
        self.fibonacci(m, false);
    }

    /*
     * Default tile layout + gaps
     */
    pub fn tile(&mut self, m: MonId) {
        let (oh, ov, ih, iv, n) = self.getgaps(m);
        if n == 0 {
            return;
        }

        let (wx, wy, ww, wh, nmaster, mfact) = self.layoutarea(m, n);
        let (mx, mut my) = (wx + ov, wy + oh);
        let (mut sx, mut sy) = (mx, my);
        let mh = wh - 2 * oh - ih * (n.min(nmaster) - 1);
        let sh = wh - 2 * oh - ih * (n - nmaster - 1);
        let mut mw = ww - 2 * ov;
        let mut sw = mw;

        if nmaster != 0 && n > nmaster {
            sw = ((mw - iv) as f32 * (1.0 - mfact)) as i32;
            mw = mw - iv - sw;
            sx = mx + mw + iv;
        }

        let (mf, sf, mrest, srest) = self.getfacts(m, mh, sh);

        let mut i = 0;
        let mut c = self.nexttiled(self.mons[m].clients);
        while let Some(k) = c {
            let bw = self.clients[k].bw;
            if i < nmaster {
                self.resize(k, mx, my, mw - (2 * bw), self.cfactsize(k, mh, mf) + (i < mrest) as i32 - (2 * bw), false);
                my += height(&self.clients[k]) + ih;
            } else {
                self.resize(k, sx, sy, sw - (2 * bw), self.cfactsize(k, sh, sf) + ((i - nmaster) < srest) as i32 - (2 * bw), false);
                sy += height(&self.clients[k]) + ih;
            }
            c = self.nexttiled(self.clients[k].next);
            i += 1;
        }
    }
}
