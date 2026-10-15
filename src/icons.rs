//! High-resolution application icons for the exposé previews. `WM_GETICON` only returns the
//! 16 to 32 px icon an app chose to set; the real artwork (an Electron app's 256 px icon, a
//! packaged app's logo) is in its package or executable, which the shell can render. A worker
//! thread resolves each window to its app (explicit AppUserModelID, else the process image;
//! hung windows and shell hosts use the window's own icon), asks the shell for the picture and
//! caches it per app, so the shell is asked once per app, not per window or panel opening.
//! Pictures are premultiplied BGRA; the exposé uploads them to the GPU.

use crate::win;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PWSTR};

/// What the shell is asked for: the largest an icon is ever drawn needs less than this.
const ASK: i32 = 256;
/// Pictures cached per session (apps, not windows).
const CACHE: usize = 64;
/// Size of the picture drawn from a window's own icon.
const OWN: i32 = 64;

/// A picture: premultiplied BGRA.
pub struct Pic {
    pub w: i32,
    pub h: i32,
    pub pixels: Vec<u32>,
}

/// A window's icon; windows of one app share `key` and `pic`.
pub struct Icon {
    pub id: u64,
    pub key: String,
    pub pic: Arc<Pic>,
}

/// Window ids and their icon handles (resolved on the UI thread), waiting for the worker.
/// A newer request replaces one still waiting: the panel opening again supersedes it.
type Queue = Arc<(Mutex<Option<Vec<(u64, isize)>>>, Condvar)>;
type Worker = (Queue, Receiver<Icon>);

thread_local! {
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
}

/// Asks for the icons of `items` (window id, `HICON` handle as a fallback source). Results
/// arrive through `drain`, each followed by `crate::snap::WM_SNAPSHOT` on `app`.
pub fn request(app: HWND, items: Vec<(u64, isize)>) {
    if items.is_empty() {
        return;
    }
    WORKER.with(|w| {
        let mut w = w.borrow_mut();
        let (queue, _) = w.get_or_insert_with(|| spawn(app));
        let (pending, wake) = &**queue;
        *pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(items);
        wake.notify_one();
    });
}

pub fn drain() -> Vec<Icon> {
    WORKER.with(|w| w.borrow().as_ref().map(|(_, rx)| rx.try_iter().collect()).unwrap_or_default())
}

#[derive(Clone)]
enum Source {
    Aumid(String),
    Path(String),
    Own(isize),
}

fn spawn(app: HWND) -> Worker {
    let queue: Queue = Arc::default();
    let (results, from_worker) = channel::<Icon>();
    let (q, app) = (queue.clone(), app.0 as isize);
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        // key -> picture (None: the shell had nothing for it, don't ask again)
        let mut cache: HashMap<String, Option<Arc<Pic>>> = HashMap::new();
        loop {
            let items = {
                let (pending, wake) = &*q;
                let mut p = pending.lock().unwrap_or_else(|e| e.into_inner());
                while p.is_none() {
                    p = wake.wait(p).unwrap_or_else(|e| e.into_inner());
                }
                p.take().unwrap_or_default()
            };
            let t = std::time::Instant::now();
            let mut n = 0;
            for (id, own) in items {
                if cache.len() > CACHE {
                    cache.clear();
                }
                let found = sources(id, own).into_iter().find_map(|(key, source)| {
                    let pic = cache.entry(key.clone()).or_insert_with(|| fetch(&source).map(Arc::new));
                    pic.clone().map(|pic| (key, pic))
                });
                if let Some((key, pic)) = found {
                    n += 1;
                    if results.send(Icon { id, key, pic }).is_err() {
                        unsafe { CoUninitialize() };
                        return;
                    }
                    unsafe {
                        let _ = PostMessageW(Some(HWND(app as *mut _)), crate::snap::WM_SNAPSHOT, WPARAM(0), LPARAM(0));
                    }
                }
            }
            crate::log!("icons: {n} in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
    });
    (queue, from_worker)
}

/// Where a window's icon may come from, best first, each with its cache key.
fn sources(id: u64, own: isize) -> Vec<(String, Source)> {
    let h = win::hwnd(id);
    let mut v = Vec::new();
    // A hung app's window is not asked anything.
    if !unsafe { IsHungAppWindow(h).as_bool() } {
        if let Some(a) = aumid_of(h) {
            v.push((format!("a:{a}"), Source::Aumid(a)));
        }
        if let Some(p) = image_path(h).filter(|p| !p.to_ascii_lowercase().ends_with("\\applicationframehost.exe")) {
            v.push((format!("p:{}", p.to_lowercase()), Source::Path(p)));
        }
    }
    if own != 0 {
        v.push((format!("h:{own:x}"), Source::Own(own)));
    }
    v
}

/// The explicit AppUserModelID of the window (packaged apps, PWAs, apps that set one).
fn aumid_of(h: HWND) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(h).ok()?;
        let value = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&value).ok()?;
        let out = (!s.is_null()).then(|| s.to_string().ok()).flatten().filter(|s| !s.is_empty());
        CoTaskMemFree(Some(s.0 as *const _));
        out
    }
}

/// The path of the executable the window's process runs.
fn image_path(h: HWND) -> Option<String> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn fetch(source: &Source) -> Option<Pic> {
    let parsing = match source {
        Source::Aumid(a) => format!("shell:AppsFolder\\{a}"),
        Source::Path(p) => p.clone(),
        Source::Own(h) => return own_icon(HICON(*h as *mut _)),
    };
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(parsing), None).ok()?;
        let bmp = factory.GetImage(SIZE { cx: ASK, cy: ASK }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let pic = bitmap_pixels(bmp);
        let _ = DeleteObject(bmp.into());
        let mut pic = pic?;
        normalize(&mut pic.pixels);
        Some(pic)
    }
}

/// A DIB section of `w`×`h` (top-down 32 bpp) with its pixels.
unsafe fn dib(dc: HDC, w: i32, h: i32) -> Option<(HBITMAP, *mut u32)> {
    let bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bmp = unsafe { CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    (!bits.is_null()).then_some((bmp, bits as *mut u32))
}

/// The pixels of a bitmap the shell returned (whatever its depth), top-down 32 bpp.
unsafe fn bitmap_pixels(bmp: HBITMAP) -> Option<Pic> {
    unsafe {
        let mut info = BITMAP::default();
        if GetObjectW(bmp.into(), size_of::<BITMAP>() as i32, Some(&mut info as *mut _ as *mut _)) == 0 {
            return None;
        }
        let (w, h) = (info.bmWidth, info.bmHeight);
        if w <= 0 || h <= 0 || w > 4096 || h > 4096 {
            return None;
        }
        let screen = GetDC(None);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() },
            ..Default::default()
        };
        let mut pixels = vec![0u32; (w * h) as usize];
        let rows = GetDIBits(screen, bmp, 0, h as u32, Some(pixels.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, screen);
        (rows == h).then_some(Pic { w, h, pixels })
    }
}

/// A window's own icon: drawn on black and on white, which gives the premultiplied color
/// (the black one) and the alpha (how much the white one differs) for any kind of icon,
/// including old ones with only a mask.
fn own_icon(icon: HICON) -> Option<Pic> {
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let render = |bg: u32| -> Option<Vec<u32>> {
            let (bmp, bits) = dib(screen, OWN, OWN)?;
            let old = SelectObject(dc, bmp.into());
            let px = std::slice::from_raw_parts_mut(bits, (OWN * OWN) as usize);
            px.fill(bg);
            let drawn = DrawIconEx(dc, 0, 0, icon, OWN, OWN, 0, None, DI_NORMAL).is_ok();
            let out = drawn.then(|| px.to_vec());
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
            out
        };
        let (black, white) = (render(0xFF00_0000), render(0xFFFF_FFFF));
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        Some(Pic { w: OWN, h: OWN, pixels: combine(&black?, &white?) })
    }
}

/// Premultiplied pixels from one drawing on black and one on white.
fn combine(black: &[u32], white: &[u32]) -> Vec<u32> {
    black
        .iter()
        .zip(white)
        .map(|(&b, &w)| {
            let ch = |p: u32, s: u32| (p >> s) & 0xFF;
            let diff: u32 = [0, 8, 16].iter().map(|&s| ch(w, s).saturating_sub(ch(b, s))).sum::<u32>() / 3;
            let a = 255 - diff.min(255);
            // premultiplied color can't exceed alpha
            let c = |s: u32| ch(b, s).min(a) << s;
            (a << 24) | c(0) | c(8) | c(16)
        })
        .collect()
}

/// Makes shell pictures premultiplied: shell bitmaps normally already are; any pixel whose
/// color exceeds its alpha shows they aren't, and a picture without alpha is made opaque.
fn normalize(px: &mut [u32]) {
    let a = |p: u32| p >> 24;
    let m = |p: u32| ((p >> 16) & 0xFF).max((p >> 8) & 0xFF).max(p & 0xFF);
    if px.iter().all(|&p| a(p) == 0) {
        px.iter_mut().for_each(|p| *p |= 0xFF00_0000);
    } else if px.iter().any(|&p| m(p) > a(p)) {
        for p in px.iter_mut() {
            let al = a(*p);
            let c = |s: u32| ((((*p >> s) & 0xFF) * al + 127) / 255) << s;
            *p = (al << 24) | c(16) | c(8) | c(0);
        }
    }
}

/// `pic` shrunk (never enlarged) so its longer side is `side`, averaging the pixels each new
/// one covers, so a large picture drawn small doesn't alias.
pub fn fit(pic: &Pic, side: i32) -> Pic {
    let long = pic.w.max(pic.h);
    if side >= long || side < 1 {
        return Pic { w: pic.w, h: pic.h, pixels: pic.pixels.clone() };
    }
    let k = side as f32 / long as f32;
    let (w, h) = (((pic.w as f32 * k).round() as i32).max(1), ((pic.h as f32 * k).round() as i32).max(1));
    let mut pixels = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let (y0, y1) = (y * pic.h / h, ((y + 1) * pic.h + h - 1) / h);
        for x in 0..w {
            let (x0, x1) = (x * pic.w / w, ((x + 1) * pic.w + w - 1) / w);
            let mut sum = [0u32; 4];
            let mut n = 0;
            for sy in y0..y1.min(pic.h) {
                for sx in x0..x1.min(pic.w) {
                    let p = pic.pixels[(sy * pic.w + sx) as usize];
                    for (i, s) in [24, 16, 8, 0].into_iter().enumerate() {
                        sum[i] += (p >> s) & 0xFF;
                    }
                    n += 1;
                }
            }
            let n = n.max(1);
            pixels.push(((sum[0] / n) << 24) | ((sum[1] / n) << 16) | ((sum[2] / n) << 8) | (sum[3] / n));
        }
    }
    Pic { w, h, pixels }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(w: i32, h: i32, p: u32) -> Pic {
        Pic { w, h, pixels: vec![p; (w * h) as usize] }
    }

    #[test]
    fn fit_averages_and_never_enlarges() {
        let small = fit(&pic(32, 32, 0xFF10_2030), 64);
        assert_eq!((small.w, small.h), (32, 32), "not enlarged");
        let f = fit(&pic(256, 256, 0xFF10_2030), 64);
        assert_eq!((f.w, f.h), (64, 64));
        assert!(f.pixels.iter().all(|&p| p == 0xFF10_2030));
        // a checkerboard of opaque white and transparent averages to half alpha, premultiplied
        let mut c = pic(4, 4, 0);
        for (i, p) in c.pixels.iter_mut().enumerate() {
            if (i % 4 + i / 4) % 2 == 0 {
                *p = 0xFFFF_FFFF;
            }
        }
        let f = fit(&c, 2);
        assert!(f.pixels.iter().all(|&p| (p >> 24) == 127 && (p & 0xFF) == 127));
    }

    #[test]
    fn fit_keeps_aspect() {
        let f = fit(&pic(200, 100, 0xFF00_0000), 50);
        assert_eq!((f.w, f.h), (50, 25));
    }

    #[test]
    fn combine_reads_alpha_from_the_white_background() {
        // opaque red: same on both; fully transparent: black on black, white on white
        let out = combine(&[0xFFFF_0000, 0xFF00_0000, 0xFF00_0000], &[0xFFFF_0000, 0xFFFF_FFFF, 0xFF80_8080]);
        assert_eq!(out[0], 0xFFFF_0000);
        assert_eq!(out[1], 0x0000_0000);
        assert_eq!(out[2] >> 24, 127, "half-covered pixel is about half alpha");
    }

    #[test]
    fn normalize_premultiplies_straight_alpha_and_opaques_missing_alpha() {
        let mut px = [0x80FF_FFFF, 0x0000_0000];
        normalize(&mut px);
        assert_eq!(px[0], 0x8080_8080, "straight white at half alpha");
        let mut opaque = [0x00FF_0000, 0x0000_FF00];
        normalize(&mut opaque);
        assert_eq!(opaque, [0xFFFF_0000, 0xFF00_FF00], "no alpha at all means opaque");
        let mut ok = [0x80_40_40_40, 0xFF_10_20_30];
        let before = ok;
        normalize(&mut ok);
        assert_eq!(ok, before, "already premultiplied stays");
    }

    #[test]
    fn shell_gives_a_premultiplied_picture_for_an_executable() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let exe = format!(r"{}\explorer.exe", std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()));
        let pic = fetch(&Source::Path(exe)).expect("explorer.exe has an icon");
        assert!(pic.w >= 48 && pic.w == pic.h, "a real icon, not 16 px: {}x{}", pic.w, pic.h);
        let m = |p: u32| ((p >> 16) & 0xFF).max((p >> 8) & 0xFF).max(p & 0xFF);
        assert!(pic.pixels.iter().any(|&p| p >> 24 == 255), "has opaque pixels");
        assert!(pic.pixels.iter().any(|&p| p >> 24 == 0), "has transparent pixels");
        assert!(pic.pixels.iter().all(|&p| m(p) <= p >> 24), "premultiplied");
    }
}
