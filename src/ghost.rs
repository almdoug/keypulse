//! Restauração invisível de janelas minimizadas.
//!
//! Muitos apps (Chrome, Electron, jogos, players) ignoram teclado ou pausam a
//! renderização enquanto estão minimizados. Para enviar teclas sem "abrir" a janela,
//! ela é restaurada totalmente transparente, sem ativação, sem animação e sem
//! receber cliques; ao final é minimizada de novo e tudo volta ao que era.

use std::ffi::c_void;
use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;

use windows_sys::Win32::Foundation::{BOOL, COLORREF, HWND};
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetLayeredWindowAttributes, GetWindowLongPtrW, IsIconic, SetLayeredWindowAttributes,
    SetWindowLongPtrW, ShowWindowAsync, GWL_EXSTYLE, LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA,
    SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, WS_EX_LAYERED, WS_EX_TRANSPARENT,
};

/// Tempo para o app voltar a processar entrada depois de "desminimizado".
const SETTLE: Duration = Duration::from_millis(150);

pub struct GhostRestore {
    hwnd: HWND,
    ex_style: isize,
    layered: Option<(COLORREF, u8, LAYERED_WINDOW_ATTRIBUTES_FLAGS)>,
}

fn set_transitions_disabled(hwnd: HWND, disabled: bool) {
    let value: BOOL = disabled as BOOL;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            &value as *const BOOL as *const c_void,
            size_of::<BOOL>() as u32,
        );
    }
}

fn wait_iconic(hwnd: HWND, iconic: bool) {
    for _ in 0..50 {
        if (unsafe { IsIconic(hwnd) } != 0) == iconic {
            return;
        }
        sleep(Duration::from_millis(10));
    }
}

impl GhostRestore {
    /// Restaura a janela invisivelmente se ela estiver minimizada; `None` caso contrário.
    pub fn begin(hwnd: HWND) -> Option<Self> {
        unsafe {
            if IsIconic(hwnd) == 0 {
                return None;
            }
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let mut layered = None;
            if ex_style as u32 & WS_EX_LAYERED != 0 {
                let (mut key, mut alpha, mut flags) = (0, 0, 0);
                if GetLayeredWindowAttributes(hwnd, &mut key, &mut alpha, &mut flags) != 0 {
                    layered = Some((key, alpha, flags));
                }
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_LAYERED | WS_EX_TRANSPARENT) as isize);
            SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA);
            set_transitions_disabled(hwnd, true);
            // Assíncrono: um app travado não pode travar o KeyPulse.
            ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE);
            wait_iconic(hwnd, false);
            sleep(SETTLE);
            Some(Self { hwnd, ex_style, layered })
        }
    }
}

impl Drop for GhostRestore {
    fn drop(&mut self) {
        let hwnd = self.hwnd;
        unsafe {
            if IsIconic(hwnd) == 0 {
                ShowWindowAsync(hwnd, SW_SHOWMINNOACTIVE);
                wait_iconic(hwnd, true);
            }
            // Só depois de minimizada a aparência original volta, para não piscar na tela.
            if let Some((key, alpha, flags)) = self.layered {
                SetLayeredWindowAttributes(hwnd, key, alpha, flags);
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, self.ex_style);
            set_transitions_disabled(hwnd, false);
        }
    }
}
