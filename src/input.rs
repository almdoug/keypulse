//! Simulação de teclado e mouse: via SendInput (janela em foco) ou via
//! PostMessage (segundo plano, sem roubar o foco).

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use std::thread::sleep;
use std::time::Duration;

use serde::Deserialize;
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, ScreenToClient};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardState, MapVirtualKeyW, SendInput, SetKeyboardState, INPUT, INPUT_KEYBOARD,
    INPUT_MOUSE, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSE_EVENT_FLAGS, VK_CONTROL, VK_LBUTTON, VK_LCONTROL,
    VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MBUTTON, VK_MENU, VK_RBUTTON, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, ChildWindowFromPointEx, GetAncestor, GetClientRect, GetCursorPos,
    GetForegroundWindow, GetGUIThreadInfo, GetWindowRect, GetWindowThreadProcessId, IsChild,
    IsIconic, IsWindow, PeekMessageW, PostMessageW, SendMessageTimeoutW, SetCursorPos,
    SetForegroundWindow, ShowWindow, WindowFromPoint, CWP_SKIPDISABLED, CWP_SKIPINVISIBLE,
    CWP_SKIPTRANSPARENT, GA_ROOT, GUITHREADINFO, MSG, PM_NOREMOVE, SMTO_ABORTIFHUNG, SW_RESTORE,
    WA_ACTIVE, WA_INACTIVE, WHEEL_DELTA, WM_ACTIVATE, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NULL,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

/// Tecla "neutra" sem função atribuída, usada para impedir que um toque de
/// Alt ative a barra de menus (mesma técnica do AutoHotkey).
const VK_MASK: u16 = 0xE8;

/// Flags `MK_*` do wParam das mensagens de mouse.
const MK_LBUTTON: usize = 0x01;
const MK_RBUTTON: usize = 0x02;
const MK_SHIFT: usize = 0x04;
const MK_CONTROL: usize = 0x08;
const MK_MBUTTON: usize = 0x10;

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MouseAction {
    Left,
    Right,
    Middle,
    WheelUp,
    WheelDown,
}

/// Como enviar um clique: botão virtual, flag MK_*, mensagens down/up e flags SendInput down/up.
struct Button {
    vk: u16,
    mk: usize,
    msg_down: u32,
    msg_up: u32,
    input_down: MOUSE_EVENT_FLAGS,
    input_up: MOUSE_EVENT_FLAGS,
}

impl MouseAction {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "Clique esquerdo",
            Self::Right => "Clique direito",
            Self::Middle => "Clique do scroll",
            Self::WheelUp => "Scroll ↑",
            Self::WheelDown => "Scroll ↓",
        }
    }

    fn button(self) -> Option<Button> {
        let b = |vk, mk, msg_down, msg_up, input_down, input_up| Button { vk, mk, msg_down, msg_up, input_down, input_up };
        match self {
            Self::Left => Some(b(VK_LBUTTON, MK_LBUTTON, WM_LBUTTONDOWN, WM_LBUTTONUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)),
            Self::Right => Some(b(VK_RBUTTON, MK_RBUTTON, WM_RBUTTONDOWN, WM_RBUTTONUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)),
            Self::Middle => Some(b(VK_MBUTTON, MK_MBUTTON, WM_MBUTTONDOWN, WM_MBUTTONUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP)),
            Self::WheelUp | Self::WheelDown => None,
        }
    }

    fn wheel_delta(self) -> i32 {
        match self {
            Self::WheelUp => WHEEL_DELTA as i32,
            Self::WheelDown => -(WHEEL_DELTA as i32),
            _ => 0,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Combo {
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub win: bool,
    #[serde(default)]
    pub vk: u16,
    /// Quando presente, o passo é uma ação de mouse e `vk` é ignorado.
    #[serde(default)]
    pub mouse: Option<MouseAction>,
    #[serde(default)]
    pub label: String,
}

impl Combo {
    fn modifiers(&self) -> Vec<u16> {
        let mut m = Vec::with_capacity(4);
        if self.ctrl {
            m.push(VK_CONTROL);
        }
        if self.alt {
            m.push(VK_MENU);
        }
        if self.shift {
            m.push(VK_SHIFT);
        }
        if self.win {
            m.push(VK_LWIN);
        }
        m
    }

    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")] {
            if on {
                parts.push(name.into());
            }
        }
        parts.push(match (self.mouse, self.label.is_empty()) {
            (_, false) => self.label.clone(),
            (Some(m), true) => m.name().into(),
            (None, true) => format!("VK {:#04X}", self.vk),
        });
        parts.join("+")
    }
}

fn is_extended(vk: u16) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2C | 0x2D | 0x2E | 0x5B | 0x5C | 0x5D | 0x6F | 0x90 | 0xA3 | 0xA5)
}

fn scan_code(vk: u16) -> u16 {
    unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) as u16 }
}

fn key_input(vk: u16, up: bool) -> INPUT {
    let mut flags = 0;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let mut input: INPUT = unsafe { zeroed() };
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;
    input.Anonymous.ki.wScan = scan_code(vk);
    input.Anonymous.ki.dwFlags = flags;
    input
}

fn send(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err("O Windows bloqueou a simulação de teclas (SendInput)".into())
    }
}

/// Pressiona modificadores → tecla (ou botão do mouse) → solta tudo na ordem inversa.
/// A janela alvo precisa estar em primeiro plano.
pub fn press_combo(hwnd: HWND, c: &Combo, hold_ms: u64) -> Result<(), String> {
    let mods = c.modifiers();
    let downs: Vec<INPUT> = mods.iter().map(|&m| key_input(m, false)).collect();
    if !downs.is_empty() {
        send(&downs)?;
        sleep(Duration::from_millis(12));
    }
    let result = match c.mouse {
        Some(m) => press_mouse(hwnd, m, hold_ms),
        None => send(&[key_input(c.vk, false)]).and_then(|_| {
            sleep(Duration::from_millis(hold_ms));
            send(&[key_input(c.vk, true)])
        }),
    };
    if !mods.is_empty() {
        sleep(Duration::from_millis(12));
        // Libera os modificadores mesmo se a ação falhou, para não deixá-los "presos".
        let ups: Vec<INPUT> = mods.iter().rev().map(|&m| key_input(m, true)).collect();
        send(&ups)?;
    }
    result
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
    let mut input: INPUT = unsafe { zeroed() };
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi.dwFlags = flags;
    input.Anonymous.mi.mouseData = data as u32;
    input
}

/// Centro da área cliente da janela, em coordenadas de tela.
fn client_center(hwnd: HWND) -> POINT {
    unsafe {
        let mut rc: RECT = zeroed();
        GetClientRect(hwnd, &mut rc);
        let mut pt = POINT { x: (rc.right - rc.left) / 2, y: (rc.bottom - rc.top) / 2 };
        ClientToScreen(hwnd, &mut pt);
        pt
    }
}

/// Clique/scroll real (SendInput) na posição atual do cursor. Se o cursor não estiver
/// sobre a janela alvo, ele vai ao centro dela só durante o envio e depois volta,
/// para que o clique nunca caia em outra janela.
fn press_mouse(hwnd: HWND, m: MouseAction, hold_ms: u64) -> Result<(), String> {
    unsafe {
        let mut original = POINT { x: 0, y: 0 };
        GetCursorPos(&mut original);
        let under = WindowFromPoint(original);
        let over_target = !under.is_null() && GetAncestor(under, GA_ROOT) == hwnd;
        if !over_target {
            let c = client_center(hwnd);
            SetCursorPos(c.x, c.y);
            sleep(Duration::from_millis(15));
        }
        let result = match m.button() {
            Some(b) => send(&[mouse_input(b.input_down, 0)]).and_then(|_| {
                sleep(Duration::from_millis(hold_ms));
                send(&[mouse_input(b.input_up, 0)])
            }),
            None => send(&[mouse_input(MOUSEEVENTF_WHEEL, m.wheel_delta())]),
        };
        if !over_target {
            sleep(Duration::from_millis(15));
            SetCursorPos(original.x, original.y);
        }
        result
    }
}

fn key_lparam(vk: u16, up: bool, alt_down: bool) -> isize {
    let mut l: isize = 1 | ((scan_code(vk) as isize) << 16);
    if is_extended(vk) {
        l |= 1 << 24;
    }
    if alt_down {
        l |= 1 << 29;
    }
    if up {
        l |= (1 << 30) | (1 << 31);
    }
    l
}

/// Controle com foco dentro da janela alvo (ex.: a caixa de texto), sem precisar ativá-la.
/// Só vale se pertencer à própria janela: apps como o Chrome rodam várias janelas na
/// mesma thread, e o foco da thread pode estar em outra delas.
fn focused_child(hwnd: HWND) -> HWND {
    unsafe {
        let tid = GetWindowThreadProcessId(hwnd, null_mut());
        let mut gti: GUITHREADINFO = zeroed();
        gti.cbSize = size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(tid, &mut gti) != 0
            && !gti.hwndFocus.is_null()
            && IsChild(hwnd, gti.hwndFocus) != 0
        {
            gti.hwndFocus
        } else {
            hwnd
        }
    }
}

fn sided(vk: u16) -> &'static [u16] {
    match vk {
        VK_CONTROL => &[VK_LCONTROL, VK_RCONTROL],
        VK_MENU => &[VK_LMENU, VK_RMENU],
        VK_SHIFT => &[VK_LSHIFT, VK_RSHIFT],
        VK_LWIN => &[VK_RWIN],
        _ => &[],
    }
}

/// Estado de teclado compartilhado com a thread da janela alvo.
///
/// Apps leem Ctrl/Alt/Shift com `GetKeyState` no momento em que processam a tecla,
/// e mensagens postadas não alteram esse estado. Anexando nossa fila de entrada à
/// da thread alvo (`AttachThreadInput`), o `SetKeyboardState` passa a valer para ela.
struct SharedKeyState {
    me: u32,
    target: u32,
    attached: bool,
    base: [u8; 256],
    original: [u8; 256],
}

impl SharedKeyState {
    fn attach(hwnd: HWND) -> Self {
        unsafe {
            // Garante que esta thread tenha fila de mensagens, exigida pelo AttachThreadInput.
            let mut msg: MSG = zeroed();
            PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
            let me = GetCurrentThreadId();
            let target = GetWindowThreadProcessId(hwnd, null_mut());
            let attached = target != 0 && target != me && AttachThreadInput(me, target, 1) != 0;
            let mut original = [0u8; 256];
            GetKeyboardState(original.as_mut_ptr());
            // Parte de um estado sem modificadores "presos", preservando Caps/Num Lock.
            let mut base = original;
            for vk in [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN] {
                for &k in std::iter::once(&vk).chain(sided(vk)) {
                    base[k as usize] &= !0x80;
                }
            }
            Self { me, target, attached, base, original }
        }
    }

    fn set(&self, pressed: &[u16]) {
        let mut state = self.base;
        for &vk in pressed {
            state[vk as usize] |= 0x80;
            if let Some(&left) = sided(vk).first() {
                state[left as usize] |= 0x80;
            }
        }
        unsafe { SetKeyboardState(state.as_ptr()) };
    }
}

impl Drop for SharedKeyState {
    fn drop(&mut self) {
        unsafe {
            SetKeyboardState(self.original.as_ptr());
            if self.attached {
                AttachThreadInput(self.me, self.target, 0);
            }
        }
    }
}

/// Espera a janela processar o que já foi postado. Mensagens enviadas (Send) são
/// atendidas antes das postadas na mesma volta do loop; a segunda chamada só retorna
/// depois que a mensagem postada foi despachada.
fn flush(hwnd: HWND) {
    for _ in 0..2 {
        send_sync(hwnd, WM_NULL, 0);
    }
}

/// Faz a janela "achar" que está ativa enquanto recebe as teclas, sem trazê-la
/// para frente. O Chrome/Electron (e outros apps) limpam o foco interno ao perder
/// a ativação e descartam teclas que chegam nesse estado.
struct FakeActivation(Option<HWND>);

impl FakeActivation {
    fn begin(hwnd: HWND) -> Self {
        // Minimizada, a ativação faria o app se restaurar sozinho (o Chrome faz isso);
        // janelas minimizadas passam antes pela restauração invisível (ghost.rs).
        if unsafe { GetForegroundWindow() == hwnd || IsIconic(hwnd) != 0 } {
            return Self(None);
        }
        send_sync(hwnd, WM_ACTIVATE, WA_ACTIVE as usize);
        Self(Some(hwnd))
    }
}

impl Drop for FakeActivation {
    fn drop(&mut self) {
        if let Some(hwnd) = self.0 {
            // Se o próprio atalho trouxe a janela para frente, ela está ativa de verdade.
            if unsafe { GetForegroundWindow() } != hwnd {
                send_sync(hwnd, WM_ACTIVATE, WA_INACTIVE as usize);
            }
        }
    }
}

fn send_sync(hwnd: HWND, msg: u32, wparam: usize) {
    let mut r = 0usize;
    unsafe { SendMessageTimeoutW(hwnd, msg, wparam, 0, SMTO_ABORTIFHUNG, 250, &mut r) };
}

/// Modo segundo plano: posta mensagens de teclado direto na janela, simulando
/// também o estado dos modificadores para que atalhos como Ctrl+J funcionem.
pub fn post_combo(hwnd: HWND, c: &Combo, hold_ms: u64) -> Result<(), String> {
    if let Some(m) = c.mouse {
        return post_mouse(hwnd, c, m, hold_ms);
    }
    let _active = FakeActivation::begin(hwnd);
    let target = focused_child(hwnd);
    let keys = SharedKeyState::attach(target);
    let mut pressed: Vec<u16> = Vec::with_capacity(5);

    let post = |vk: u16, up: bool, pressed: &[u16]| -> Result<(), String> {
        // Com Alt (e sem Ctrl) o Windows gera WM_SYS*; Ctrl+Alt gera WM_KEY* normal.
        let alt_held = pressed.contains(&VK_MENU);
        let sys = (alt_held || (up && vk == VK_MENU)) && !pressed.contains(&VK_CONTROL);
        let msg = match (up, sys) {
            (false, false) => WM_KEYDOWN,
            (false, true) => WM_SYSKEYDOWN,
            (true, false) => WM_KEYUP,
            (true, true) => WM_SYSKEYUP,
        };
        if unsafe { PostMessageW(target, msg, vk as usize, key_lparam(vk, up, alt_held)) } == 0 {
            return Err("Falha ao enviar mensagem para a janela".into());
        }
        flush(target);
        Ok(())
    };

    for vk in c.modifiers().into_iter().chain(std::iter::once(c.vk)) {
        pressed.push(vk);
        keys.set(&pressed);
        post(vk, false, &pressed)?;
    }
    sleep(Duration::from_millis(hold_ms));
    while let Some(vk) = pressed.pop() {
        keys.set(&pressed);
        post(vk, true, &pressed)?;
    }
    Ok(())
}

fn make_lparam(x: i32, y: i32) -> isize {
    ((x as u16 as u32) | ((y as u16 as u32) << 16)) as i32 as isize
}

/// Ponto (em coordenadas de tela) do clique em segundo plano: a posição do cursor
/// se ele estiver sobre a janela alvo, senão o centro da área cliente.
fn background_point(hwnd: HWND) -> POINT {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        let mut rc: RECT = zeroed();
        let inside = GetCursorPos(&mut pt) != 0
            && GetWindowRect(hwnd, &mut rc) != 0
            && (rc.left..rc.right).contains(&pt.x)
            && (rc.top..rc.bottom).contains(&pt.y);
        if inside { pt } else { client_center(hwnd) }
    }
}

/// Controle filho mais profundo sob o ponto (ex.: a área de conteúdo do Chrome),
/// que é quem de fato processa o mouse.
fn deepest_child_at(hwnd: HWND, screen: POINT) -> HWND {
    let mut current = hwnd;
    for _ in 0..32 {
        let mut pt = screen;
        unsafe { ScreenToClient(current, &mut pt) };
        let flags = CWP_SKIPINVISIBLE | CWP_SKIPDISABLED | CWP_SKIPTRANSPARENT;
        let child = unsafe { ChildWindowFromPointEx(current, pt, flags) };
        if child.is_null() || child == current {
            break;
        }
        current = child;
    }
    current
}

/// Modo segundo plano: posta as mensagens de mouse direto na janela, sem mover o cursor.
fn post_mouse(hwnd: HWND, c: &Combo, m: MouseAction, hold_ms: u64) -> Result<(), String> {
    let _active = FakeActivation::begin(hwnd);
    let screen = background_point(hwnd);
    let target = deepest_child_at(hwnd, screen);
    let mut client = screen;
    unsafe { ScreenToClient(target, &mut client) };
    let at_client = make_lparam(client.x, client.y);

    let mods = c.modifiers();
    let mut mk = 0usize;
    if c.ctrl {
        mk |= MK_CONTROL;
    }
    if c.shift {
        mk |= MK_SHIFT;
    }
    // Ctrl/Alt/Shift também ficam visíveis para GetKeyState durante o envio.
    let keys = SharedKeyState::attach(target);
    keys.set(&mods);

    let post = |msg: u32, wparam: usize, lparam: isize| -> Result<(), String> {
        if unsafe { PostMessageW(target, msg, wparam, lparam) } == 0 {
            return Err("Falha ao enviar mensagem para a janela".into());
        }
        flush(target);
        Ok(())
    };

    post(WM_MOUSEMOVE, mk, at_client)?;
    match m.button() {
        Some(b) => {
            let mut pressed = mods.clone();
            pressed.push(b.vk);
            keys.set(&pressed);
            post(b.msg_down, mk | b.mk, at_client)?;
            sleep(Duration::from_millis(hold_ms));
            keys.set(&mods);
            post(b.msg_up, mk, at_client)?;
        }
        None => {
            // WM_MOUSEWHEEL leva o delta na palavra alta e coordenadas de tela no lParam.
            let wparam = ((m.wheel_delta() as i16 as u16 as usize) << 16) | mk;
            post(WM_MOUSEWHEEL, wparam, make_lparam(screen.x, screen.y))?;
        }
    }
    Ok(())
}

fn wait_foreground(hwnd: HWND) -> bool {
    for _ in 0..25 {
        if unsafe { GetForegroundWindow() } == hwnd {
            return true;
        }
        sleep(Duration::from_millis(8));
    }
    false
}

/// Traz a janela para frente contornando o bloqueio de foco do Windows.
pub fn bring_to_front(hwnd: HWND) -> bool {
    unsafe {
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            return false;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() == hwnd {
            return true;
        }

        // 1) Tentativa direta.
        if SetForegroundWindow(hwnd) != 0 && wait_foreground(hwnd) {
            return true;
        }

        // 2) Compartilha a fila de entrada com a thread que está em primeiro plano.
        let mut msg: MSG = zeroed();
        PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
        let me = GetCurrentThreadId();
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(me, fg_thread, 0);
        }
        if wait_foreground(hwnd) {
            return true;
        }

        // 3) Com Alt pressionado o Windows libera a troca de foco.
        let _ = send(&[key_input(VK_MENU, false)]);
        SetForegroundWindow(hwnd);
        let _ = send(&[key_input(VK_MASK, false), key_input(VK_MASK, true), key_input(VK_MENU, true)]);
        wait_foreground(hwnd)
    }
}
