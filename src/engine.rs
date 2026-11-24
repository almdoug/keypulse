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

/// Um passo da sequência. Com `interval_ms` ele roda num timer próprio,
/// independente da sequência principal.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    #[serde(flatten)]
    pub combo: Combo,
    #[serde(default)]
    pub interval_ms: Option<u64>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    pub target: Target,
    pub steps: Vec<Step>,
    pub interval_ms: u64,
    pub key_delay_ms: u64,
    pub hold_ms: u64,
    pub mode: SendMode,
    pub restore_focus: bool,
    pub send_on_start: bool,
    #[serde(default = "default_true")]
    pub keep_minimized: bool,
    pub repeat: Option<u32>,
    /// Rota de caminhada que roda em loop junto com a sequência (`None` = desligada).
    #[serde(default)]
    pub route: Option<Route>,
}

fn default_true() -> bool {
    true
}

/// Trecho da rota: ficar parado um tempo ou apertar uma tecla de movimento N vezes.
#[derive(Deserialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Leg {
    Wait { ms: u64 },
    Move { key: Combo, count: u32 },
}

impl Leg {
    fn describe(&self) -> String {
        match self {
            Leg::Wait { ms } => format!("parado {}", human(*ms)),
            Leg::Move { key, count } => format!("{} × {count}", key.describe()),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// Tempo entre um passo (sqm) e o próximo.
    pub step_ms: u64,
    pub legs: Vec<Leg>,
}

/// Onde a rota está: trecho atual, passos já dados nele e quando é a próxima ação.
struct RouteState {
    leg: usize,
    progress: u32,
    next: Instant,
}

impl RouteState {
    fn new(route: &Route, start: Instant) -> Self {
        let mut s = Self { leg: 0, progress: 0, next: start };
        s.enter(route, 0, start);
        s
    }

    /// Começa o trecho `leg` no instante `at`.
    fn enter(&mut self, route: &Route, leg: usize, at: Instant) {
        self.leg = leg;
        self.progress = 0;
        self.next = match route.legs[leg] {
            Leg::Wait { ms } => at + Duration::from_millis(ms),
            Leg::Move { .. } => at,
        };
    }

    fn next_leg(&self, route: &Route) -> usize {
        (self.leg + 1) % route.legs.len()
    }

    /// Ação vencida agora: a tecla a apertar ou `None` se era o fim de uma espera
    /// (nesse caso a rota já passou para o próximo trecho).
    fn take<'a>(&mut self, route: &'a Route) -> Option<&'a Combo> {
        match &route.legs[self.leg] {
            Leg::Wait { .. } => {
                let (leg, at) = (self.next_leg(route), self.next);
                self.enter(route, leg, at);
                None
            }
            Leg::Move { key, .. } => Some(key),
        }
    }

    /// Registra um passo dado (enviado ou não) que terminou em `after`.
    fn stepped(&mut self, route: &Route, after: Instant) {
        let step = Duration::from_millis(route.step_ms);
        self.progress += 1;
        let count = match route.legs[self.leg] {
            Leg::Move { count, .. } => count,
            Leg::Wait { .. } => 0,
        };
        if self.progress >= count {
            let leg = self.next_leg(route);
            self.enter(route, leg, after + step);
        } else {
            self.next = after + step;
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RouteEvent {
    leg: usize,
    progress: u32,
    /// Teclas efetivamente enviadas neste evento (para as estatísticas).
    sent: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CycleEvent {
    cycle: u64,
    sent: usize,
    ok: bool,
    next_in_ms: Option<u64>,
    /// Período completo do próximo timer a disparar (para o anel de progresso).
    next_period_ms: Option<u64>,
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

fn send_sequence(hwnd: HWND, cfg: &RunConfig, steps: &[&Combo], rx: &Receiver<()>) -> Result<(), String> {
    let previous = unsafe { GetForegroundWindow() };
    let focus = cfg.mode == SendMode::Focus;
    // Minimizada: restaura invisível durante a sequência e minimiza de novo ao sair
    // (o Drop roda depois da devolução de foco no fim da função).
    let ghost = if cfg.keep_minimized { GhostRestore::begin(hwnd) } else { None };

    for (i, &step) in steps.iter().enumerate() {
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
            input::press_combo(hwnd, step, cfg.hold_ms)?;
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

fn sequence_text(steps: &[&Combo]) -> String {
    steps.iter().map(|c| c.describe()).collect::<Vec<_>>().join("  →  ")
}

/// Um relógio de envio: a sequência principal (passos sem intervalo próprio)
/// ou um passo com intervalo próprio.
struct Timer {
    steps: Vec<usize>,
    period: Duration,
    next: Instant,
    runs: u64,
}

fn build_timers(cfg: &RunConfig, start: Instant) -> Vec<Timer> {
    let first = |period: Duration| if cfg.send_on_start { start } else { start + period };
    let mut timers = Vec::new();
    let main: Vec<usize> = (0..cfg.steps.len()).filter(|&i| cfg.steps[i].interval_ms.is_none()).collect();
    if !main.is_empty() {
        let period = Duration::from_millis(cfg.interval_ms);
        timers.push(Timer { steps: main, period, next: first(period), runs: 0 });
    }
    for (i, step) in cfg.steps.iter().enumerate() {
        if let Some(ms) = step.interval_ms {
            let period = Duration::from_millis(ms);
            timers.push(Timer { steps: vec![i], period, next: first(period), runs: 0 });
        }
    }
    timers
}

fn schedule_text(cfg: &RunConfig, timers: &[Timer]) -> String {
    timers
        .iter()
        .map(|t| {
            let steps: Vec<&Combo> = t.steps.iter().map(|&i| &cfg.steps[i].combo).collect();
            format!("{} · a cada {}", sequence_text(&steps), human(t.period.as_millis() as u64))
        })
        .collect::<Vec<_>>()
        .join("  |  ")
}

/// Próximo timer a disparar entre os que ainda não terminaram.
fn next_timer(timers: &[Timer], repeat: Option<u32>) -> Option<&Timer> {
    timers
        .iter()
        .filter(|t| repeat.map_or(true, |n| t.runs < n as u64))
        .min_by_key(|t| t.next)
}

fn run(app: AppHandle, cfg: RunConfig, rx: Receiver<()>, id: u64) {
    let _ = app.emit("engine:status", StatusEvent { running: true });
    let mut timers = build_timers(&cfg, Instant::now());
    let independent = timers.len() > 1;
    let route = cfg.route.as_ref().filter(|r| !r.legs.is_empty());
    let mut route_state = route.map(|r| RouteState::new(r, Instant::now()));
    let mut started_text = schedule_text(&cfg, &timers);
    if let Some(r) = route {
        let legs: Vec<String> = r.legs.iter().map(Leg::describe).collect();
        let route_text = format!("rota: {}", legs.join(" → "));
        started_text = [started_text, route_text].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  |  ");
    }
    log(&app, "info", format!("Iniciado · {started_text}"));
    if let (Some(r), Some(s)) = (route, &route_state) {
        log(&app, "info", format!("Rota: {}", r.legs[s.leg].describe()));
        let _ = app.emit("engine:route", RouteEvent { leg: s.leg, progress: 0, sent: 0 });
    }

    let mut cycle: u64 = 0;
    let mut route_failing = false;
    loop {
        let timer_next = next_timer(&timers, cfg.repeat).map(|t| (t.next, t.period));
        // Com "Limitado", a execução termina quando a sequência acaba; a rota sozinha
        // (sem nenhuma tecla na sequência) roda até ser parada.
        if timer_next.is_none() && !timers.is_empty() {
            log(&app, "info", format!("Concluído após {cycle} envio(s)"));
            break;
        }

        // A rota vence antes da sequência? Então a próxima ação é dela.
        if let (Some(r), Some(state)) = (route, route_state.as_mut()) {
            if timer_next.map_or(true, |(tn, _)| state.next < tn) {
                let now = Instant::now();
                if state.next > now && wait(&rx, state.next - now) {
                    break;
                }

                let leg_before = state.leg;
                let sent = match state.take(r) {
                    None => 0,
                    Some(key) => {
                        let label = format!("Rota ({})", key.describe());
                        let ok = match deliver(&app, &cfg, &[key], &label, &rx, route_failing) {
                            Delivery::Interrupted => break,
                            Delivery::Sent => true,
                            Delivery::Failed => false,
                        };
                        // Só avisa da primeira falha seguida, para não lotar o log a cada passo.
                        route_failing = !ok;
                        state.stepped(r, Instant::now());
                        ok as usize
                    }
                };
                if state.leg != leg_before {
                    log(&app, "info", format!("Rota: {}", r.legs[state.leg].describe()));
                }
                let _ = app.emit("engine:route", RouteEvent { leg: state.leg, progress: state.progress, sent });
                continue;
            }
        }

        let Some((due_at, period)) = timer_next else { continue };
        let now = Instant::now();
        if due_at > now {
            // Depois de cada envio a interface já recebe o tempo até o próximo;
            // só falta avisar antes do primeiro (sem "Enviar ao iniciar").
            if cycle == 0 {
                let _ = app.emit(
                    "engine:cycle",
                    CycleEvent {
                        cycle,
                        sent: 0,
                        ok: false,
                        next_in_ms: Some((due_at - now).as_millis() as u64),
                        next_period_ms: Some(period.as_millis() as u64),
                    },
                );
            }
            if wait(&rx, due_at - now) {
                break;
            }
        }

        // Todos os timers vencidos agora saem juntos, numa só sequência
        // (uma única troca de foco), na ordem da lista.
        let started = Instant::now();
        let tolerance = Duration::from_millis(30);
        let due: Vec<usize> = (0..timers.len())
            .filter(|&t| {
                cfg.repeat.map_or(true, |n| timers[t].runs < n as u64) && timers[t].next <= started + tolerance
            })
            .collect();
        let mut indices: Vec<usize> = due.iter().flat_map(|&t| timers[t].steps.iter().copied()).collect();
        indices.sort_unstable();
        let steps: Vec<&Combo> = indices.iter().map(|&i| &cfg.steps[i].combo).collect();
        cycle += 1;
        let label = if independent { format!("#{cycle} ({})", sequence_text(&steps)) } else { format!("Ciclo #{cycle}") };

        let ok = match deliver(&app, &cfg, &steps, &label, &rx, false) {
            Delivery::Interrupted => break,
            Delivery::Sent => {
                log(&app, "ok", format!("{label} enviado"));
                true
            }
            Delivery::Failed => false,
        };

        // O próximo envio conta a partir do início deste (como antes), sem
        // acumular atrasos nem disparar em rajada se o envio demorou.
        let after = Instant::now();
        for &t in &due {
            let timer = &mut timers[t];
            timer.runs += 1;
            timer.next = (started + timer.period).max(after);
        }

        let upcoming = next_timer(&timers, cfg.repeat);
        let _ = app.emit(
            "engine:cycle",
            CycleEvent {
                cycle,
                sent: if ok { steps.len() } else { 0 },
                ok,
                next_in_ms: upcoming.map(|t| t.next.saturating_duration_since(after).as_millis() as u64),
                next_period_ms: upcoming.map(|t| t.period.as_millis() as u64),
            },
        );
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

enum Delivery {
    Sent,
    Failed,
    Interrupted,
}

/// Localiza a janela e envia os passos, registrando falhas no log
/// (a menos que `quiet`, usado para não repetir a mesma falha a cada passo da rota).
fn deliver(app: &AppHandle, cfg: &RunConfig, steps: &[&Combo], label: &str, rx: &Receiver<()>, quiet: bool) -> Delivery {
    match resolve_target(&cfg.target) {
        Some(hwnd) => match send_sequence(hwnd, cfg, steps, rx) {
            Ok(()) => Delivery::Sent,
            Err(e) if e == "Interrompido" => Delivery::Interrupted,
            Err(e) => {
                if !quiet {
                    log(app, "error", format!("{label}: {e}"));
                }
                Delivery::Failed
            }
        },
        None => {
            if !quiet {
                log(app, "warn", format!("{label}: janela \"{}\" não encontrada", cfg.target.title));
            }
            Delivery::Failed
        }
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
    let has_route = cfg.route.as_ref().is_some_and(|r| !r.legs.is_empty());
    if cfg.steps.is_empty() && !has_route {
        return Err("Adicione pelo menos uma combinação de teclas".into());
    }
    if let Some(r) = cfg.route.as_ref().filter(|_| has_route) {
        if r.step_ms < 50 {
            return Err("Rota: o tempo por sqm mínimo é de 50 ms".into());
        }
        for leg in &r.legs {
            match leg {
                Leg::Wait { ms } if *ms < 100 => return Err("Rota: a parada mínima é de 0,1 segundo".into()),
                Leg::Move { count: 0, .. } => return Err("Rota: cada trecho precisa andar pelo menos 1 sqm".into()),
                _ => {}
            }
        }
    }
    let too_short = |ms: u64| ms < 100;
    if cfg.steps.iter().any(|s| s.interval_ms.is_none()) && too_short(cfg.interval_ms) {
        return Err("O intervalo mínimo é de 0,1 segundo".into());
    }
    if let Some(s) = cfg.steps.iter().find(|s| s.interval_ms.is_some_and(too_short)) {
        return Err(format!("{}: o intervalo mínimo é de 0,1 segundo", s.combo.describe()));
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
    if config.steps.is_empty() {
        return Err("Adicione pelo menos uma combinação de teclas para testar".into());
    }
    validate(&config)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_tx, rx) = mpsc::channel::<()>();
        let hwnd = resolve_target(&config.target)
            .ok_or_else(|| format!("Janela \"{}\" não encontrada", config.target.title))?;
        // O teste envia todos os passos uma vez, em ordem, ignorando intervalos próprios.
        let steps: Vec<&Combo> = config.steps.iter().map(|s| &s.combo).collect();
        send_sequence(hwnd, &config, &steps, &rx)?;
        Ok(sequence_text(&steps))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::MouseAction;

    fn config(send_on_start: bool) -> RunConfig {
        serde_json::from_str(&format!(
            r#"{{
                "target": {{ "hwnd": 1, "title": "x", "process": "x.exe" }},
                "steps": [
                    {{ "ctrl": true, "alt": true, "shift": false, "win": false, "vk": 112, "mouse": null, "label": "F1", "intervalMs": null }},
                    {{ "ctrl": true, "alt": false, "shift": false, "win": false, "vk": 0, "mouse": "wheelDown", "label": "Scroll ↓", "intervalMs": 2500 }},
                    {{ "ctrl": false, "alt": false, "shift": false, "win": false, "vk": 0, "mouse": "left", "label": "Clique esquerdo", "intervalMs": null }}
                ],
                "intervalMs": 5000, "keyDelayMs": 150, "holdMs": 40, "mode": "focus",
                "restoreFocus": true, "keepMinimized": true, "sendOnStart": {send_on_start}, "repeat": 2
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn parses_mouse_and_own_interval() {
        let cfg = config(true);
        assert_eq!(cfg.steps[1].combo.mouse, Some(MouseAction::WheelDown));
        assert_eq!(cfg.steps[1].interval_ms, Some(2500));
        assert_eq!(cfg.steps[2].combo.mouse, Some(MouseAction::Left));
        assert_eq!(cfg.steps[1].combo.describe(), "Ctrl+Scroll ↓");
        assert!(validate(&cfg).is_ok());
    }

    #[test]
    fn steps_without_interval_share_the_main_timer() {
        let start = Instant::now();
        let timers = build_timers(&config(true), start);
        assert_eq!(timers.len(), 2);
        assert_eq!(timers[0].steps, vec![0, 2]);
        assert_eq!(timers[0].period, Duration::from_millis(5000));
        assert_eq!(timers[1].steps, vec![1]);
        assert_eq!(timers[1].period, Duration::from_millis(2500));
        assert!(timers.iter().all(|t| t.next == start));

        let delayed = build_timers(&config(false), start);
        assert_eq!(delayed[1].next, start + Duration::from_millis(2500));
        assert_eq!(next_timer(&delayed, Some(2)).unwrap().steps, vec![1]);
    }

    #[test]
    fn rejects_too_short_own_interval() {
        let mut cfg = config(true);
        cfg.steps[1].interval_ms = Some(50);
        assert!(validate(&cfg).is_err());
    }

    fn route() -> Route {
        serde_json::from_str(
            r#"{ "stepMs": 300, "legs": [
                { "type": "wait", "ms": 10000 },
                { "type": "move", "key": { "vk": 39, "label": "→" }, "count": 3 },
                { "type": "wait", "ms": 10000 },
                { "type": "move", "key": { "vk": 37, "label": "←" }, "count": 2 }
            ] }"#,
        )
        .unwrap()
    }

    #[test]
    fn route_walks_legs_in_order_and_loops() {
        let r = route();
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let mut s = RouteState::new(&r, t0);
        assert_eq!((s.leg, s.next), (0, t0 + ms(10_000)));

        // Fim da espera: passa para "→ × 3" sem enviar nada.
        assert!(s.take(&r).is_none());
        assert_eq!((s.leg, s.next), (1, t0 + ms(10_000)));

        let mut now = s.next;
        let mut pressed = Vec::new();
        while s.leg == 1 {
            pressed.push(s.take(&r).unwrap().vk);
            s.stepped(&r, now);
            now = s.next;
        }
        assert_eq!(pressed, vec![39, 39, 39]);
        // Último passo + 300 ms para concluir + 10 s parado.
        assert_eq!(s.leg, 2);
        assert_eq!(s.next, t0 + ms(10_000 + 600 + 300 + 10_000));

        assert!(s.take(&r).is_none());
        for _ in 0..2 {
            assert_eq!(s.take(&r).unwrap().vk, 37);
            let at = s.next;
            s.stepped(&r, at);
        }
        // Volta para o começo da rota.
        assert_eq!(s.leg, 0);
    }

    #[test]
    fn route_alone_is_valid_and_bad_legs_are_rejected() {
        let mut cfg = config(true);
        cfg.steps.clear();
        assert!(validate(&cfg).is_err());
        cfg.route = Some(route());
        assert!(validate(&cfg).is_ok());

        let mut bad = route();
        bad.legs.push(serde_json::from_str(r#"{ "type": "move", "key": { "vk": 38 }, "count": 0 }"#).unwrap());
        cfg.route = Some(bad);
        assert!(validate(&cfg).is_err());
    }
}
