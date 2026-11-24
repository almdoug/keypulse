//! Gravação livre da rota: captura as setas que o usuário aperta na janela alvo
//! (hook de teclado de baixo nível, funciona com o jogo em foco) e converte a
//! gravação nos trechos editáveis da rota (Parado / Andar × N).

use std::mem::zeroed;
use std::ptr::{null, null_mut};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetForegroundWindow, GetMessageW, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, GA_ROOT, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG,
    PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

use crate::winlist;

/// VK_LEFT, VK_UP, VK_RIGHT, VK_DOWN.
const ARROWS: std::ops::RangeInclusive<u32> = 37..=40;

struct Recording {
    app: AppHandle,
    target_hwnd: isize,
    target_process: String,
    started: Instant,
    /// (tecla, ms desde o início) de cada toque.
    presses: Vec<(u16, u64)>,
    /// Setas seguradas agora, para ignorar a repetição automática do teclado.
    held: [bool; 4],
    thread_id: u32,
    /// Última janela em primeiro plano consultada e se ela é o alvo.
    last_fg: (isize, bool),
}

impl Recording {
    fn target_in_front(&mut self) -> bool {
        let fg = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) };
        if fg as isize != self.last_fg.0 {
            let ok = fg as isize == self.target_hwnd
                || (!self.target_process.is_empty()
                    && winlist::window_process_name(fg).eq_ignore_ascii_case(&self.target_process));
            self.last_fg = (fg as isize, ok);
        }
        self.last_fg.1
    }

    fn on_key(&mut self, vk: u32, down: bool) {
        let slot = (vk - ARROWS.start()) as usize;
        if !down {
            self.held[slot] = false;
            return;
        }
        if self.held[slot] {
            return;
        }
        self.held[slot] = true;
        // Só conta o que foi apertado no jogo, não o que se digita em outras janelas.
        if !self.target_in_front() {
            return;
        }
        self.presses.push((vk as u16, self.started.elapsed().as_millis() as u64));
        let _ = self.app.emit("recorder:key", RecorderKey { count: self.presses.len(), vk: vk as u16 });
    }
}

#[derive(Serialize, Clone)]
struct RecorderKey {
    count: usize,
    vk: u16,
}

static RECORDING: Mutex<Option<Recording>> = Mutex::new(None);

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = &*(lparam as *const KBDLLHOOKSTRUCT);
        // Teclas injetadas (pelo próprio KeyPulse, por exemplo) não entram na gravação.
        if ARROWS.contains(&k.vkCode) && k.flags & LLKHF_INJECTED == 0 {
            let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            // try_lock: o hook nunca pode travar a entrada de teclado do sistema.
            if let Ok(mut guard) = RECORDING.try_lock() {
                if let Some(rec) = guard.as_mut() {
                    rec.on_key(k.vkCode, down);
                }
            }
        }
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

/// Trecho gravado, no mesmo formato que a interface usa para a rota.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RecordedLeg {
    Wait { sec: f64 },
    Move { vk: u16, count: u32 },
}

/// Converte os toques em trechos. Toques seguidos viram "Andar × N" (o ritmo da
/// caminhada fica por conta do tempo por sqm); pausas a partir de `max(1 s, 3 × sqm)`
/// viram "Parado", descontando o tempo por sqm que a rota já espera após cada passo.
/// O tempo entre o último toque e o fim da gravação vira a parada final, antes de a
/// rota voltar ao começo; o tempo antes do primeiro toque é ignorado.
pub fn to_legs(presses: &[(u16, u64)], end_ms: u64, step_ms: u64) -> Vec<RecordedLeg> {
    let pause = step_ms.saturating_mul(3).max(1000);
    let wait = |gap: u64| {
        let ms = gap.saturating_sub(step_ms).max(100);
        RecordedLeg::Wait { sec: ((ms + 50) / 100) as f64 / 10.0 }
    };
    let mut legs: Vec<RecordedLeg> = Vec::new();
    let mut last_t: Option<u64> = None;
    for &(vk, t) in presses {
        if let Some(prev) = last_t {
            if t.saturating_sub(prev) >= pause {
                legs.push(wait(t - prev));
            }
        }
        last_t = Some(t);
        match legs.last_mut() {
            Some(RecordedLeg::Move { vk: v, count }) if *v == vk => *count += 1,
            _ => legs.push(RecordedLeg::Move { vk, count: 1 }),
        }
    }
    if let Some(t) = last_t {
        if end_ms.saturating_sub(t) >= pause {
            legs.push(wait(end_ms - t));
        }
    }
    legs
}

#[tauri::command]
pub fn start_recording(app: AppHandle, hwnd: i64, process: String) -> Result<(), String> {
    if RECORDING.lock().unwrap().is_some() {
        return Err("Já existe uma gravação em andamento".into());
    }
    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    std::thread::spawn(move || unsafe {
        // Cria a fila de mensagens da thread antes de qualquer PostThreadMessage.
        let mut msg: MSG = zeroed();
        PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
        *RECORDING.lock().unwrap() = Some(Recording {
            app,
            target_hwnd: winlist::to_hwnd(hwnd) as isize,
            target_process: process,
            started: Instant::now(),
            presses: Vec::new(),
            held: [false; 4],
            thread_id: GetCurrentThreadId(),
            last_fg: (0, false),
        });
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), GetModuleHandleW(null()), 0);
        if hook.is_null() {
            *RECORDING.lock().unwrap() = None;
            let _ = tx.send(Err("O Windows não permitiu capturar o teclado".into()));
            return;
        }
        let _ = tx.send(Ok(()));
        // O hook de baixo nível só funciona enquanto esta thread processa mensagens.
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {}
        UnhookWindowsHookEx(hook);
    });
    rx.recv().map_err(|e| e.to_string())?
}

fn finish() -> Option<Recording> {
    let rec = RECORDING.lock().unwrap().take()?;
    unsafe { PostThreadMessageW(rec.thread_id, WM_QUIT, 0, 0) };
    Some(rec)
}

/// Encerra a gravação e devolve os trechos da rota.
#[tauri::command]
pub fn stop_recording(step_ms: u64) -> Result<Vec<RecordedLeg>, String> {
    let rec = finish().ok_or("Nenhuma gravação em andamento")?;
    if rec.presses.is_empty() {
        return Err("Nenhuma seta foi gravada. Grave com a janela do jogo em primeiro plano.".into());
    }
    let end = rec.started.elapsed().as_millis() as u64;
    Ok(to_legs(&rec.presses, end, step_ms))
}

#[tauri::command]
pub fn cancel_recording() {
    finish();
}

#[cfg(test)]
mod tests {
    use super::*;
    use RecordedLeg::{Move, Wait};

    const RIGHT: u16 = 39;
    const UP: u16 = 38;
    const LEFT: u16 = 37;

    #[test]
    fn groups_taps_and_turns_pauses_into_waits() {
        let presses = [
            (RIGHT, 5_000), // tempo antes do 1º toque é ignorado
            (RIGHT, 5_350),
            (RIGHT, 5_700),
            (UP, 6_100), // troca de direção sem pausa
            (UP, 6_450),
            (LEFT, 16_450), // 10 s parado
            (LEFT, 16_800),
        ];
        let legs = to_legs(&presses, 27_100, 300);
        assert_eq!(
            legs,
            vec![
                Move { vk: RIGHT, count: 3 },
                Move { vk: UP, count: 2 },
                Wait { sec: 9.7 },
                Move { vk: LEFT, count: 2 },
                Wait { sec: 10.0 },
            ]
        );
    }

    #[test]
    fn same_direction_after_a_pause_is_a_new_leg() {
        let legs = to_legs(&[(RIGHT, 0), (RIGHT, 300), (RIGHT, 5_300)], 5_400, 300);
        assert_eq!(legs, vec![Move { vk: RIGHT, count: 2 }, Wait { sec: 4.7 }, Move { vk: RIGHT, count: 1 }]);
    }
}
