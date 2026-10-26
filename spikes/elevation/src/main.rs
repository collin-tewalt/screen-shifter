//! Throwaway spike for openspec/changes/admin-windows-v11, tasks 1.1-1.7. Not part of the app.
//!
//! Install the test package (see package.ps1), then start "Screen Shifter elevation spike" from
//! Start (or run `ss-spike.exe`). It runs unelevated, asks you to approve a few UAC prompts, and
//! writes everything it finds to %PUBLIC%\Documents\ss-spike\results.txt (also printed at the end).
//! It only ever touches windows it created itself (its own elevated copy's test windows).

use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
use std::time::{Duration, Instant};
use windows::ApplicationModel::StartupTask;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{COLOR_WINDOW, GetSysColorBrush};
use windows::Win32::Security::*;
use windows::Win32::Storage::Packaging::Appx::{GetCurrentPackageFamilyName, GetCurrentPackageFullName};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
use windows::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_MENU, VK_RCONTROL, VK_RMENU};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

const MUTEX: PCWSTR = w!("Local\\ss-spike-mutex");
const CLASS: PCWSTR = w!("SsSpikeAdmin");
const WM_TRAYCB: u32 = WM_APP + 1;

fn results_path() -> std::path::PathBuf {
    let public = std::env::var("PUBLIC").unwrap_or_else(|_| r"C:\Users\Public".into());
    std::path::Path::new(&public).join("Documents").join("ss-spike").join("results.txt")
}

fn log(line: &str) {
    println!("{line}");
    let p = results_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "[pid {}] {line}", std::process::id());
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn pause(prompt: &str) {
    print!("\n>>> {prompt}\n    Press Enter to continue... ");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
}

fn package_name(family: bool) -> Option<String> {
    unsafe {
        let mut len = 0u32;
        let probe = if family { GetCurrentPackageFamilyName(&mut len, None) } else { GetCurrentPackageFullName(&mut len, None) };
        if probe != ERROR_INSUFFICIENT_BUFFER {
            return None; // APPMODEL_ERROR_NO_PACKAGE: no identity
        }
        let mut buf = vec![0u16; len as usize];
        let r = if family {
            GetCurrentPackageFamilyName(&mut len, Some(PWSTR(buf.as_mut_ptr())))
        } else {
            GetCurrentPackageFullName(&mut len, Some(PWSTR(buf.as_mut_ptr())))
        };
        (r == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]))
    }
}

/// (elevated, elevation type, integrity RID) of a process's token, or why it couldn't be read.
fn token_facts(process: HANDLE) -> Result<(bool, i32, u32), String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).map_err(|e| format!("OpenProcessToken: {e}"))?;
        let mut elev = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let r1 = GetTokenInformation(token, TokenElevation, Some(&mut elev as *mut _ as *mut _), size_of::<TOKEN_ELEVATION>() as u32, &mut len);
        let mut etype = TOKEN_ELEVATION_TYPE(0);
        let r2 = GetTokenInformation(token, TokenElevationType, Some(&mut etype as *mut _ as *mut _), size_of::<TOKEN_ELEVATION_TYPE>() as u32, &mut len);
        let mut buf = vec![0u8; 256];
        let r3 = GetTokenInformation(token, TokenIntegrityLevel, Some(buf.as_mut_ptr() as *mut _), buf.len() as u32, &mut len);
        let _ = CloseHandle(token);
        r1.map_err(|e| format!("TokenElevation: {e}"))?;
        r2.map_err(|e| format!("TokenElevationType: {e}"))?;
        r3.map_err(|e| format!("TokenIntegrityLevel: {e}"))?;
        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let sid = label.Label.Sid;
        let n = *GetSidSubAuthorityCount(sid);
        let rid = *GetSidSubAuthority(sid, (n - 1) as u32);
        Ok((elev.TokenIsElevated != 0, etype.0, rid))
    }
}

fn describe(facts: &Result<(bool, i32, u32), String>) -> String {
    match facts {
        Ok((e, t, rid)) => {
            let ty = match *t { 1 => "Default", 2 => "Full", 3 => "Limited", _ => "?" };
            let il = match *rid { 0x1000 => "Low", 0x2000 => "Medium", 0x2100 => "Medium+", 0x3000 => "High", 0x4000 => "System", _ => "other" };
            format!("elevated={e} type={ty} integrity={il} (0x{rid:x})")
        }
        Err(e) => format!("token unreadable: {e}"),
    }
}

fn me() -> String {
    let facts = token_facts(unsafe { GetCurrentProcess() });
    format!(
        "{} identity={} localappdata={}",
        describe(&facts),
        package_name(false).unwrap_or_else(|| "NONE".into()),
        std::env::var("LOCALAPPDATA").unwrap_or_default()
    )
}

fn is_elevated() -> bool {
    matches!(token_facts(unsafe { GetCurrentProcess() }), Ok((true, _, _)))
}

fn probe_path() -> std::path::PathBuf {
    std::path::Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default()).join("ss-spike").join("probe.txt")
}

/// Reads the probe, then replaces it (write a temp file, rename over) with `who`.
fn probe_swap(who: &str) {
    let p = probe_path();
    let read = std::fs::read_to_string(&p).map(|s| s.trim().to_string()).unwrap_or_else(|e| format!("<unreadable: {e}>"));
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let tmp = p.with_extension("tmp");
    let write = std::fs::write(&tmp, format!("written by {who}"))
        .and_then(|_| std::fs::rename(&tmp, &p))
        .map(|_| "ok".to_string())
        .unwrap_or_else(|e| format!("FAILED: {e}"));
    log(&format!("  probe [1.3]: read \"{read}\"; replace as {who}: {write}"));
}

fn startup_task_state() -> String {
    StartupTask::GetAsync(&HSTRING::from("SpikeTask"))
        .and_then(|op| op.join())
        .and_then(|t| t.State())
        .map(|s| format!("{s:?}"))
        .unwrap_or_else(|e| format!("FAILED: {e}"))
}

// ---------- runas ----------

static TICKS: AtomicU32 = AtomicU32::new(0);
unsafe extern "system" fn tick(_: HWND, _: u32, _: usize, _: u32) {
    TICKS.fetch_add(1, SeqCst);
}

/// ShellExecuteExW("runas"). Returns the process handle on success.
fn runas(route: &str, file: &str, params: &str) -> Option<HANDLE> {
    let (file_w, params_w) = (wide(file), wide(params));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: PCWSTR(params_w.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    TICKS.store(0, SeqCst);
    let timer = unsafe { SetTimer(None, 0, 50, Some(tick)) }; // only fires if the call pumps messages
    let t = Instant::now();
    let r = unsafe { ShellExecuteExW(&mut info) };
    let ms = t.elapsed().as_millis();
    unsafe {
        let _ = KillTimer(None, timer);
    }
    let pumped = TICKS.load(SeqCst);
    let handle = (!info.hProcess.is_invalid()).then_some(info.hProcess);
    log(&format!(
        "ROUTE {route} [1.2]: file={file} -> {} | process handle: {} | took {ms} ms | timer ticks during the call: {pumped} ({})",
        match &r { Ok(()) => "TRUE".into(), Err(e) => format!("FALSE ({e})") },
        handle.map(|h| format!("yes, pid {}", unsafe { GetProcessId(h) })).unwrap_or("NO".into()),
        if pumped > 0 { "it pumps messages" } else { "it blocks without pumping" },
    ));
    handle
}

// ---------- child: launched elevated by a route ----------

fn child(route: &str) {
    log(&format!("CHILD route={route} args={:?} {}", std::env::args().skip(1).collect::<Vec<_>>(), me()));
    probe_swap(&format!("elevated child ({route})"));
    log(&format!("  StartupTask API from this process [1.4]: {}", startup_task_state()));
}

// ---------- the elevated copy with test windows ----------

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static FILTERED: std::sync::Mutex<isize> = std::sync::Mutex::new(0);
static CLICKS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static RESTARTED: AtomicBool = AtomicBool::new(false);

fn which(h: HWND) -> usize {
    if h.0 as isize == *FILTERED.lock().unwrap() { 0 } else { 1 }
}

fn tray(h: HWND, op: NOTIFY_ICON_MESSAGE) {
    let k = which(h);
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: h,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAYCB,
        hIcon: unsafe { LoadIconW(None, if k == 0 { IDI_INFORMATION } else { IDI_WARNING }).unwrap_or_default() },
        ..Default::default()
    };
    let tip: Vec<u16> = (if k == 0 { "ss-spike FILTERED" } else { "ss-spike UNFILTERED" }).encode_utf16().collect();
    nid.szTip[..tip.len()].copy_from_slice(&tip);
    unsafe {
        let _ = Shell_NotifyIconW(op, &nid);
    }
}

unsafe extern "system" fn admin_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let name = ["filtered", "unfiltered"][which(h)];
    if msg != 0 && msg == TASKBAR_CREATED.load(SeqCst) {
        RESTARTED.store(true, SeqCst);
        log(&format!("ADMIN [1.7]: TaskbarCreated RECEIVED by the {name} window; re-adding its tray icon"));
        tray(h, NIM_ADD);
        return LRESULT(0);
    }
    match msg {
        WM_TRAYCB => {
            let ev = lp.0 as u32;
            if matches!(ev, WM_LBUTTONUP | WM_RBUTTONUP) {
                let n = CLICKS[which(h)].fetch_add(1, SeqCst) + 1;
                let when = if RESTARTED.load(SeqCst) { "AFTER the Explorer restart" } else { "before any Explorer restart" };
                log(&format!("ADMIN [1.7]: tray click #{n} RECEIVED by the {name} icon, {when}"));
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(h, msg, wp, lp) },
    }
}

fn admin() {
    log(&format!("ADMIN start {}", me()));
    log(&format!("  StartupTask API elevated [1.4]: {}", startup_task_state()));
    let mutex = unsafe { CreateMutexW(None, true, MUTEX) };
    log(&format!("  created mutex {}: {:?}", "Local\\ss-spike-mutex", mutex.as_ref().map(|_| "ok")));
    unsafe {
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), SeqCst);
        let inst = GetModuleHandleW(None).unwrap_or_default().into();
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(admin_proc), hInstance: inst, lpszClassName: CLASS, hbrBackground: GetSysColorBrush(COLOR_WINDOW), ..Default::default() });
        let mk = |title: PCWSTR, x: i32| {
            CreateWindowExW(WINDOW_EX_STYLE(0), CLASS, title, WS_OVERLAPPEDWINDOW | WS_VISIBLE, x, 200, 520, 220, None, None, Some(inst), None).unwrap_or_default()
        };
        let filtered = mk(w!("Spike admin window (filtered) - close either to end"), 120);
        *FILTERED.lock().unwrap() = filtered.0 as isize;
        let unfiltered = mk(w!("Spike admin window (unfiltered) - close either to end"), 680);
        let f = ChangeWindowMessageFilterEx(filtered, TASKBAR_CREATED.load(SeqCst), MSGFLT_ALLOW, None);
        let f2 = ChangeWindowMessageFilterEx(filtered, WM_TRAYCB, MSGFLT_ALLOW, None);
        log(&format!("  ChangeWindowMessageFilterEx on the filtered window: TaskbarCreated {:?}, tray callback {:?}", f.is_ok(), f2.is_ok()));
        tray(filtered, NIM_ADD);
        tray(unfiltered, NIM_ADD);
        SetTimer(None, 0, 300_000, Some(quit_timer)); // ends by itself after 5 minutes
        let mut m = MSG::default();
        while GetMessageW(&mut m, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&m);
            DispatchMessageW(&m);
        }
        tray(filtered, NIM_DELETE);
        tray(unfiltered, NIM_DELETE);
        log(&format!("ADMIN [1.7]: tray clicks in total: filtered {}, unfiltered {}", CLICKS[0].load(SeqCst), CLICKS[1].load(SeqCst)));
    }
    log("ADMIN exits");
}

unsafe extern "system" fn quit_timer(_: HWND, _: u32, _: usize, _: u32) {
    unsafe { PostQuitMessage(0) };
}

// ---------- the unelevated run that drives everything ----------

static KEYS_ON_ADMIN: AtomicU32 = AtomicU32::new(0);
static KEYS_ELSEWHERE: AtomicU32 = AtomicU32::new(0);
static ADMIN_PID: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn kbd(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        let vk = k.vkCode as u16;
        if [VK_CONTROL.0, VK_LCONTROL.0, VK_RCONTROL.0, VK_MENU.0, VK_LMENU.0, VK_RMENU.0].contains(&vk) {
            let mut pid = 0u32;
            unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
            if pid == ADMIN_PID.load(SeqCst) { &KEYS_ON_ADMIN } else { &KEYS_ELSEWHERE }.fetch_add(1, SeqCst);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn pump_for(d: Duration) {
    let end = Instant::now() + d;
    unsafe {
        let mut m = MSG::default();
        while Instant::now() < end {
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// The test windows of the elevated copy we launched (matched by its pid).
fn admin_windows(pid: u32) -> Vec<HWND> {
    let mut v = Vec::new();
    let mut prev: Option<HWND> = None;
    loop {
        let h = unsafe { FindWindowExW(None, prev, CLASS, None) }.unwrap_or_default();
        if h.is_invalid() {
            break;
        }
        let mut p = 0u32;
        unsafe { GetWindowThreadProcessId(h, Some(&mut p)) };
        if p == pid {
            v.push(h);
        }
        prev = Some(h);
    }
    v
}

fn medium() {
    let _ = std::fs::remove_file(results_path());
    log(&format!("MAIN (unelevated) {}", me()));
    let Some(pfn) = package_name(true) else {
        log("No package identity: install the spike package and start it from Start or with ss-spike.exe.");
        pause("Nothing to do");
        return;
    };
    let exe = {
        let mut buf = [0u16; 1024];
        let n = unsafe { GetModuleFileNameW(None, &mut buf) } as usize;
        String::from_utf16_lossy(&buf[..n])
    };
    let _ = std::fs::create_dir_all(probe_path().parent().unwrap());
    let _ = std::fs::write(probe_path(), "written by the unelevated main run");
    log(&format!("  probe written at {}", probe_path().display()));

    // 1.2 / 1.3 / 1.4: three ways of relaunching elevated
    let nocap = pfn.contains("NoCap");
    log(&format!("  this package {} the allowElevation capability", if nocap { "does NOT declare" } else { "declares" }));
    let all = [
        ("exe-path", exe.clone()),
        ("alias", "ss-spike.exe".to_string()),
        ("appsfolder", format!("shell:AppsFolder\\{pfn}!Spike")),
    ];
    let routes: Vec<(&str, String)> = if nocap { vec![all[0].clone()] } else { all.to_vec() };
    for (route, file) in &routes {
        pause(&format!("Route '{route}': a UAC prompt will appear. Approve it (Yes)."));
        if let Some(h) = runas(route, file, &format!("--child {route}")) {
            unsafe {
                WaitForSingleObject(h, 30_000);
                let _ = CloseHandle(h);
            }
        }
    }
    let read = std::fs::read_to_string(probe_path()).unwrap_or_default();
    log(&format!("  probe after the children [1.3], read by the unelevated run: \"{}\"", read.trim()));
    probe_swap("the unelevated main run");

    // Pick a route that kept the identity for the elevated test copy.
    let results = std::fs::read_to_string(results_path()).unwrap_or_default();
    let good = routes
        .iter()
        .find(|(r, _)| results.lines().any(|l| l.contains(&format!("CHILD route={r} ")) && !l.contains("identity=NONE")))
        .or(routes.first())
        .unwrap()
        .clone();
    log(&format!("Using route '{}' for the elevated test copy", good.0));
    pause("The elevated test copy: another UAC prompt. Approve it.");
    let Some(h) = runas(&format!("{} (test copy)", good.0), &good.1, "--admin") else {
        log("Couldn't start the elevated test copy; stopping here.");
        pause("Done");
        return;
    };
    let pid = unsafe { GetProcessId(h) };
    ADMIN_PID.store(pid, SeqCst);
    std::thread::sleep(Duration::from_secs(2));
    let wins = admin_windows(pid);
    log(&format!("  found {} test window(s) of the elevated copy (pid {pid})", wins.len()));

    // 1.6: the single-instance mutex
    let m = unsafe { CreateMutexW(None, true, MUTEX) };
    let last = unsafe { GetLastError() };
    log(&format!("MUTEX [1.6]: CreateMutexW from unelevated -> {} (GetLastError after: {:?})", match &m { Ok(_) => "Ok".to_string(), Err(e) => format!("Err {e}") }, last));
    if let Ok(mh) = m {
        unsafe { let _ = CloseHandle(mh); }
    }

    // 1.5: the elevated window's process, seen from here
    let open = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
    match open {
        Ok(ph) => {
            log(&format!("INTEGRITY [1.5]: OpenProcess(limited) ok; {}", describe(&token_facts(ph))));
            unsafe { let _ = CloseHandle(ph); }
        }
        Err(e) => log(&format!("INTEGRITY [1.5]: OpenProcess(limited) FAILED: {e}")),
    }

    // UIPI: can we hide its window?
    if let Some(&w2) = wins.last() {
        let before = unsafe { IsWindowVisible(w2).as_bool() };
        let r = unsafe { ShowWindowAsync(w2, SW_HIDE) };
        std::thread::sleep(Duration::from_millis(400));
        let after = unsafe { IsWindowVisible(w2).as_bool() };
        log(&format!("UIPI: ShowWindowAsync(SW_HIDE) on the elevated window -> {} ; visible before={before} after={after}", r.as_bool()));
        if before && !after {
            unsafe { let _ = ShowWindowAsync(w2, SW_SHOW); }
        }
    }

    // 1.1: the hooks while an elevated window has focus
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(kbd), None, 0) };
    log(&format!("  keyboard hook installed: {}", hook.is_ok()));
    pause("Hook control: after Enter, stay in THIS console window and press and release Ctrl+Alt a few times (10 seconds).");
    KEYS_ON_ADMIN.store(0, SeqCst);
    KEYS_ELSEWHERE.store(0, SeqCst);
    println!("    ...10 seconds, press Ctrl+Alt here");
    pump_for(Duration::from_secs(10));
    log(&format!(
        "HOOKS [1.1] control: Ctrl/Alt events seen while a NORMAL window had focus: {} (must be > 0 for the next line to mean anything)",
        KEYS_ELSEWHERE.load(SeqCst)
    ));
    pause("Hook test: after Enter, within 15 seconds CLICK one of the 'Spike admin window' windows and press and release Ctrl+Alt a few times. Then click back here.");
    KEYS_ON_ADMIN.store(0, SeqCst);
    KEYS_ELSEWHERE.store(0, SeqCst);
    println!("    ...15 seconds, go");
    pump_for(Duration::from_secs(15));
    log(&format!(
        "HOOKS [1.1]: Ctrl/Alt events seen while the ELEVATED window had focus: {}; elsewhere: {}",
        KEYS_ON_ADMIN.load(SeqCst),
        KEYS_ELSEWHERE.load(SeqCst)
    ));
    if let Ok(hk) = hook {
        unsafe { let _ = UnhookWindowsHookEx(hk); }
    }

    // 1.7: tray while elevated
    pause("Tray test [1.7]: click each of the two new tray icons (an 'i' and a '!'; maybe under the ^ arrow) once. Then restart Explorer (Task Manager > Windows Explorer > Restart), wait for the taskbar, and click each icon that came back once more. Every click is logged now.");
    pause("Finally: close one of the 'Spike admin window' windows (that ends the elevated test copy).");
    unsafe {
        WaitForSingleObject(h, 60_000);
        let _ = CloseHandle(h);
    }
    log("MAIN done. Paste the whole results file to Claude.");
    println!("\n===== {} =====\n{}", results_path().display(), std::fs::read_to_string(results_path()).unwrap_or_default());
    pause("Done");
}

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    }
    let args: Vec<String> = std::env::args().collect();
    let flag = |f: &str| args.iter().position(|a| a == f);
    if let Some(i) = flag("--child") {
        child(args.get(i + 1).map(String::as_str).unwrap_or("?"));
    } else if flag("--admin").is_some() {
        admin();
    } else if is_elevated() {
        child("ARGS-LOST (started elevated without --child)");
    } else {
        medium();
    }
}
