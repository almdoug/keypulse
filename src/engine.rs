//! Motor de execução: roda a sequência de teclas em ciclos numa thread própria
//! e informa o progresso para a interface via eventos.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow};

use crate::ghost::GhostRestore;
use crate::input::{self, Combo};
use crate::winlist::{self, to_hwnd};

#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SendMode {
    Focus,
    Background,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub hwnd: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub process: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    pub target: Target,
    pub steps: Vec<Combo>,
    pub interval_ms: u64,
    pub key_delay_ms: u64,
    pub hold_ms: u64,
    pub mode: SendMode,
    pub restore_focus: bool,
    pub send_on_start: bool,
    #[serde(default = "default_true")]
    pub keep_minimized: bool,
    pub repeat: Option<u32>,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CycleEvent {
    cycle: u64,
    sent: usize,
    ok: bool,
    next_in_ms: Option<u64>,
}

#[derive(Serialize, Clone)]
struct LogEvent<'a> {
    level: &'a str,
    message: String,
}

#[derive(Serialize, Clone)]
struct StatusEvent {
    running: bool,
}

struct Running {
    id: u64,
    stop: Sender<()>,
}

#[derive(Default)]
pub struct Engine {
    current: Mutex<Option<Running>>,
    next_id: AtomicU64,
}

impl Engine {
    fn stop_current(&self) -> bool {
        match self.current.lock().unwrap().take() {
            Some(r) => {
                let _ = r.stop.send(());
                true
            }
            None => false,
        }
    }
}

fn log(app: &AppHandle, level: &str, message: impl Into<String>) {
    let _ = app.emit("engine:log", LogEvent { level, message: message.into() });
}

/// Espera `dur`, retornando `true` se uma parada foi solicitada nesse meio tempo.
fn wait(rx: &Receiver<()>, dur: Duration) -> bool {
    match rx.recv_timeout(dur) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
        Err(RecvTimeoutError::Timeout) => false,
    }
}

/// Reencontra a janela alvo mesmo que ela tenha sido fechada e reaberta.
fn resolve_target(t: &Target) -> Option<HWND> {
    let h = to_hwnd(t.hwnd);
    let same_process = |hwnd: HWND| {
        t.process.is_empty() || winlist::window_process_name(hwnd).eq_ignore_ascii_case(&t.process)
    };
    if unsafe { IsWindow(h) } != 0 && same_process(h) {
        return Some(h);
    }
    let candidates: Vec<HWND> = winlist::enum_top_level()
        .into_iter()
        .filter(|&w| winlist::is_task_window(w) && same_process(w))
        .collect();
    candidates
        .iter()
        .copied()
        .find(|&w| winlist::window_title(w) == t.title)
        .or_else(|| (candidates.len() == 1).then(|| candidates[0]))
}

fn send_sequence(hwnd: HWND, cfg: &RunConfig, rx: &Receiver<()>) -> Result<(), String> {
    let previous = unsafe { GetForegroundWindow() };
    let focus = cfg.mode == SendMode::Focus;
    // Minimizada: restaura invisível durante a sequência e minimiza de novo ao sair
    // (o Drop roda depois da devolução de foco no fim da função).
    let ghost = if cfg.keep_minimized { GhostRestore::begin(hwnd) } else { None };

    for (i, step) in cfg.steps.iter().enumerate() {
        if i > 0 && wait(rx, Duration::from_millis(cfg.key_delay_ms)) {
            return Err("Interrompido".into());
        }
        if focus {
            if !input::bring_to_front(hwnd) {
                return Err("O Windows não permitiu focar a janela alvo".into());
            }
            if i == 0 {
                sleep(Duration::from_millis(60));
            }
            input::press_combo(step, cfg.hold_ms)?;
        } else {
            input::post_combo(hwnd, step, cfg.hold_ms)?;
        }
    }

    // No modo segundo plano alguns atalhos fazem o próprio app pular para frente
    // (ex.: Ctrl+J no Chrome); nesse caso o foco também é devolvido.
    let target_in_front = || unsafe { GetForegroundWindow() } == hwnd;
    // Uma janela que vai voltar a ser minimizada não pode ficar com o foco.
    if (cfg.restore_focus || ghost.is_some()) && !previous.is_null() && previous != hwnd {
        if focus {
            sleep(Duration::from_millis(40));
            input::bring_to_front(previous);
        } else {
            for _ in 0..10 {
                sleep(Duration::from_millis(30));
                if target_in_front() {
                    input::bring_to_front(previous);
                    break;
                }
            }
        }
    }
    if ghost.is_some() {
        // Dá tempo do app concluir atalhos assíncronos antes de minimizar.
        sleep(Duration::from_millis(120));
    }
    Ok(())
}

fn sequence_text(cfg: &RunConfig) -> String {
    cfg.steps.iter().map(Combo::describe).collect::<Vec<_>>().join("  →  ")
}

fn run(app: AppHandle, cfg: RunConfig, rx: Receiver<()>, id: u64) {
    let interval = Duration::from_millis(cfg.interval_ms);
    let _ = app.emit("engine:status", StatusEvent { running: true });
    log(&app, "info", format!("Iniciado · {} · a cada {}", sequence_text(&cfg), human(cfg.interval_ms)));

    let mut stopped = false;
    if !cfg.send_on_start {
        let _ = app.emit("engine:cycle", CycleEvent { cycle: 0, sent: 0, ok: false, next_in_ms: Some(cfg.interval_ms) });
        stopped = wait(&rx, interval);
    }

    let mut cycle: u64 = 0;
    while !stopped {
        let started = Instant::now();
        cycle += 1;

        let ok = match resolve_target(&cfg.target) {
            Some(hwnd) => match send_sequence(hwnd, &cfg, &rx) {
                Ok(()) => {
                    log(&app, "ok", format!("Ciclo #{cycle} enviado"));
                    true
                }
                Err(e) if e == "Interrompido" => break,
                Err(e) => {
                    log(&app, "error", format!("Ciclo #{cycle}: {e}"));
                    false
                }
            },
            None => {
                log(&app, "warn", format!("Ciclo #{cycle}: janela \"{}\" não encontrada", cfg.target.title));
                false
            }
        };

        let done = cfg.repeat.is_some_and(|n| cycle >= n as u64);
        let remaining = interval.saturating_sub(started.elapsed());
        let _ = app.emit(
            "engine:cycle",
            CycleEvent {
                cycle,
                sent: if ok { cfg.steps.len() } else { 0 },
                ok,
                next_in_ms: (!done).then(|| remaining.as_millis() as u64),
            },
        );
        if done {
            log(&app, "info", format!("Concluído após {cycle} ciclo(s)"));
            break;
        }
        stopped = wait(&rx, remaining);
    }

    // Só avisa a interface se esta execução ainda é a atual (evita corrida com um novo start).
    let engine = app.state::<Engine>();
    let mut current = engine.current.lock().unwrap();
    let is_current = match current.as_ref() {
        Some(r) if r.id == id => {
            *current = None;
            true
        }
        Some(_) => false,
        None => true,
    };
    drop(current);
    if is_current {
        let _ = app.emit("engine:status", StatusEvent { running: false });
    }
}

fn human(ms: u64) -> String {
    if ms >= 60_000 && ms % 1000 == 0 {
        let (m, s) = (ms / 60_000, (ms % 60_000) / 1000);
        if s == 0 { format!("{m} min") } else { format!("{m} min {s} s") }
    } else if ms % 1000 == 0 {
        format!("{} s", ms / 1000)
    } else {
        format!("{:.1} s", ms as f64 / 1000.0).replace('.', ",")
    }
}

fn validate(cfg: &RunConfig) -> Result<(), String> {
    if cfg.steps.is_empty() {
        return Err("Adicione pelo menos uma combinação de teclas".into());
    }
    if cfg.interval_ms < 100 {
        return Err("O intervalo mínimo é de 0,1 segundo".into());
    }
    Ok(())
}

#[tauri::command]
pub fn start(app: AppHandle, engine: State<Engine>, config: RunConfig) -> Result<(), String> {
    validate(&config)?;
    engine.stop_current();
    let (tx, rx) = mpsc::channel();
    let id = engine.next_id.fetch_add(1, Ordering::SeqCst) + 1;
    *engine.current.lock().unwrap() = Some(Running { id, stop: tx });
    thread::spawn(move || run(app, config, rx, id));
    Ok(())
}

#[tauri::command]
pub fn stop(app: AppHandle, engine: State<Engine>) {
    if engine.stop_current() {
        log(&app, "info", "Parado");
    }
    let _ = app.emit("engine:status", StatusEvent { running: false });
}

#[tauri::command]
pub fn is_running(engine: State<Engine>) -> bool {
    engine.current.lock().unwrap().is_some()
}

/// Executa a sequência uma única vez, sem iniciar o agendamento.
#[tauri::command]
pub async fn test_sequence(config: RunConfig) -> Result<String, String> {
    validate(&config)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_tx, rx) = mpsc::channel::<()>();
        let hwnd = resolve_target(&config.target)
            .ok_or_else(|| format!("Janela \"{}\" não encontrada", config.target.title))?;
        send_sequence(hwnd, &config, &rx)?;
        Ok(sequence_text(&config))
    })
    .await
    .map_err(|e| e.to_string())?
}
