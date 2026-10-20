//! The exposé: hovering a slot's tile spreads that slot's windows across its monitor as
//! previews (snapshots from `snap.rs`), drawn with DirectComposition on an overlay behind the
//! panel and animated by the compositor. State lives in `gpu::Gpu` (see `State`), so losing
//! the device drops it with everything else.

use crate::core::Entry;
use crate::gpu::{self, Gpu, Overlay};
use crate::win;
use std::collections::HashMap;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::D2D1_BORDER_MODE_HARD;
use windows::Win32::Graphics::DirectComposition::*;
use windows::core::Interface;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

const OPEN_MS: u64 = 250;
const CLOSE_MS: u64 = 180;
const CHOOSE_MS: u64 = 220;
const LIFT_MS: u64 = 150;
const LAND_MS: u64 = 220;
/// A lifted preview fits this (like a dragged window's ghost), scaled for the monitor.
const LIFT_MAX: (f32, f32) = (360.0, 240.0);
/// Darkening without a frosted backdrop (it couldn't be captured), and with one.
const DIM: f32 = 0.45;
const TINT: f32 = 0.3;
/// Backdrop blur (standard deviation, px).
const BLUR: f32 = 28.0;
const GAP: f32 = 32.0;
/// Preview icons: fade, inset from the preview's corner, share of its shorter side, size range
/// (all in 100% pixels), and the smallest preview that carries one.
const ICON_FADE_MS: u64 = 150;
const ICON_OUT_MS: u64 = 90;
const ICON_INSET: f32 = 8.0;
const ICON_FRAC: f32 = 0.22;
const ICON_RANGE: (f32, f32) = (32.0, 72.0);
const ICON_MIN_CELL: f32 = 120.0;
/// Icon pictures kept on the GPU (apps, not windows).
const ICON_SURFACES: usize = 64;
/// The close badge's diameter at 100%. It straddles the preview's top-right corner (clear of
/// the window's own close button); the margin and the gap between previews leave room for it.
const BADGE: f32 = 32.0;
/// A preview whose window was asked to close.
const DIMMED: f32 = 0.4;
const MARGIN: f32 = 48.0;
const HIGHLIGHT: f32 = 4.0;
const ACCENT: u32 = 0xFF00_78D7; // BGRA of the panel accent
/// Cards (windows without a picture) are drawn this wide, at the window's aspect.
const CARD_W: i32 = 320;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    /// The largest rect of aspect `w`:`h` centered inside this one.
    fn fit(&self, w: f32, h: f32) -> Rect {
        let k = (self.w / w).min(self.h / h);
        let (fw, fh) = (w * k, h * k);
        Rect { x: self.x + (self.w - fw) / 2.0, y: self.y + (self.h - fh) / 2.0, w: fw, h: fh }
    }
}

/// Lays out windows of the given sizes in reading order: the column count that gives the
/// most preview area wins; previews keep their aspect, never grow past real size, and are
/// centered in their cells; a short last row is centered.
pub fn grid(sizes: &[(f32, f32)], area: Rect, gap: f32) -> Vec<Rect> {
    let n = sizes.len();
    let scale = |(w, h): (f32, f32), cw: f32, ch: f32| (cw / w).min(ch / h).min(1.0);
    let mut best: Option<(f32, usize)> = None;
    for c in 1..=n {
        let r = n.div_ceil(c);
        let cw = (area.w - gap * (c - 1) as f32) / c as f32;
        let ch = (area.h - gap * (r - 1) as f32) / r as f32;
        if cw <= 0.0 || ch <= 0.0 {
            continue;
        }
        let total: f32 = sizes.iter().map(|&s| scale(s, cw, ch).powi(2) * s.0 * s.1).sum();
        if best.is_none_or(|(t, _)| total > t) {
            best = Some((total, c));
        }
    }
    let Some((_, c)) = best else { return vec![] };
    let r = n.div_ceil(c);
    let cw = (area.w - gap * (c - 1) as f32) / c as f32;
    let ch = (area.h - gap * (r - 1) as f32) / r as f32;
    sizes
        .iter()
        .enumerate()
        .map(|(i, &(w, h))| {
            let (row, col) = (i / c, i % c);
            let in_row = (n - row * c).min(c);
            let shift = (c - in_row) as f32 * (cw + gap) / 2.0;
            let cell = Rect { x: area.x + shift + col as f32 * (cw + gap), y: area.y + row as f32 * (ch + gap), w: cw, h: ch };
            let k = scale((w, h), cw, ch);
            let (pw, ph) = (w * k, h * k);
            Rect { x: cell.x + (cw - pw) / 2.0, y: cell.y + (ch - ph) / 2.0, w: pw, h: ph }
        })
        .collect()
}

struct Preview {
    id: u64,
    visual: IDCompositionVisual2,
    scale: IDCompositionScaleTransform,
    fx: IDCompositionEffectGroup,
    /// content size in pixels
    sw: f32,
    sh: f32,
    /// where it grows out of and folds back to
    home: Rect,
    cell: Rect,
    from_tile: bool,
    /// Scale center in content pixels (the grab point once dragged; 0 before).
    cx: f32,
    cy: f32,
    /// Dropped elsewhere: faded out, ignored from then on.
    gone: bool,
    /// The app icon at its bottom-left, once known.
    icon: Option<IDCompositionEffectGroup>,
    /// The close x at its top-right.
    badge: Option<Badge>,
    /// Its window was asked to close: the preview is inert until that's settled.
    dimmed: bool,
}

struct Badge {
    visual: IDCompositionVisual2,
    fx: IDCompositionEffectGroup,
    /// in monitor coordinates
    rect: Rect,
    size: i32,
}

/// The close x: a disc with a white cross, premultiplied BGRA, anti-aliased. `hot`: red, for
/// when the cursor is on it.
fn close_badge(size: i32, hot: bool) -> Vec<u32> {
    let n = size as f32;
    let c = n / 2.0;
    let r = c - 0.5;
    let (rgb, alpha) = if hot { ([0xC4 as f32, 0x2B as f32, 0x1C as f32], 0.95) } else { ([0x2B as f32; 3], 0.85) };
    let (arm, half) = (n * 0.22, n * 0.055);
    let cover = |v: f32| v.clamp(0.0, 1.0);
    (0..size * size)
        .map(|i| {
            let (x, y) = ((i % size) as f32 + 0.5 - c, (i / size) as f32 + 0.5 - c);
            let disc = cover(r - (x * x + y * y).sqrt() + 0.5) * alpha;
            let reach = cover(arm + 0.5 - x.abs()) * cover(arm + 0.5 - y.abs());
            let line = |d: f32| cover(half + 0.5 - d.abs() / std::f32::consts::SQRT_2) * reach;
            let cross = line(x - y).max(line(x + y));
            let a = cross + disc * (1.0 - cross);
            let ch = |k: usize| ((cross + rgb[k] / 255.0 * disc * (1.0 - cross)).min(a) * 255.0).round() as u32;
            (((a * 255.0).round() as u32) << 24) | (ch(0) << 16) | (ch(1) << 8) | ch(2)
        })
        .collect()
}

/// Where a preview's icon goes (the preview is `cell`) and how big it is drawn: a share of the
/// preview's shorter side within a range, never larger than the picture's own `max_px`.
/// None when the preview is too small to carry one.
fn icon_place(cell: Rect, scale: f32, max_px: f32) -> Option<Rect> {
    let short = cell.w.min(cell.h);
    if short < ICON_MIN_CELL * scale {
        return None;
    }
    let size = (short * ICON_FRAC).clamp(ICON_RANGE.0 * scale, ICON_RANGE.1 * scale).min(max_px);
    let inset = ICON_INSET * scale;
    Some(Rect { x: cell.x + inset, y: cell.y + cell.h - inset - size, w: size, h: size })
}

impl Preview {
    /// Offset and scale that put the content exactly on `r`, given the scale center.
    fn place(&self, r: Rect) -> (f32, f32, f32, f32) {
        let (sx, sy) = (r.w / self.sw, r.h / self.sh);
        (r.x - self.cx + self.cx * sx, r.y - self.cy + self.cy * sy, sx, sy)
    }
}

struct Drag {
    /// The dragged preview's window id.
    id: u64,
    /// grab point in content pixels
    c: (f32, f32),
    s0: (f32, f32),
    s1: (f32, f32),
    last: POINT,
    started: u64,
}

struct Set {
    monitor: String,
    slot: usize,
    current: bool,
    mon: RECT,
    root: IDCompositionVisual2,
    dim: IDCompositionEffectGroup,
    dim_to: f32,
    /// The frosted monitor picture: its blur, and its opacity.
    backdrop: Option<(IDCompositionGaussianBlurEffect, IDCompositionEffectGroup)>,
    highlight: (IDCompositionVisual2, IDCompositionScaleTransform, IDCompositionEffectGroup),
    previews: Vec<Preview>,
    entries: Vec<Entry>,
    /// When a closing set's motion is over and it can be removed.
    ends: u64,
    /// A preview was dropped into another slot: the frozen backdrop is out of date.
    dropped: bool,
    /// Holds every preview's icon, above all previews; its opacity fades them all at once.
    icons: IDCompositionVisual2,
    icons_fx: IDCompositionEffectGroup,
    /// The monitor's scale, and when the open motion is over (0: no motion).
    scale: f32,
    settled: u64,
}

#[derive(Default)]
pub struct State {
    overlays: Vec<Overlay>,
    /// Pictures on the GPU, by window (grid size, full resolution); replaced by newer ones.
    surfaces: HashMap<u64, (Pic, Option<Pic>)>,
    cards: HashMap<u64, (IDCompositionSurface, f32, f32)>,
    /// App icon pictures by app key, and each window's key.
    icon_surfaces: HashMap<String, Pic>,
    icon_keys: HashMap<u64, String>,
    /// Close badge pictures by size: (normal, hot).
    badges: HashMap<i32, (Pic, Pic)>,
    /// The preview whose badge is under the cursor.
    hot_close: Option<u64>,
    /// One spread per monitor (several at once in the swipe-opened exposé).
    open: Vec<Set>,
    closing: Vec<Set>,
    /// The window id of the highlighted preview, in whichever spread holds it.
    hovered: Option<u64>,
    drag: Option<Drag>,
    /// Spans every monitor, above the panels, while a preview is dragged; its origin.
    drag_layer: Option<(Overlay, POINT)>,
    /// Dropped previews still flying on the drag layer, and when they're done.
    flying: Vec<(IDCompositionVisual2, u64)>,
}

impl State {
    pub fn overlays(&self) -> impl Iterator<Item = &Overlay> {
        self.overlays.iter().chain(self.drag_layer.as_ref().map(|(o, _)| o))
    }
}

type R<T> = windows::core::Result<T>;
/// A picture on the GPU and its size in pixels.
type Pic = (IDCompositionSurface, f32, f32);

fn client(r: RECT, mon: RECT) -> Rect {
    Rect { x: (r.left - mon.left) as f32, y: (r.top - mon.top) as f32, w: (r.right - r.left) as f32, h: (r.bottom - r.top) as f32 }
}

/// Values of one transition, sharing a begin time; `dur == 0` sets them at once.
struct Tx {
    dcomp: IDCompositionDesktopDevice,
    begin: i64,
    begin_ms: u64,
    dur: u64,
    /// Held at the start values this long before the motion runs.
    delay: u64,
}

impl Tx {
    fn new(g: &Gpu, dur: u64) -> R<Tx> {
        Tx::after(g, dur, 0)
    }

    /// A transition starting `delay_ms` after the next frame.
    fn after(g: &Gpu, dur: u64, delay_ms: u64) -> R<Tx> {
        let (begin, begin_ms) = gpu::begin_time(&g.dcomp, 0)?;
        Ok(Tx { dcomp: g.dcomp.clone(), begin, begin_ms, dur, delay: delay_ms })
    }

    fn anim(&self, from: f32, to: f32) -> R<IDCompositionAnimation> {
        let m = crate::anim::Motion { from, to, start: self.begin_ms, dur: self.dur };
        if self.delay > 0 { gpu::animation_after(&self.dcomp, &m, self.begin, self.delay) } else { gpu::animation(&self.dcomp, &m, self.begin) }
    }

    fn rect(&self, p: &Preview, from: Rect, to: Rect) -> R<()> {
        self.place(p, p.place(from), p.place(to))
    }

    /// Animates (offset x, offset y, scale x, scale y).
    fn place(&self, p: &Preview, a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> R<()> {
        unsafe {
            if self.dur == 0 {
                p.visual.SetOffsetX2(b.0)?;
                p.visual.SetOffsetY2(b.1)?;
                p.scale.SetScaleX2(b.2)?;
                return p.scale.SetScaleY2(b.3);
            }
            p.visual.SetOffsetX(&self.anim(a.0, b.0)?)?;
            p.visual.SetOffsetY(&self.anim(a.1, b.1)?)?;
            p.scale.SetScaleX(&self.anim(a.2, b.2)?)?;
            p.scale.SetScaleY(&self.anim(a.3, b.3)?)
        }
    }

    fn opacity(&self, fx: &IDCompositionEffectGroup, from: f32, to: f32) -> R<()> {
        unsafe { if self.dur == 0 { fx.SetOpacity2(to) } else { fx.SetOpacity(&self.anim(from, to)?) } }
    }
}

/// Adds `child` in front of everything `parent` already holds (`top` = its frontmost child,
/// updated). AddVisual without a reference puts `insertAbove = true` at the *bottom*.
fn stack(parent: &IDCompositionVisual2, child: &IDCompositionVisual2, top: &mut Option<IDCompositionVisual2>) -> R<()> {
    unsafe {
        match top {
            Some(t) => parent.AddVisual(child, true, &*t)?,
            None => parent.AddVisual(child, false, None::<&IDCompositionVisual>)?,
        }
    }
    *top = Some(child.clone());
    Ok(())
}

/// Gives preview `i` of `set` its icon if the app's picture is known and the preview is big
/// enough; it fades in with `tx`.
fn add_icon(tx: &Tx, surfaces: &HashMap<String, Pic>, keys: &HashMap<u64, String>, set: &mut Set, i: usize) -> R<()> {
    let p = &set.previews[i];
    if p.icon.is_some() || p.gone {
        return Ok(());
    }
    let Some((surface, w, h)) = keys.get(&p.id).and_then(|k| surfaces.get(k)) else {
        crate::log!("icon: {:x} has no picture yet (key known: {})", p.id, keys.contains_key(&p.id));
        return Ok(());
    };
    let Some(r) = icon_place(p.cell, set.scale, w.max(*h)) else {
        crate::log!("icon: {:x} preview too small ({}x{})", p.id, p.cell.w, p.cell.h);
        return Ok(());
    };
    crate::log!("icon: {:x} at {:.0},{:.0} {:.0}px (picture {w}x{h})", p.id, r.x, r.y, r.w);
    unsafe {
        let visual = tx.dcomp.CreateVisual()?;
        visual.SetContent(surface)?;
        let st = tx.dcomp.CreateScaleTransform()?;
        st.SetScaleX2(r.w / w)?;
        st.SetScaleY2(r.h / h)?;
        visual.SetTransform(&st)?;
        visual.SetOffsetX2(r.x)?;
        visual.SetOffsetY2(r.y)?;
        let fx = tx.dcomp.CreateEffectGroup()?;
        fx.SetOpacity2(0.0)?;
        visual.SetEffect(&fx)?;
        set.icons.AddVisual(&visual, false, None::<&IDCompositionVisual>)?;
        tx.opacity(&fx, 0.0, 1.0)?;
        set.previews[i].icon = Some(fx); // the container keeps the visual
    }
    Ok(())
}

/// The badge pictures for `size`, uploaded once.
fn badge_pics(g: &mut Gpu, size: i32) -> R<(Pic, Pic)> {
    if let Some(b) = g.expose.badges.get(&size) {
        return Ok(b.clone());
    }
    let up = |g: &Gpu, hot| -> R<Pic> { Ok((gpu::surface_from_pixels(g, size, size, &close_badge(size, hot), true)?, size as f32, size as f32)) };
    let pair = (up(g, false)?, up(g, true)?);
    g.expose.badges.insert(size, pair.clone());
    Ok(pair)
}

/// Gives preview `i` its close badge, fading in with `tx`.
fn add_badge(tx: &Tx, pics: &(Pic, Pic), set: &mut Set, i: usize, size: i32) -> R<()> {
    let p = &set.previews[i];
    if p.badge.is_some() || p.gone {
        return Ok(());
    }
    let rect = badge_place(p.cell, size as f32);
    unsafe {
        let visual = tx.dcomp.CreateVisual()?;
        visual.SetContent(&pics.0.0)?;
        visual.SetOffsetX2(rect.x)?;
        visual.SetOffsetY2(rect.y)?;
        let fx = tx.dcomp.CreateEffectGroup()?;
        fx.SetOpacity2(0.0)?;
        visual.SetEffect(&fx)?;
        set.icons.AddVisual(&visual, false, None::<&IDCompositionVisual>)?;
        tx.opacity(&fx, 0.0, 1.0)?;
        set.previews[i].badge = Some(Badge { visual, fx, rect, size });
    }
    Ok(())
}

/// Where a preview's close badge of `size` pixels goes: centered on the preview's top-right corner.
fn badge_place(cell: Rect, size: f32) -> Rect {
    Rect { x: cell.x + cell.w - size / 2.0, y: cell.y - size / 2.0, w: size, h: size }
}

/// Back to the normal picture on whichever badge was hot.
fn unhot(g: &mut Gpu) -> R<()> {
    let Some(id) = g.expose.hot_close.take() else { return Ok(()) };
    for set in &g.expose.open {
        if let Some(b) = set.previews.iter().find(|p| p.id == id).and_then(|p| p.badge.as_ref())
            && let Some(pair) = g.expose.badges.get(&b.size)
        {
            unsafe { b.visual.SetContent(&pair.0.0)? };
        }
    }
    unsafe { g.dcomp.Commit() }
}

/// A 1×1 surface of one color, stretched by a scale transform.
fn solid(g: &Gpu, bgra: u32) -> R<IDCompositionSurface> {
    gpu::surface_from_pixels(g, 1, 1, &[bgra], true)
}

/// A card for a window without a picture: its icon and title, at the window's aspect.
fn card_pixels(id: u64, aspect: f32) -> (i32, i32, Vec<u32>) {
    let (w, h) = (CARD_W, ((CARD_W as f32 / aspect) as i32).clamp(120, 480));
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let mut px = vec![0xFF2B_2B2B; (w * h) as usize];
        if let Ok(bmp) = CreateDIBSection(Some(screen), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            let old = SelectObject(dc, bmp.into());
            let bg = CreateSolidBrush(COLORREF(0x002B2B2B));
            FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, bg);
            let _ = DeleteObject(bg.into());
            let icon = win::icon_of(id);
            let size = 48;
            let iy = h / 2 - size;
            if !icon.is_invalid() {
                let _ = DrawIconEx(dc, (w - size) / 2, iy, icon, size, size, 0, None, DI_NORMAL);
            }
            let font = CreateFontW(20, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"));
            let old_font = SelectObject(dc, font.into());
            SetTextColor(dc, COLORREF(0x00F0F0F0));
            SetBkMode(dc, TRANSPARENT);
            let mut text: Vec<u16> = win::title_of(win::hwnd(id)).encode_utf16().collect();
            let mut tr = RECT { left: 12, top: iy + size + 12, right: w - 12, bottom: h - 8 };
            DrawTextW(dc, &mut text, &mut tr, DT_CENTER | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX);
            SelectObject(dc, old_font);
            let _ = DeleteObject(font.into());
            let src = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize);
            px = src.iter().map(|p| p | 0xFF00_0000).collect(); // GDI leaves alpha at 0
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
        }
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        (w, h, px)
    }
}

/// The window's grid picture, or a card.
fn content(g: &mut Gpu, id: u64, aspect: f32) -> R<Pic> {
    if let Some((s, _)) = g.expose.surfaces.get(&id) {
        return Ok(s.clone());
    }
    if let Some(s) = g.expose.cards.get(&id) {
        return Ok(s.clone());
    }
    let (w, h, px) = card_pixels(id, aspect);
    let s = (gpu::surface_from_pixels(g, w, h, &px, false)?, w as f32, h as f32);
    g.expose.cards.insert(id, s.clone());
    Ok(s)
}

/// Spreads `entries` (a slot's root windows, topmost first) across `monitor`. `tile`: the
/// hovered tile's screen rect (hidden slots grow out of it). The spread already open on this
/// monitor, if any, collapses; spreads on other monitors are left alone.
pub fn open(monitor: &str, slot: usize, current: bool, entries: &[Entry], tile: Option<RECT>, motion: bool) -> bool {
    let Some(m) = win::monitors().into_iter().find(|m| m.name == monitor) else { return false };
    let Some((panel_hwnd, panel_rect)) = crate::panel::window_of(monitor) else { return false };
    let roots: Vec<(Entry, RECT)> = entries.iter().filter_map(|e| Some((e.clone(), win::frame_rect(e.id)?))).collect();
    if roots.is_empty() {
        collapse_monitor(monitor, motion);
        return false;
    }
    gpu::with(monitor, |g| {
        collapse_monitor_in(g, monitor, motion)?;
        open_in(g, monitor, slot, current, &roots, tile, m.rect, m.work, panel_hwnd, panel_rect, m.scale, motion)
    })
    .is_some()
}

/// Collapses the spread on one monitor only.
pub fn collapse_monitor(monitor: &str, motion: bool) {
    gpu::with_existing(|g| collapse_monitor_in(g, monitor, motion));
}

fn collapse_monitor_in(g: &mut Gpu, monitor: &str, motion: bool) -> R<()> {
    let (mine, rest): (Vec<Set>, Vec<Set>) = std::mem::take(&mut g.expose.open).into_iter().partition(|s| s.monitor == monitor);
    g.expose.open = rest;
    for set in mine {
        fold(g, set, motion)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn open_in(g: &mut Gpu, monitor: &str, slot: usize, current: bool, roots: &[(Entry, RECT)], tile: Option<RECT>,
    mon: RECT, work: RECT, panel_hwnd: HWND, panel_rect: RECT, scale: f32, motion: bool) -> R<()> {
    if !g.expose.overlays.iter().any(|o| o.monitor == monitor) {
        let o = gpu::make_overlay(&g.dcomp, monitor, mon)?;
        g.expose.overlays.push(o);
    }
    let margin = MARGIN * scale;
    let area = Rect {
        x: (work.left - mon.left) as f32 + margin,
        y: (work.top - mon.top) as f32 + margin,
        w: (work.right - work.left) as f32 - 2.0 * margin,
        h: (panel_rect.top - work.top) as f32 - 2.0 * margin,
    };
    let sizes: Vec<(f32, f32)> = roots.iter().map(|(_, r)| (((r.right - r.left).max(1)) as f32, ((r.bottom - r.top).max(1)) as f32)).collect();
    let cells = grid(&sizes, area, GAP * scale);
    let tile = tile.map(|t| client(t, mon));
    unsafe {
        let root = g.dcomp.CreateVisual()?;
        let tx = Tx::new(g, if motion { OPEN_MS } else { 0 })?;
        let mut top = None; // back to front: backdrop, tint, highlight, previews

        // Frosted backdrop: the monitor's own picture, blurring in (GPU capture, 5-10 ms).
        let backdrop = match gpu::capture_monitor(g, monitor, mon)? {
            Some(pic) => {
                let outer = g.dcomp.CreateVisual()?;
                let fx = g.dcomp.CreateEffectGroup()?;
                fx.SetOpacity2(1.0)?;
                outer.SetEffect(&fx)?;
                let inner = g.dcomp.CreateVisual()?;
                inner.SetContent(&pic)?;
                let blur = g.dcomp.cast::<IDCompositionDevice3>()?.CreateGaussianBlurEffect()?;
                blur.SetBorderMode(D2D1_BORDER_MODE_HARD)?; // no dark fringe at the monitor edges
                if tx.dur == 0 { blur.SetStandardDeviation2(BLUR)? } else { blur.SetStandardDeviation(&tx.anim(0.0, BLUR)?)? }
                inner.SetEffect(&blur)?;
                outer.AddVisual(&inner, false, None::<&IDCompositionVisual>)?;
                stack(&root, &outer, &mut top)?;
                Some((blur, fx))
            }
            None => None,
        };
        let dim_to = if backdrop.is_some() { TINT } else { DIM };

        let dim_v = g.dcomp.CreateVisual()?;
        dim_v.SetContent(&solid(g, 0xFF00_0000)?)?;
        let dim_scale = g.dcomp.CreateScaleTransform()?;
        dim_scale.SetScaleX2((mon.right - mon.left) as f32)?;
        dim_scale.SetScaleY2((mon.bottom - mon.top) as f32)?;
        dim_v.SetTransform(&dim_scale)?;
        let dim = g.dcomp.CreateEffectGroup()?;
        tx.opacity(&dim, 0.0, dim_to)?;
        dim_v.SetEffect(&dim)?;
        stack(&root, &dim_v, &mut top)?;

        let hl_v = g.dcomp.CreateVisual()?;
        hl_v.SetContent(&solid(g, ACCENT)?)?;
        let hl_scale = g.dcomp.CreateScaleTransform()?;
        hl_v.SetTransform(&hl_scale)?;
        let hl_fx = g.dcomp.CreateEffectGroup()?;
        hl_fx.SetOpacity2(0.0)?;
        hl_v.SetEffect(&hl_fx)?;
        stack(&root, &hl_v, &mut top)?;

        let mut previews = vec![];
        for ((e, r), cell) in roots.iter().zip(cells) {
            let (surface, sw, sh) = content(g, e.id, cell.w / cell.h)?;
            let visual = g.dcomp.CreateVisual()?;
            visual.SetContent(&surface)?;
            let st = g.dcomp.CreateScaleTransform()?;
            visual.SetTransform(&st)?;
            let fx = g.dcomp.CreateEffectGroup()?;
            visual.SetEffect(&fx)?;
            stack(&root, &visual, &mut top)?;
            let (home, from_tile) = match (current, tile) {
                (false, Some(t)) => (t.fit(cell.w, cell.h), true),
                _ => (client(*r, mon), false),
            };
            let p = Preview { id: e.id, visual, scale: st, fx, sw, sh, home, cell, from_tile, cx: 0.0, cy: 0.0, gone: false, icon: None, badge: None, dimmed: false };
            tx.rect(&p, home, cell)?;
            tx.opacity(&p.fx, if from_tile { 0.0 } else { 1.0 }, 1.0)?;
            previews.push(p);
        }
        let icons = g.dcomp.CreateVisual()?;
        let icons_fx = g.dcomp.CreateEffectGroup()?;
        icons_fx.SetOpacity2(1.0)?;
        icons.SetEffect(&icons_fx)?;
        stack(&root, &icons, &mut top)?; // above every preview
        let overlay = g.expose.overlays.iter().find(|o| o.monitor == monitor).expect("made above");
        // In front of any spread still folding away on this monitor.
        let mut front = g.expose.closing.iter().rev().find(|s| s.monitor == monitor).map(|s| s.root.clone());
        stack(&overlay.root, &root, &mut front)?;
        g.dcomp.Commit()?;
        // Directly behind the panel: above every other window, never above the panel.
        let _ = SetWindowPos(overlay.hwnd, Some(panel_hwnd), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        g.expose.open.push(Set {
            monitor: monitor.to_string(), slot, current, mon, root, dim, dim_to, backdrop, highlight: (hl_v, hl_scale, hl_fx), previews,
            entries: roots.iter().map(|(e, _)| e.clone()).collect(), ends: 0, dropped: false,
            icons, icons_fx, scale, settled: if motion { tx.begin_ms + OPEN_MS } else { 0 },
        });
        g.expose.hovered = None;
        // Icons already known fade in once the spread has settled.
        let k = g.expose.open.len() - 1;
        let itx = Tx::after(g, if motion { ICON_FADE_MS } else { 0 }, if motion { OPEN_MS } else { 0 })?;
        let size = (BADGE * scale).round() as i32;
        let pics = badge_pics(g, size)?;
        for i in 0..g.expose.open[k].previews.len() {
            add_icon(&itx, &g.expose.icon_surfaces, &g.expose.icon_keys, &mut g.expose.open[k], i)?;
            add_badge(&itx, &pics, &mut g.expose.open[k], i, size)?;
        }
        g.dcomp.Commit()?;
    }
    Ok(())
}

/// Folds the open spread back (to its windows, or into its tile) and fades the dimming.
pub fn collapse(motion: bool) {
    gpu::with_existing(|g| collapse_in(g, motion));
}

fn collapse_in(g: &mut Gpu, motion: bool) -> R<()> {
    g.expose.hovered = None;
    for set in std::mem::take(&mut g.expose.open) {
        fold(g, set, motion)?;
    }
    Ok(())
}

/// Puts the previews in their windows' stacking order (the topmost window's preview on top),
/// so that folding or choosing moves them over one another as the real windows overlap.
/// Previews are created topmost first, which `stack` would otherwise leave upside down.
fn restack(set: &Set) -> R<()> {
    unsafe {
        let mut below = set.highlight.0.clone();
        for p in set.previews.iter().rev().filter(|p| !p.gone) {
            let _ = set.root.RemoveVisual(&p.visual);
            set.root.AddVisual(&p.visual, true, &below)?;
            below = p.visual.clone();
        }
    }
    Ok(())
}

/// Folds one spread back into its windows or tile, fading its dimming.
fn fold(g: &mut Gpu, mut set: Set, motion: bool) -> R<()> {
    if set.previews.iter().any(|p| Some(p.id) == g.expose.hot_close) {
        g.expose.hot_close = None;
    }
    restack(&set)?;
    let dur = if motion { CLOSE_MS } else { 0 };
    let tx = Tx::new(g, dur)?;
    unsafe {
        tx.opacity(&set.dim, set.dim_to, 0.0)?;
        match &set.backdrop {
            // the frozen picture still shows a window that was moved away: fade it
            Some((_, fx)) if set.dropped => tx.opacity(fx, 1.0, 0.0)?,
            Some((blur, _)) => if dur == 0 { blur.SetStandardDeviation2(0.0)? } else { blur.SetStandardDeviation(&tx.anim(BLUR, 0.0)?)? },
            None => {}
        }
        set.highlight.2.SetOpacity2(0.0)?;
        // icons go first, so they never trail their previews as these fold
        Tx::new(g, if motion { ICON_OUT_MS } else { 0 })?.opacity(&set.icons_fx, 1.0, 0.0)?;
        for p in set.previews.iter().filter(|p| !p.gone) {
            tx.rect(p, p.cell, p.home)?;
            if p.from_tile {
                tx.opacity(&p.fx, 1.0, 0.0)?;
            }
        }
        g.dcomp.Commit()?;
    }
    set.ends = tx.begin_ms + dur;
    g.expose.closing.push(set);
    Ok(())
}

/// What choosing preview `id` means: (monitor, slot, slot is current, window).
pub fn chosen(id: u64) -> Option<(String, usize, bool, Entry)> {
    gpu::with_existing(|g| {
        // previews and entries are in the same order
        Ok(g.expose.open.iter().find_map(|s| {
            let i = s.previews.iter().position(|p| p.id == id && !p.gone)?;
            Some((s.monitor.clone(), s.slot, s.current, s.entries.get(i)?.clone()))
        }))
    })
    .flatten()
}

/// The chosen preview flies onto its window (`target`, screen) or fades. Every other spread
/// on every monitor fades: a choice ends the exposé.
pub fn choose(id: u64, target: Option<RECT>, motion: bool) {
    gpu::with_existing(|g| {
        let sets = std::mem::take(&mut g.expose.open);
        g.expose.hovered = None;
        g.expose.hot_close = None;
        let dur = if motion { CHOOSE_MS } else { 0 };
        let tx = Tx::new(g, dur)?;
        let icons_out = Tx::new(g, if motion { ICON_OUT_MS } else { 0 })?;
        for mut set in sets {
            restack(&set)?;
            unsafe {
                tx.opacity(&set.dim, set.dim_to, 0.0)?;
                // The slot underneath may change: fade the old picture rather than unblur it.
                if let Some((_, fx)) = &set.backdrop {
                    tx.opacity(fx, 1.0, 0.0)?;
                }
                set.highlight.2.SetOpacity2(0.0)?;
                icons_out.opacity(&set.icons_fx, 1.0, 0.0)?;
                for p in set.previews.iter_mut().filter(|p| !p.gone) {
                    match target.filter(|_| p.id == id) {
                        Some(t) => {
                            // Land on the window at full resolution: no blurry moment at the end.
                            if let Some(Some((full, fw, fh))) = g.expose.surfaces.get(&p.id).map(|(_, f)| f.clone()) {
                                p.visual.SetContent(&full)?;
                                (p.sw, p.sh) = (fw, fh);
                            }
                            tx.rect(p, p.cell, client(t, set.mon))?
                        }
                        None => tx.opacity(&p.fx, 1.0, 0.0)?,
                    }
                }
            }
            set.ends = tx.begin_ms + dur;
            g.expose.closing.push(set);
        }
        unsafe { g.dcomp.Commit()? };
        Ok(())
    });
}

/// The first open spread's monitor and slot (the only one in the Ctrl+Alt exposé).
pub fn open_slot() -> Option<(String, usize)> {
    gpu::with_existing(|g| Ok(g.expose.open.first().map(|s| (s.monitor.clone(), s.slot)))).flatten()
}

/// The slot spread open on `monitor`, if any.
pub fn open_slot_on(monitor: &str) -> Option<usize> {
    gpu::with_existing(|g| Ok(g.expose.open.iter().find(|s| s.monitor == monitor).map(|s| s.slot))).flatten()
}

/// The window id of the preview under a screen point, in whichever spread is on that point.
pub fn preview_at(pt: POINT) -> Option<u64> {
    gpu::with_existing(|g| {
        let dragged = g.expose.drag.as_ref().map(|d| d.id);
        Ok(g.expose.open.iter().find(|s| inside(s.mon, pt)).and_then(|s| {
            let (x, y) = ((pt.x - s.mon.left) as f32, (pt.y - s.mon.top) as f32);
            s.previews.iter().rev().find(|p| !p.gone && !p.dimmed && Some(p.id) != dragged && p.cell.contains(x, y)).map(|p| p.id)
        }))
    })
    .flatten()
}

fn inside(r: RECT, pt: POINT) -> bool {
    pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom
}

/// On an open spread's monitor but not on a preview or that monitor's panel.
pub fn dim_at(pt: POINT) -> bool {
    let Some(monitor) = gpu::with_existing(|g| Ok(g.expose.open.iter().find(|s| inside(s.mon, pt)).map(|s| s.monitor.clone()))).flatten() else {
        return false;
    };
    let on_panel = crate::panel::window_of(&monitor).is_some_and(|(_, r)| inside(r, pt));
    !on_panel && preview_at(pt).is_none()
}

/// The preview whose close x is under a screen point.
pub fn close_at(pt: POINT) -> Option<u64> {
    gpu::with_existing(|g| {
        let dragged = g.expose.drag.as_ref().map(|d| d.id);
        Ok(g.expose.open.iter().find(|s| inside(s.mon, pt)).and_then(|s| {
            let (x, y) = ((pt.x - s.mon.left) as f32, (pt.y - s.mon.top) as f32);
            s.previews
                .iter()
                .rev()
                .find(|p| !p.gone && !p.dimmed && Some(p.id) != dragged && p.badge.as_ref().is_some_and(|b| b.rect.contains(x, y)))
                .map(|p| p.id)
        }))
    })
    .flatten()
}

/// Makes the close x of preview `id` red (None: none is).
pub fn set_close_hot(id: Option<u64>) {
    gpu::with_existing(|g| {
        if g.expose.hot_close == id {
            return Ok(());
        }
        unhot(g)?;
        if let Some(id) = id {
            for set in &g.expose.open {
                if let Some(b) = set.previews.iter().find(|p| p.id == id).and_then(|p| p.badge.as_ref())
                    && let Some(pair) = g.expose.badges.get(&b.size)
                {
                    unsafe { b.visual.SetContent(&pair.1.0)? };
                    g.expose.hot_close = Some(id);
                }
            }
            unsafe { g.dcomp.Commit()? };
        }
        Ok(())
    });
}

/// Dims preview `id` while its window is asked to close (and makes it inert), or restores it.
pub fn dim_preview(id: u64, on: bool) {
    gpu::with_existing(|g| {
        if on && g.expose.hovered == Some(id) {
            g.expose.hovered = None;
            for set in &g.expose.open {
                unsafe { set.highlight.2.SetOpacity2(0.0)? };
            }
        }
        for set in g.expose.open.iter_mut() {
            if let Some(p) = set.previews.iter_mut().find(|p| p.id == id && !p.gone) {
                p.dimmed = on;
                unsafe { p.fx.SetOpacity2(if on { DIMMED } else { 1.0 })? };
            }
        }
        unsafe { g.dcomp.Commit() }
    });
}

/// The window of preview `id` is gone: the preview, its icon and its x fade away.
pub fn fade_out(id: u64, motion: bool) {
    gpu::with_existing(|g| {
        let tx = Tx::new(g, if motion { CLOSE_MS } else { 0 })?;
        if g.expose.hovered == Some(id) {
            g.expose.hovered = None;
            for set in &g.expose.open {
                unsafe { set.highlight.2.SetOpacity2(0.0)? };
            }
        }
        for set in g.expose.open.iter_mut() {
            if let Some(p) = set.previews.iter_mut().find(|p| p.id == id && !p.gone) {
                p.gone = true;
                tx.opacity(&p.fx, if p.dimmed { DIMMED } else { 1.0 }, 0.0)?;
                unsafe {
                    if let Some(fx) = &p.icon {
                        fx.SetOpacity2(0.0)?;
                    }
                    if let Some(b) = &p.badge {
                        b.fx.SetOpacity2(0.0)?;
                    }
                }
            }
        }
        unsafe { g.dcomp.Commit() }
    });
}

pub fn hovered() -> Option<u64> {
    gpu::with_existing(|g| Ok(g.expose.hovered.filter(|_| !g.expose.open.is_empty()))).flatten()
}

/// Highlights the preview with window id `id` in whichever spread holds it (None: none).
pub fn set_hover(id: Option<u64>) {
    gpu::with_existing(|g| {
        if g.expose.hovered == id {
            return Ok(());
        }
        unsafe {
            for set in &g.expose.open {
                let (v, st, fx) = &set.highlight;
                match id.and_then(|id| set.previews.iter().find(|p| p.id == id && !p.gone)) {
                    Some(p) => {
                        let c = p.cell;
                        v.SetOffsetX2(c.x - HIGHLIGHT)?;
                        v.SetOffsetY2(c.y - HIGHLIGHT)?;
                        st.SetScaleX2(c.w + 2.0 * HIGHLIGHT)?;
                        st.SetScaleY2(c.h + 2.0 * HIGHLIGHT)?;
                        fx.SetOpacity2(1.0)?;
                    }
                    None => fx.SetOpacity2(0.0)?,
                }
            }
            g.dcomp.Commit()?;
        }
        g.expose.hovered = id;
        Ok(())
    });
}

/// New pictures arrived: upload them (the CPU copies are dropped), and swap them into an
/// open spread (replacing cards).
pub fn snapshots_arrived(shots: Vec<crate::snap::Shot>) {
    let Some(any_monitor) = win::monitor_names().into_iter().next() else { return };
    if shots.is_empty() {
        return;
    }
    gpu::with(&any_monitor, |g| {
        let mut changed = false;
        let upload = |g: &Gpu, p: &crate::snap::Pic| -> R<Pic> {
            Ok((gpu::surface_from_pixels(g, p.w, p.h, &p.pixels, false)?, p.w as f32, p.h as f32))
        };
        for shot in shots {
            let id = shot.id;
            let s = upload(g, &shot.grid)?;
            let full = shot.full.as_ref().map(|p| upload(g, p)).transpose()?;
            g.expose.cards.remove(&id);
            g.expose.surfaces.insert(id, (s.clone(), full));
            for set in g.expose.open.iter_mut() {
                if let Some(p) = set.previews.iter_mut().find(|p| p.id == id) {
                    unsafe { p.visual.SetContent(&s.0)? };
                    (p.sw, p.sh) = (s.1, s.2);
                    let tx = Tx { dcomp: g.dcomp.clone(), begin: 0, begin_ms: 0, dur: 0, delay: 0 };
                    tx.rect(p, p.cell, p.cell)?;
                    changed = true;
                }
            }
        }
        g.expose.surfaces.retain(|&id, _| unsafe { IsWindow(Some(win::hwnd(id))).as_bool() });
        g.expose.cards.retain(|&id, _| unsafe { IsWindow(Some(win::hwnd(id))).as_bool() });
        if changed {
            unsafe { g.dcomp.Commit()? };
        }
        Ok(())
    });
}

/// New app icons arrived: upload each app's picture once (shrunk to what is ever drawn), and
/// give previews in open spreads their icons (fading in, once the spread has settled).
pub fn icons_arrived(icons: Vec<crate::icons::Icon>) {
    if icons.is_empty() {
        return;
    }
    let Some(any_monitor) = win::monitor_names().into_iter().next() else { return };
    let side = (ICON_RANGE.1 * win::monitors().iter().map(|m| m.scale).fold(1.0, f32::max)).ceil() as i32;
    gpu::with(&any_monitor, |g| {
        for ic in icons {
            if !g.expose.icon_surfaces.contains_key(&ic.key) {
                if g.expose.icon_surfaces.len() >= ICON_SURFACES {
                    g.expose.icon_surfaces.clear(); // open spreads keep their own references
                }
                let p = crate::icons::fit(&ic.pic, side);
                let surface = gpu::surface_from_pixels(g, p.w, p.h, &p.pixels, true)?;
                g.expose.icon_surfaces.insert(ic.key.clone(), (surface, p.w as f32, p.h as f32));
            }
            g.expose.icon_keys.insert(ic.id, ic.key);
        }
        g.expose.icon_keys.retain(|&id, _| unsafe { IsWindow(Some(win::hwnd(id))).as_bool() });
        let now = win::now_ms();
        for k in 0..g.expose.open.len() {
            let settled = g.expose.open[k].settled;
            let itx = Tx::after(g, if settled > 0 { ICON_FADE_MS } else { 0 }, settled.saturating_sub(now))?;
            for i in 0..g.expose.open[k].previews.len() {
                add_icon(&itx, &g.expose.icon_surfaces, &g.expose.icon_keys, &mut g.expose.open[k], i)?;
            }
        }
        unsafe { g.dcomp.Commit() }
    });
}

/// Removes closed spreads whose motion is over. True while any spread exists.
pub fn reap(now: u64) -> bool {
    gpu::with_existing(|g| {
        let (gone, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut g.expose.closing).into_iter().partition(|s| now >= s.ends + 50);
        g.expose.closing = keep;
        if !gone.is_empty() {
            remove(g, &gone)?;
        }
        let (landed, flying): (Vec<_>, Vec<_>) = std::mem::take(&mut g.expose.flying).into_iter().partition(|(_, ends)| now >= ends + 50);
        g.expose.flying = flying;
        if !landed.is_empty() {
            drop_flying(g, &landed)?;
        }
        Ok(!g.expose.open.is_empty() || !g.expose.closing.is_empty() || !g.expose.flying.is_empty() || g.expose.drag.is_some())
    })
    .unwrap_or(false)
}

pub fn any() -> bool {
    gpu::with_existing(|g| Ok(!g.expose.open.is_empty() || !g.expose.closing.is_empty() || !g.expose.flying.is_empty())).unwrap_or(false)
}

/// Removes every spread at once (monitor change, Restore all, Exit, a switch).
pub fn clear_all() {
    gpu::with_existing(|g| {
        let mut gone = std::mem::take(&mut g.expose.closing);
        gone.extend(std::mem::take(&mut g.expose.open));
        g.expose.hovered = None;
        g.expose.drag = None;
        let flying = std::mem::take(&mut g.expose.flying);
        drop_flying(g, &flying)?;
        if let Some((o, _)) = &g.expose.drag_layer {
            unsafe {
                // a dragged preview lives on the drag layer: take everything off it
                let _ = o.root.RemoveAllVisuals();
                let _ = ShowWindow(o.hwnd, SW_HIDE);
            }
        }
        remove(g, &gone)
    });
}

fn remove(g: &Gpu, gone: &[Set]) -> R<()> {
    unsafe {
        for s in gone {
            if let Some(o) = g.expose.overlays.iter().find(|o| o.monitor == s.monitor) {
                let _ = o.root.RemoveVisual(&s.root);
            }
        }
        g.dcomp.Commit()?;
        for o in &g.expose.overlays {
            let live = g.expose.open.iter().chain(&g.expose.closing).any(|s| s.monitor == o.monitor);
            if !live {
                let _ = ShowWindow(o.hwnd, SW_HIDE);
            }
        }
    }
    Ok(())
}

fn drop_flying(g: &Gpu, landed: &[(IDCompositionVisual2, u64)]) -> R<()> {
    let Some((o, _)) = &g.expose.drag_layer else { return Ok(()) };
    unsafe {
        for (v, _) in landed {
            let _ = o.root.RemoveVisual(v);
        }
        g.dcomp.Commit()?;
        if g.expose.drag.is_none() && g.expose.flying.is_empty() {
            let _ = ShowWindow(o.hwnd, SW_HIDE);
        }
    }
    Ok(())
}

/// Previews in the open spreads that haven't been dropped elsewhere.
pub fn live() -> usize {
    gpu::with_existing(|g| Ok(g.expose.open.iter().map(|s| s.previews.iter().filter(|p| !p.gone).count()).sum())).unwrap_or(0)
}

/// The dragged preview: (monitor, slot, slot is current, window).
pub fn drag_info() -> Option<(String, usize, bool, Entry)> {
    let id = gpu::with_existing(|g| Ok(g.expose.drag.as_ref().map(|d| d.id))).flatten()?;
    chosen(id)
}

fn union_rect() -> RECT {
    win::monitors().iter().fold(RECT { left: i32::MAX, top: i32::MAX, right: i32::MIN, bottom: i32::MIN }, |u, m| RECT {
        left: u.left.min(m.rect.left),
        top: u.top.min(m.rect.top),
        right: u.right.max(m.rect.right),
        bottom: u.bottom.max(m.rect.bottom),
    })
}

/// The open spread holding preview `id`, by position in `open`.
fn holder(open: &[Set], id: u64) -> Option<usize> {
    open.iter().position(|s| s.previews.iter().any(|p| p.id == id && !p.gone))
}

/// Lifts preview `id` under the cursor: it moves onto the drag layer (above the panels, over
/// every monitor), shrinks around the grab point, and follows `drag_move`.
pub fn drag_start(id: u64, pt: POINT, motion: bool) -> bool {
    let Some(monitor) = gpu::with_existing(|g| Ok(holder(&g.expose.open, id).map(|k| g.expose.open[k].monitor.clone()))).flatten() else {
        return false;
    };
    let scale = win::monitors().into_iter().find(|m| m.name == monitor).map_or(1.0, |m| m.scale);
    let union = union_rect();
    gpu::with_existing(|g| {
        let Some(k) = holder(&g.expose.open, id) else { return Ok(false) };
        if g.expose.drag_layer.is_none() {
            let o = gpu::make_overlay(&g.dcomp, "drag", union)?;
            g.expose.drag_layer = Some((o, POINT { x: union.left, y: union.top }));
        }
        let tx = Tx::new(g, if motion { LIFT_MS } else { 0 })?;
        unhot(g)?;
        let set = &mut g.expose.open[k];
        let i = set.previews.iter().position(|p| p.id == id).expect("held above");
        let p = &mut set.previews[i];
        let c = p.cell;
        let (gx, gy) = ((pt.x - set.mon.left) as f32 - c.x, (pt.y - set.mon.top) as f32 - c.y);
        let (cx, cy) = (gx / c.w * p.sw, gy / c.h * p.sh);
        let s0 = (c.w / p.sw, c.h / p.sh);
        let k = (LIFT_MAX.0 * scale / c.w).min(LIFT_MAX.1 * scale / c.h).min(1.0);
        let s1 = (s0.0 * k, s0.1 * k);
        let (layer, origin) = g.expose.drag_layer.as_ref().expect("made above");
        unsafe {
            set.root.RemoveVisual(&p.visual)?;
            layer.root.AddVisual(&p.visual, false, None::<&IDCompositionVisual>)?;
            // Center the scale on the grab point: then the grab point sits at offset + c
            // whatever the scale, so the cursor can move it while the scale animates.
            (p.cx, p.cy) = (cx, cy);
            p.scale.SetCenterX2(cx)?;
            p.scale.SetCenterY2(cy)?;
            p.visual.SetOffsetX2((pt.x - origin.x) as f32 - cx)?;
            p.visual.SetOffsetY2((pt.y - origin.y) as f32 - cy)?;
            if tx.dur == 0 {
                p.scale.SetScaleX2(s1.0)?;
                p.scale.SetScaleY2(s1.1)?;
            } else {
                p.scale.SetScaleX(&tx.anim(s0.0, s1.0)?)?;
                p.scale.SetScaleY(&tx.anim(s0.1, s1.1)?)?;
            }
            set.highlight.2.SetOpacity2(0.0)?;
            if let Some(fx) = &p.icon {
                fx.SetOpacity2(0.0)?; // the icon stays behind while its preview is carried
            }
            if let Some(b) = &p.badge {
                b.fx.SetOpacity2(0.0)?;
            }
            g.dcomp.Commit()?;
            let _ = SetWindowPos(layer.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
        g.expose.hovered = None;
        g.expose.drag = Some(Drag { id, c: (cx, cy), s0, s1, last: pt, started: tx.begin_ms });
        Ok(true)
    })
    .unwrap_or(false)
}

/// The lifted preview follows the cursor.
pub fn drag_move(pt: POINT) {
    gpu::with_existing(|g| {
        let Some(id) = g.expose.drag.as_ref().map(|d| d.id) else { return Ok(()) };
        let (Some(k), Some((_, origin))) = (holder(&g.expose.open, id), g.expose.drag_layer.as_ref()) else { return Ok(()) };
        let d = g.expose.drag.as_mut().expect("checked");
        d.last = pt;
        let p = g.expose.open[k].previews.iter().find(|p| p.id == id).expect("held");
        unsafe {
            p.visual.SetOffsetX2((pt.x - origin.x) as f32 - d.c.0)?;
            p.visual.SetOffsetY2((pt.y - origin.y) as f32 - d.c.1)?;
            g.dcomp.Commit()
        }
    });
}

/// Where a dropped preview goes.
pub enum Land {
    /// Fly onto this screen rect (a tile, or the window's new place) and fade out.
    Into(RECT),
    /// Back to its place in the spread.
    Back,
}

/// Ends a preview drag.
pub fn drag_end(land: Land, motion: bool) {
    gpu::with_existing(|g| {
        let dur = if !motion { 0 } else if matches!(land, Land::Into(_)) { LAND_MS } else { CLOSE_MS };
        let tx = Tx::new(g, dur)?;
        let Some(d) = g.expose.drag.take() else { return Ok(()) };
        let Some(k) = holder(&g.expose.open, d.id) else { return Ok(()) };
        let Some((layer, origin)) = g.expose.drag_layer.as_ref() else { return Ok(()) };
        let set = &mut g.expose.open[k];
        let p = set.previews.iter_mut().find(|p| p.id == d.id).expect("held");
        // The lift may still be running: take its current scale from the same curve.
        let now = win::now_ms();
        let t = if motion { now.saturating_sub(d.started) as f32 / LIFT_MS as f32 } else { 1.0 };
        let e = crate::anim::ease_out(t);
        let s = (d.s0.0 + (d.s1.0 - d.s0.0) * e, d.s0.1 + (d.s1.1 - d.s0.1) * e);
        unsafe {
            match land {
                Land::Into(r) => {
                    let at = ((d.last.x - origin.x) as f32 - d.c.0, (d.last.y - origin.y) as f32 - d.c.1, s.0, s.1);
                    let to = p.place(Rect { x: (r.left - origin.x) as f32, y: (r.top - origin.y) as f32, w: (r.right - r.left) as f32, h: (r.bottom - r.top) as f32 });
                    tx.place(p, at, to)?;
                    tx.opacity(&p.fx, 1.0, 0.0)?;
                    p.gone = true;
                    set.dropped = true;
                    g.expose.flying.push((p.visual.clone(), tx.begin_ms + tx.dur));
                }
                Land::Back => {
                    layer.root.RemoveVisual(&p.visual)?;
                    set.root.AddVisual(&p.visual, false, None::<&IDCompositionVisual>)?; // in front
                    // ...which would cover the icons: they go back on top, and this one returns
                    set.root.RemoveVisual(&set.icons)?;
                    set.root.AddVisual(&set.icons, false, None::<&IDCompositionVisual>)?;
                    if let Some(fx) = &p.icon {
                        tx.opacity(fx, 0.0, 1.0)?;
                    }
                    if let Some(b) = &p.badge {
                        tx.opacity(&b.fx, 0.0, 1.0)?;
                    }
                    let at = ((d.last.x - set.mon.left) as f32 - d.c.0, (d.last.y - set.mon.top) as f32 - d.c.1, s.0, s.1);
                    tx.place(p, at, p.place(p.cell))?;
                    if g.expose.flying.is_empty() {
                        let _ = ShowWindow(layer.hwnd, SW_HIDE);
                    }
                }
            }
            g.dcomp.Commit()
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_straddles_the_corner_and_fits_the_margins() {
        let cell = Rect { x: 100.0, y: 80.0, w: 400.0, h: 300.0 };
        let b = badge_place(cell, BADGE);
        assert_eq!((b.x + b.w / 2.0, b.y + b.h / 2.0), (500.0, 80.0), "centered on the top-right corner");
        const { assert!(BADGE / 2.0 <= MARGIN && BADGE / 2.0 <= GAP, "half a badge fits the margin and the gap") };
    }

    #[test]
    fn badge_is_a_disc_with_a_cross() {
        let size = 24;
        let (n, h) = (close_badge(size, false), close_badge(size, true));
        let at = |v: &[u32], x: i32, y: i32| v[(y * size + x) as usize];
        assert_eq!(at(&n, 0, 0) >> 24, 0, "the corner is outside the disc");
        let centre = at(&n, 12, 12);
        assert!(centre >> 24 > 240 && (centre & 0xFF) > 240, "the cross is white at the centre");
        let rim = at(&n, 12, 2); // inside the disc, off both strokes
        assert!((rim >> 24) > 150 && (rim >> 24) < 255 && (rim & 0xFF) < 100, "dark translucent disc: {rim:08x}");
        assert!((at(&h, 12, 2) >> 16 & 0xFF) > (rim >> 16 & 0xFF) + 50, "hot is red");
        assert!(n.iter().chain(&h).all(|&p| {
            let a = p >> 24;
            (p >> 16 & 0xFF) <= a && (p >> 8 & 0xFF) <= a && (p & 0xFF) <= a
        }), "premultiplied");
    }

    #[test]
    fn icon_sits_in_the_bottom_left_and_is_sized_by_the_preview() {
        let cell = Rect { x: 100.0, y: 50.0, w: 600.0, h: 400.0 };
        let r = icon_place(cell, 1.0, 256.0).unwrap();
        assert_eq!((r.w, r.h), (72.0, 72.0), "22% of 400 is 88: clamped to the top of the range");
        assert_eq!((r.x, r.y + r.h), (108.0, 442.0), "inset 8 from the left and bottom edges");
        let mid = icon_place(Rect { x: 0.0, y: 0.0, w: 300.0, h: 200.0 }, 1.0, 256.0).unwrap();
        assert_eq!(mid.w, 44.0);
        let small = icon_place(Rect { x: 0.0, y: 0.0, w: 300.0, h: 130.0 }, 1.0, 256.0).unwrap();
        assert_eq!(small.w, 32.0, "bottom of the range");
    }

    #[test]
    fn icon_scales_with_the_monitor_and_never_exceeds_its_pixels() {
        let cell = Rect { x: 0.0, y: 0.0, w: 600.0, h: 400.0 };
        assert_eq!(icon_place(cell, 2.0, 256.0).unwrap().w, 88.0, "22% of 400, within 64..144");
        assert_eq!(icon_place(cell, 2.0, 64.0).unwrap().w, 64.0, "a 64 px picture is never drawn larger");
    }

    #[test]
    fn tiny_previews_carry_no_icon() {
        assert!(icon_place(Rect { x: 0.0, y: 0.0, w: 500.0, h: 119.0 }, 1.0, 256.0).is_none());
        assert!(icon_place(Rect { x: 0.0, y: 0.0, w: 500.0, h: 239.0 }, 2.0, 256.0).is_none());
        assert!(icon_place(Rect { x: 0.0, y: 0.0, w: 500.0, h: 120.0 }, 1.0, 256.0).is_some());
    }

    const AREA: Rect = Rect { x: 48.0, y: 48.0, w: 2464.0, h: 1100.0 };

    fn check(sizes: &[(f32, f32)]) -> Vec<Rect> {
        let cells = grid(sizes, AREA, 32.0);
        assert_eq!(cells.len(), sizes.len());
        for (i, (c, &(w, h))) in cells.iter().zip(sizes).enumerate() {
            assert!(c.x >= AREA.x - 0.01 && c.y >= AREA.y - 0.01, "{i} inside (top-left)");
            assert!(c.x + c.w <= AREA.x + AREA.w + 0.01 && c.y + c.h <= AREA.y + AREA.h + 0.01, "{i} inside (bottom-right)");
            assert!(c.w <= w + 0.01 && c.h <= h + 0.01, "{i} never enlarged");
            assert!((c.w / c.h - w / h).abs() < 0.01, "{i} keeps aspect");
            for (j, d) in cells.iter().enumerate().skip(i + 1) {
                let apart = c.x + c.w <= d.x || d.x + d.w <= c.x || c.y + c.h <= d.y || d.y + d.h <= c.y;
                assert!(apart, "{i} and {j} overlap");
            }
        }
        cells
    }

    #[test]
    fn one_window_is_centered_and_not_enlarged() {
        let c = check(&[(800.0, 600.0)]);
        assert_eq!((c[0].w, c[0].h), (800.0, 600.0));
        assert!((c[0].x + c[0].w / 2.0 - (AREA.x + AREA.w / 2.0)).abs() < 0.01);
    }

    #[test]
    fn big_windows_shrink_to_fit() {
        let c = check(&[(2560.0, 1400.0), (2560.0, 1400.0)]);
        assert!(c[0].w < 2560.0);
        assert!((c[0].y - c[1].y).abs() < 0.01, "two wide windows side by side");
    }

    #[test]
    fn mixes_and_many() {
        check(&[(2560.0, 1400.0), (600.0, 1200.0), (1200.0, 800.0), (400.0, 300.0), (1920.0, 1080.0)]);
        let many: Vec<(f32, f32)> = (0..24).map(|i| (800.0 + i as f32 * 40.0, 600.0)).collect();
        check(&many);
    }

    #[test]
    fn short_last_row_is_centered() {
        let c = check(&[(1200.0, 900.0); 5]);
        let last = c.last().unwrap();
        let first_row_y = c[0].y;
        if (last.y - first_row_y).abs() > 1.0 {
            let row: Vec<&Rect> = c.iter().filter(|r| (r.y - last.y).abs() < 1.0).collect();
            let mid = (row.first().unwrap().x + row.last().unwrap().x + row.last().unwrap().w) / 2.0;
            assert!((mid - (AREA.x + AREA.w / 2.0)).abs() < 1.0);
        }
    }

    #[test]
    fn empty() {
        assert!(grid(&[], AREA, 32.0).is_empty());
    }
}
