//! Atalho global Ctrl+Shift+F12 para iniciar/parar de qualquer lugar.

use std::mem::zeroed;
use std::ptr::null_mut;

use tauri::{AppHandle, Emitter};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, VK_F12,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || unsafe {
        if RegisterHotKey(null_mut(), 1, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_F12 as u32) == 0 {
            return;
        }
        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY {
                let _ = app.emit("hotkey:toggle", ());
            }
        }
    });
}
