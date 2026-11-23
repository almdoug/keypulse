//! Enumeração das janelas de nível superior (as mesmas que aparecem no Alt+Tab),
//! com nome do processo, ícone em PNG e detecção de privilégio de administrador.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use serde::Serialize;
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HWND, LPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::ExtractIconExW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, DrawIconEx, EnumWindows, GetClassLongPtrW, GetDesktopWindow, GetWindow,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SendMessageTimeoutW, DI_NORMAL, GCLP_HICON, GCLP_HICONSM, GWL_EXSTYLE,
    GW_CHILD, GW_HWNDNEXT, GW_OWNER, HICON, ICON_BIG, ICON_SMALL2, SMTO_ABORTIFHUNG, WM_GETICON,
    WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

const ICON_SIZE: i32 = 32;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub hwnd: i64,
    pub title: String,
    pub process: String,
    pub exe_path: String,
    pub pid: u32,
    pub icon: Option<String>,
    pub minimized: bool,
    pub elevated: bool,
}

pub fn to_hwnd(v: i64) -> HWND {
    v as isize as HWND
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam as *mut Vec<HWND>);
    out.push(hwnd);
    1
}

/// Todas as janelas de nível superior, em ordem Z (a mais recente primeiro).
///
/// `EnumWindows` é a via normal, mas o Windows esconde algumas janelas nela:
/// o cliente do RubinOT, por exemplo, tem janelas visíveis, com título, no topo
/// da ordem Z e ainda assim nunca aparece na enumeração (nem no Alt+Tab). A
/// cadeia de janelas irmãs do desktop (`GetWindow`) não passa por esse filtro,
/// então lemos as duas fontes e unimos os resultados.
pub fn enum_top_level() -> Vec<HWND> {
    let mut out: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), &mut out as *mut Vec<HWND> as LPARAM);

        let mut seen: HashSet<isize> = out.iter().map(|h| *h as isize).collect();
        let mut hwnd = GetWindow(GetDesktopWindow(), GW_CHILD);
        let mut guard = 0u32;
        while !hwnd.is_null() && guard < 10_000 {
            if seen.insert(hwnd as isize) {
                out.push(hwnd);
            }
            hwnd = GetWindow(hwnd, GW_HWNDNEXT);
            guard += 1;
        }
    }
    out
}

pub fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

pub fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            &mut cloaked as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
        )
    };
    hr == 0 && cloaked != 0
}

/// Mesma regra usada pelo Alt+Tab: visível, não é tool window, não é janela
/// "possuída" (diálogos) e não está oculta pelo DWM (apps UWP suspensos).
pub fn is_task_window(hwnd: HWND) -> bool {
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW != 0 {
            return false;
        }
        if !GetWindow(hwnd, GW_OWNER).is_null() && ex & WS_EX_APPWINDOW == 0 {
            return false;
        }
    }
    !is_cloaked(hwnd)
}

pub fn process_path(pid: u32) -> String {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size);
        CloseHandle(h);
        if ok == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..size as usize])
    }
}

pub fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or_default().to_string()
}

pub fn window_process_name(hwnd: HWND) -> String {
    file_name(&process_path(window_pid(hwnd)))
}

unsafe fn token_elevated(token: HANDLE) -> bool {
    let mut te: TOKEN_ELEVATION = zeroed();
    let mut len = 0u32;
    GetTokenInformation(
        token,
        TokenElevation,
        &mut te as *mut TOKEN_ELEVATION as *mut c_void,
        size_of::<TOKEN_ELEVATION>() as u32,
        &mut len,
    ) != 0
        && te.TokenIsElevated != 0
}

pub fn self_elevated() -> bool {
    static SELF: OnceLock<bool> = OnceLock::new();
    *SELF.get_or_init(|| unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let e = token_elevated(token);
        CloseHandle(token);
        e
    })
}

/// `true` quando a janela roda como administrador e nós não — nesse caso o
/// Windows (UIPI) bloqueia silenciosamente as teclas simuladas.
fn process_elevated(pid: u32) -> bool {
    if self_elevated() {
        return false;
    }
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut token: HANDLE = null_mut();
        let elevated = if OpenProcessToken(h, TOKEN_QUERY, &mut token) == 0 {
            true
        } else {
            let e = token_elevated(token);
            CloseHandle(token);
            e
        };
        CloseHandle(h);
        elevated
    }
}

/// Desenha o ícone sobre fundo preto e branco e reconstrói o canal alfa a
/// partir da diferença — funciona tanto para ícones modernos quanto legados.
unsafe fn render_icon(icon: HICON) -> Option<String> {
    let size = ICON_SIZE;
    let dc = CreateCompatibleDC(null_mut());
    if dc.is_null() {
        return None;
    }
    let mut bmi: BITMAPINFO = zeroed();
    bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = size;
    bmi.bmiHeader.biHeight = -size;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB as u32;
    let mut bits: *mut c_void = null_mut();
    let bmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bmp.is_null() || bits.is_null() {
        DeleteDC(dc);
        return None;
    }
    let old = SelectObject(dc, bmp);
    let n = (size * size) as usize;
    let px = std::slice::from_raw_parts_mut(bits as *mut u32, n);

    let mut passes: Vec<Vec<u32>> = Vec::with_capacity(2);
    for bg in [0xFF00_0000u32, 0xFFFF_FFFF] {
        px.fill(bg);
        DrawIconEx(dc, 0, 0, icon, size, size, 0, null_mut(), DI_NORMAL);
        GdiFlush();
        passes.push(px.to_vec());
    }
    SelectObject(dc, old);
    DeleteObject(bmp);
    DeleteDC(dc);

    let (black, white) = (&passes[0], &passes[1]);
    let ch = |v: u32, shift: u32| ((v >> shift) & 0xFF) as i32;
    let mut rgba = Vec::with_capacity(n * 4);
    let mut visible = false;
    for i in 0..n {
        let (b, w) = (black[i], white[i]);
        let diff = ((ch(w, 16) - ch(b, 16)) + (ch(w, 8) - ch(b, 8)) + (ch(w, 0) - ch(b, 0))) / 3;
        let a = (255 - diff).clamp(0, 255);
        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        visible = true;
        let un = |c: i32| ((c * 255 + a / 2) / a).clamp(0, 255) as u8;
        rgba.extend_from_slice(&[un(ch(b, 16)), un(ch(b, 8)), un(ch(b, 0)), a as u8]);
    }
    if !visible {
        return None;
    }

    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, size as u32, size as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out)
    ))
}

fn window_icon(hwnd: HWND, exe: &str) -> Option<String> {
    unsafe {
        let mut h: usize = 0;
        for which in [ICON_BIG, ICON_SMALL2] {
            if h == 0 {
                SendMessageTimeoutW(hwnd, WM_GETICON, which as usize, 0, SMTO_ABORTIFHUNG, 60, &mut h);
            }
        }
        if h == 0 {
            h = GetClassLongPtrW(hwnd, GCLP_HICON);
        }
        if h == 0 {
            h = GetClassLongPtrW(hwnd, GCLP_HICONSM);
        }
        if h != 0 {
            if let Some(png) = render_icon(h as HICON) {
                return Some(png);
            }
        }
        if !exe.is_empty() {
            let wide: Vec<u16> = exe.encode_utf16().chain(std::iter::once(0)).collect();
            let mut large: HICON = null_mut();
            if ExtractIconExW(wide.as_ptr(), 0, &mut large, null_mut(), 1) > 0 && !large.is_null() {
                let png = render_icon(large);
                DestroyIcon(large);
                return png;
            }
        }
        None
    }
}

fn icon_cache() -> &'static Mutex<HashMap<isize, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<isize, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn collect_windows() -> Vec<WindowInfo> {
    let me = std::process::id();
    let mut cache = icon_cache().lock().unwrap();
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for hwnd in enum_top_level() {
        if !is_task_window(hwnd) {
            continue;
        }
        let title = window_title(hwnd);
        if title.trim().is_empty() {
            continue;
        }
        let pid = window_pid(hwnd);
        if pid == me {
            continue;
        }
        let exe_path = process_path(pid);
        let process = file_name(&exe_path);
        if process.eq_ignore_ascii_case("explorer.exe") && title == "Program Manager" {
            continue;
        }
        let key = hwnd as isize;
        seen.insert(key);
        let icon = cache
            .entry(key)
            .or_insert_with(|| window_icon(hwnd, &exe_path))
            .clone();
        out.push(WindowInfo {
            hwnd: key as i64,
            title,
            process: if process.is_empty() { "desconhecido".into() } else { process },
            exe_path,
            pid,
            icon,
            minimized: unsafe { IsIconic(hwnd) != 0 },
            elevated: process_elevated(pid),
        });
    }
    cache.retain(|k, _| seen.contains(k));
    out
}

#[tauri::command]
pub async fn list_windows() -> Vec<WindowInfo> {
    collect_windows()
}

#[tauri::command]
pub async fn focus_window(hwnd: i64) -> Result<(), String> {
    if crate::input::bring_to_front(to_hwnd(hwnd)) {
        Ok(())
    } else {
        Err("O Windows não permitiu trazer a janela para frente".into())
    }
}

#[tauri::command]
pub fn is_admin() -> bool {
    self_elevated()
}
