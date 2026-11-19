'use strict';

const TAURI = window.__TAURI__;
const invoke = (cmd, args) => TAURI.core.invoke(cmd, args);
const listen = (event, cb) => TAURI.event.listen(event, cb);
const appWindow = TAURI.window.getCurrentWindow();

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => Array.from(document.querySelectorAll(sel));
const esc = (s) =>
  String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

/* ---------- Ícones (traço estilo Lucide) ---------- */
const PATHS = {
  refresh: '<path d="M21 12a9 9 0 1 1-2.64-6.36L21 8"/><path d="M21 3v5h-5"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
  edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>',
  up: '<path d="m18 15-6-6-6 6"/>',
  down: '<path d="m6 9 6 6 6-6"/>',
  copy: '<rect x="9" y="9" width="12" height="12" rx="2.5"/><path d="M5 15V6a2 2 0 0 1 2-2h9"/>',
  trash: '<path d="M3 6h18"/><path d="M8 6V4h8v2"/><path d="m19 6-1 14H6L5 6"/><path d="M10 11v5M14 11v5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  play: '<path d="M7 4.8v14.4a.8.8 0 0 0 1.2.7l11.4-7.2a.8.8 0 0 0 0-1.4L8.2 4.1A.8.8 0 0 0 7 4.8Z" fill="currentColor"/>',
  stop: '<rect x="5.5" y="5.5" width="13" height="13" rx="2.5" fill="currentColor"/>',
  zap: '<path d="M13 2 4 14h7l-1 8 9-12h-7Z"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  target: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="5"/><circle cx="12" cy="12" r="1.2" fill="currentColor"/>',
  layers: '<path d="m12 3 9 5-9 5-9-5Z"/><path d="m3 13 9 5 9-5"/>',
  lock: '<rect x="4" y="11" width="16" height="10" rx="2.5"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>',
  infinity: '<path d="M6.5 8.5c-2 0-3.5 1.6-3.5 3.5s1.5 3.5 3.5 3.5c3.5 0 7.5-7 11-7 2 0 3.5 1.6 3.5 3.5s-1.5 3.5-3.5 3.5c-3.5 0-7.5-7-11-7Z"/>',
  hash: '<path d="M5 9h14M5 15h14M10 4 8 20M16 4l-2 16"/>',
  window: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M3 9h18"/><path d="M7 6.5h.01M10 6.5h.01"/>',
  eye: '<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>',
  shield: '<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6Z"/>',
  alert: '<path d="M12 3 2 20h20Z"/><path d="M12 10v4M12 17h.01"/>',
  dot: '<circle cx="12" cy="12" r="4" fill="currentColor"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>',
};
const icon = (name) =>
  `<svg class="ico" viewBox="0 0 24 24" aria-hidden="true">${PATHS[name] || ''}</svg>`;
const hydrateIcons = (root = document) =>
  root.querySelectorAll('[data-icon]').forEach((el) => (el.innerHTML = icon(el.dataset.icon)));

/* ---------- Teclas (códigos virtuais do Windows) ---------- */
const seq = (n, f) => Array.from({ length: n }, (_, i) => f(i));
const KEY_GROUPS = [
  { name: 'Função', keys: seq(24, (i) => ({ vk: 112 + i, label: `F${i + 1}` })) },
  { name: 'Letras', keys: seq(26, (i) => ({ vk: 65 + i, label: String.fromCharCode(65 + i) })) },
  { name: 'Números', keys: seq(10, (i) => ({ vk: 48 + i, label: String(i) })) },
  {
    name: 'Controle',
    keys: [
      { vk: 13, label: 'Enter' },
      { vk: 32, label: 'Espaço', alias: 'space barra' },
      { vk: 9, label: 'Tab' },
      { vk: 27, label: 'Esc', alias: 'escape' },
      { vk: 8, label: 'Backspace', alias: 'apagar' },
      { vk: 46, label: 'Delete', alias: 'del excluir' },
      { vk: 45, label: 'Insert', alias: 'ins' },
    ],
  },
  {
    name: 'Navegação',
    keys: [
      { vk: 38, label: '↑', alias: 'seta cima up' },
      { vk: 40, label: '↓', alias: 'seta baixo down' },
      { vk: 37, label: '←', alias: 'seta esquerda left' },
      { vk: 39, label: '→', alias: 'seta direita right' },
      { vk: 36, label: 'Home', alias: 'inicio' },
      { vk: 35, label: 'End', alias: 'fim' },
      { vk: 33, label: 'PgUp', alias: 'page up' },
      { vk: 34, label: 'PgDn', alias: 'page down' },
    ],
  },
  {
    name: 'Teclado numérico',
    keys: [
      ...seq(10, (i) => ({ vk: 96 + i, label: `Num ${i}`, alias: `numpad${i}` })),
      { vk: 106, label: 'Num *', alias: 'numpad multiplicar' },
      { vk: 107, label: 'Num +', alias: 'numpad somar' },
      { vk: 109, label: 'Num −', alias: 'numpad subtrair -' },
      { vk: 110, label: 'Num ,', alias: 'numpad decimal .' },
      { vk: 111, label: 'Num /', alias: 'numpad dividir' },
    ],
  },
  {
    name: 'Sistema',
    keys: [
      { vk: 44, label: 'PrtSc', alias: 'print screen' },
      { vk: 19, label: 'Pause', alias: 'break' },
      { vk: 20, label: 'Caps Lock' },
      { vk: 144, label: 'Num Lock' },
      { vk: 145, label: 'Scroll Lock' },
      { vk: 93, label: 'Menu', alias: 'contexto' },
    ],
  },
  {
    name: 'Mídia',
    keys: [
      { vk: 179, label: 'Play/Pause', alias: 'midia tocar pausar' },
      { vk: 176, label: 'Próxima', alias: 'next midia' },
      { vk: 177, label: 'Anterior', alias: 'prev previous midia' },
      { vk: 178, label: 'Stop', alias: 'parar midia' },
      { vk: 173, label: 'Mudo', alias: 'mute volume' },
      { vk: 174, label: 'Vol −', alias: 'volume baixar' },
      { vk: 175, label: 'Vol +', alias: 'volume aumentar' },
    ],
  },
  {
    name: 'Símbolos',
    keys: [
      { vk: 189, label: '-' }, { vk: 187, label: '=' }, { vk: 188, label: ',' }, { vk: 190, label: '.' },
      { vk: 186, label: ';' }, { vk: 191, label: '/' }, { vk: 192, label: "'" }, { vk: 219, label: '´' },
      { vk: 221, label: '[' }, { vk: 220, label: ']' }, { vk: 222, label: '~' }, { vk: 226, label: '\\' },
      { vk: 193, label: '/?', alias: 'abnt' },
    ],
  },
];
const VK_LABEL = new Map();
const SYMBOL_VKS = new Set(KEY_GROUPS.at(-1).keys.map((k) => k.vk));
KEY_GROUPS.forEach((g) =>
  g.keys.forEach((k) => {
    if (!VK_LABEL.has(k.vk)) VK_LABEL.set(k.vk, k.label);
    k.search = `${k.label} ${k.alias || ''} ${g.name}`.toLowerCase();
  }),
);
const MODIFIER_VKS = new Set([16, 17, 18, 91, 92, 160, 161, 162, 163, 164, 165]);
const MODS = [
  ['ctrl', 'Ctrl'],
  ['alt', 'Alt'],
  ['shift', 'Shift'],
  ['win', 'Win'],
];

const comboParts = (s) => [...MODS.filter(([k]) => s[k]).map(([, l]) => l), s.label || `VK ${s.vk}`];
const comboText = (s) => comboParts(s).join(' + ');
function keycaps(s, { pendingKey = false } = {}) {
  const mods = MODS.filter(([k]) => s[k]).map(([, l]) => `<kbd class="cap mod">${l}</kbd>`);
  const key =
    s.vk != null ? `<kbd class="cap">${esc(s.label || `VK ${s.vk}`)}</kbd>` : pendingKey ? '<kbd class="cap ghost">?</kbd>' : null;
  return [...mods, key].filter(Boolean).join('<span class="plus">+</span>');
}

/* ---------- Estado ---------- */
const DEFAULTS = {
  target: null,
  steps: [
    { ctrl: true, alt: true, shift: false, win: false, vk: 112, label: 'F1' },
    { ctrl: true, alt: false, shift: false, win: false, vk: 113, label: 'F2' },
  ],
  intervalSec: 5,
  keyDelayMs: 150,
  holdMs: 40,
  mode: 'focus',
  restoreFocus: true,
  keepMinimized: true,
  sendOnStart: true,
  repeatMode: 'infinite',
  repeatCount: 10,
};
const PRESETS = [1, 5, 10, 30, 60, 300];

let config = structuredClone(DEFAULTS);
let windows = [];
let windowsSig = '';
let targetWin = null;
let running = false;
let nextAt = null;
let cycleTotal = 0;
let finished = false;
let startedAt = 0;
let stats = { cycles: 0, keys: 0 };

let saveTimer = 0;
function save() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => invoke('save_config', { data: JSON.stringify(config) }).catch(() => {}), 300);
}

/* ---------- Janelas ---------- */
const winList = $('#winList');
const searchInput = $('#searchInput');

async function refreshWindows({ manual = false } = {}) {
  if (manual) {
    const btn = $('#refreshBtn');
    btn.classList.remove('spin');
    void btn.offsetWidth;
    btn.classList.add('spin');
  }
  try {
    windows = await invoke('list_windows');
  } catch (e) {
    return;
  }
  const sig = windows.map((w) => `${w.hwnd}|${w.title}|${w.minimized}|${w.icon ? 1 : 0}`).join('\n');
  syncTarget();
  if (sig !== windowsSig || manual) {
    windowsSig = sig;
    renderWindows();
  }
  renderTarget();
}

function syncTarget() {
  const t = config.target;
  if (!t) return (targetWin = null);
  let w = windows.find((w) => w.hwnd === t.hwnd);
  if (!w) w = windows.find((w) => w.process === t.process && w.title === t.title);
  if (w && (w.hwnd !== t.hwnd || w.title !== t.title)) {
    t.hwnd = w.hwnd;
    t.title = w.title;
    save();
  }
  targetWin = w || null;
}

const initial = (s) => (s || '?').replace(/[^a-z0-9]/gi, '').charAt(0) || '?';
const iconHtml = (w) => (w.icon ? `<img src="${w.icon}" alt="" draggable="false">` : `<span class="letter">${esc(initial(w.process))}</span>`);

function renderWindows() {
  const q = searchInput.value.trim().toLowerCase();
  const list = windows.filter((w) => !q || w.title.toLowerCase().includes(q) || w.process.toLowerCase().includes(q));
  $('#winCount').textContent = windows.length;
  if (!list.length) {
    winList.innerHTML = `<div class="empty">${icon(q ? 'search' : 'window')}<div>${
      q ? `Nenhuma janela corresponde a “${esc(q)}”` : 'Nenhuma janela aberta encontrada'
    }</div></div>`;
    return;
  }
  winList.innerHTML = list
    .map((w, i) => {
      const selected = config.target && config.target.hwnd === w.hwnd;
      const sub = [w.process, w.minimized ? 'minimizada' : ''].filter(Boolean).join(' · ');
      return `<button class="win-item${selected ? ' selected' : ''}" data-hwnd="${w.hwnd}" style="animation-delay:${Math.min(i, 12) * 18}ms" title="${esc(w.title)}">
        <span class="win-icon">${iconHtml(w)}</span>
        <span class="win-meta"><span class="win-title" style="display:block">${esc(w.title)}</span><span class="win-sub" style="display:block">${esc(sub)}</span></span>
        ${w.elevated ? `<span class="chip amber" title="Esta janela roda como administrador. Abra o KeyPulse como administrador para controlá-la.">${icon('shield')}Admin</span>` : ''}
        <span class="win-check">${icon('check')}</span>
      </button>`;
    })
    .join('');
}

winList.addEventListener('click', (e) => {
  const item = e.target.closest('.win-item');
  if (!item || running) return;
  const w = windows.find((w) => w.hwnd === Number(item.dataset.hwnd));
  if (!w) return;
  config.target = { hwnd: w.hwnd, title: w.title, process: w.process };
  targetWin = w;
  save();
  renderWindows();
  renderTarget();
  if (w.elevated) toast('Esta janela roda como administrador — execute o KeyPulse como administrador para controlá-la.', 'error', 5000);
});
searchInput.addEventListener('input', renderWindows);
$('#refreshBtn').addEventListener('click', () => refreshWindows({ manual: true }));

function renderTarget() {
  const card = $('#targetCard');
  const t = config.target;
  if (!t) {
    card.className = 'card target-card empty-target';
    card.innerHTML = `
      <div class="target-icon placeholder">${icon('target')}</div>
      <div class="target-meta">
        <div class="target-eyebrow">Janela alvo</div>
        <div class="target-title">Nenhuma janela selecionada</div>
        <div class="target-tags"><span class="chip muted">Escolha uma janela na lista ao lado</span></div>
      </div>`;
    return;
  }
  const w = targetWin;
  const status = !w
    ? `<span class="chip red">${icon('alert')}Não encontrada</span>`
    : w.elevated
      ? `<span class="chip amber">${icon('shield')}Requer admin</span>`
      : `<span class="chip green">${icon('dot')}Disponível</span>`;
  card.className = 'card target-card has-target';
  card.innerHTML = `
    <div class="target-icon">${w ? iconHtml(w) : icon('window')}</div>
    <div class="target-meta">
      <div class="target-eyebrow">Janela alvo</div>
      <div class="target-title" title="${esc(t.title)}">${esc(t.title)}</div>
      <div class="target-tags">${status}<span class="chip muted">${esc(t.process)}</span>${
        w?.minimized ? '<span class="chip muted">Minimizada</span>' : ''
      }</div>
    </div>
    <div class="target-actions">
      <button class="icon-btn" data-act="show" title="Trazer para frente" ${w ? '' : 'disabled'}>${icon('eye')}</button>
      <button class="icon-btn danger" data-act="clear" title="Remover seleção">${icon('x')}</button>
    </div>`;
}

$('#targetCard').addEventListener('click', async (e) => {
  const btn = e.target.closest('[data-act]');
  if (!btn || running) return;
  if (btn.dataset.act === 'clear') {
    config.target = null;
    targetWin = null;
    save();
    renderWindows();
    renderTarget();
  } else if (btn.dataset.act === 'show' && targetWin) {
    invoke('focus_window', { hwnd: targetWin.hwnd }).catch((err) => toast(String(err), 'error'));
  }
});

/* ---------- Sequência ---------- */
const stepsList = $('#stepsList');

function renderSteps() {
  if (!config.steps.length) {
    stepsList.innerHTML = `<li class="steps-empty">Nenhuma combinação ainda. Clique em <b>Adicionar</b> para gravar a primeira.</li>`;
    return;
  }
  const last = config.steps.length - 1;
  stepsList.innerHTML = config.steps
    .map(
      (s, i) => `
      ${i > 0 ? `<li class="step-gap">${icon('clock')}${config.keyDelayMs} ms</li>` : ''}
      <li class="step" data-i="${i}">
        <span class="step-idx">${String(i + 1).padStart(2, '0')}</span>
        <div class="combo">${keycaps(s)}</div>
        <div class="step-actions">
          <button class="icon-btn sm" data-act="edit" title="Editar">${icon('edit')}</button>
          <button class="icon-btn sm" data-act="up" title="Mover para cima" ${i === 0 ? 'disabled' : ''}>${icon('up')}</button>
          <button class="icon-btn sm" data-act="down" title="Mover para baixo" ${i === last ? 'disabled' : ''}>${icon('down')}</button>
          <button class="icon-btn sm" data-act="dup" title="Duplicar">${icon('copy')}</button>
          <button class="icon-btn sm danger" data-act="del" title="Remover">${icon('trash')}</button>
        </div>
      </li>`,
    )
    .join('');
}

stepsList.addEventListener('click', (e) => {
  const btn = e.target.closest('[data-act]');
  if (!btn) return;
  const i = Number(btn.closest('.step').dataset.i);
  const steps = config.steps;
  switch (btn.dataset.act) {
    case 'edit':
      return openEditor(i);
    case 'up':
      [steps[i - 1], steps[i]] = [steps[i], steps[i - 1]];
      break;
    case 'down':
      [steps[i + 1], steps[i]] = [steps[i], steps[i + 1]];
      break;
    case 'dup':
      steps.splice(i + 1, 0, { ...steps[i] });
      break;
    case 'del':
      steps.splice(i, 1);
      break;
  }
  save();
  renderSteps();
});
stepsList.addEventListener('dblclick', (e) => {
  const step = e.target.closest('.step');
  if (step && !e.target.closest('button')) openEditor(Number(step.dataset.i));
});
$('#addStepBtn').addEventListener('click', () => openEditor(-1));

/* ---------- Editor de combinação ---------- */
const modal = $('#modal');
const keySearch = $('#keySearch');
const capture = $('#capture');
let editingIndex = -1;
let draft = null;
const held = new Set();

function openEditor(index) {
  if (running) return;
  editingIndex = index;
  draft =
    index >= 0
      ? { ...config.steps[index] }
      : { ctrl: false, alt: false, shift: false, win: false, vk: null, label: '' };
  $('#modalTitle').textContent = index >= 0 ? `Editar combinação ${index + 1}` : 'Nova combinação';
  keySearch.value = '';
  held.clear();
  modal.hidden = false;
  renderPalette();
  renderDraft();
  capture.focus();
}

function closeEditor() {
  modal.hidden = true;
  draft = null;
}

function renderDraft() {
  const empty = !MODS.some(([k]) => draft[k]) && draft.vk == null;
  $('#captureKeys').innerHTML = empty
    ? '<kbd class="cap ghost">Ctrl</kbd><span class="plus">+</span><kbd class="cap ghost">Alt</kbd><span class="plus">+</span><kbd class="cap ghost">F1</kbd>'
    : keycaps(draft, { pendingKey: true });
  const recording = document.activeElement === capture;
  capture.classList.toggle('recording', recording);
  $('#captureHint').innerHTML = recording
    ? '<span class="rec-dot"></span>Gravando — pressione a combinação desejada'
    : 'Clique aqui para gravar pelo teclado';
  $$('#modToggles button').forEach((b) => b.classList.toggle('active', !!draft[b.dataset.mod]));
  $$('.kp-key').forEach((b) => b.classList.toggle('active', Number(b.dataset.vk) === draft.vk));
  $('#modalSave').disabled = draft.vk == null;
  $('#footPreview').textContent = draft.vk == null ? 'Escolha uma tecla principal' : comboText(draft);
}

function renderPalette() {
  const q = keySearch.value.trim().toLowerCase();
  const html = KEY_GROUPS.map((g) => {
    const keys = g.keys.filter((k) => !q || k.search.includes(q));
    if (!keys.length) return '';
    return `<div class="kp-group"><div class="kp-title">${g.name}</div><div class="kp-keys">${keys
      .map((k) => `<button class="kp-key" data-vk="${k.vk}" data-label="${esc(k.label)}" tabindex="-1">${esc(k.label)}</button>`)
      .join('')}</div></div>`;
  }).join('');
  $('#keyPalette').innerHTML = html || `<div class="empty">${icon('search')}<div>Nenhuma tecla encontrada</div></div>`;
  if (draft) renderDraft();
}

keySearch.addEventListener('input', renderPalette);
keySearch.addEventListener('focus', () => renderDraft());
capture.addEventListener('focus', () => renderDraft());
capture.addEventListener('blur', () => draft && renderDraft());
capture.addEventListener('click', () => capture.focus());

$('#keyPalette').addEventListener('click', (e) => {
  const b = e.target.closest('.kp-key');
  if (!b) return;
  draft.vk = Number(b.dataset.vk);
  draft.label = b.dataset.label;
  renderDraft();
});
$('#modToggles').addEventListener('click', (e) => {
  const b = e.target.closest('button');
  if (!b) return;
  draft[b.dataset.mod] = !draft[b.dataset.mod];
  renderDraft();
});
$('#modalCancel').addEventListener('click', closeEditor);
$('#modalClose').addEventListener('click', closeEditor);
modal.addEventListener('mousedown', (e) => e.target === modal && closeEditor());
$('#modalSave').addEventListener('click', () => {
  if (draft.vk == null) return;
  const step = { ctrl: !!draft.ctrl, alt: !!draft.alt, shift: !!draft.shift, win: !!draft.win, vk: draft.vk, label: draft.label };
  if (editingIndex >= 0) config.steps[editingIndex] = step;
  else config.steps.push(step);
  save();
  renderSteps();
  closeEditor();
});

function labelForEvent(e, vk) {
  if (VK_LABEL.has(vk) && !SYMBOL_VKS.has(vk)) return VK_LABEL.get(vk);
  if (e.key && e.key.length === 1 && e.key !== ' ') return e.key.toUpperCase();
  return VK_LABEL.get(vk) || (e.key && e.key !== 'Unidentified' ? e.key : `VK ${vk}`);
}

function onRecordKey(e) {
  const vk = e.keyCode;
  held.add(vk);
  const mods = {
    ctrl: e.ctrlKey,
    alt: e.altKey,
    shift: e.shiftKey,
    win: e.metaKey || held.has(91) || held.has(92),
  };
  if (MODIFIER_VKS.has(vk)) {
    if (vk === 17 || vk === 162 || vk === 163) mods.ctrl = true;
    if (vk === 18 || vk === 164 || vk === 165) mods.alt = true;
    if (vk === 16 || vk === 160 || vk === 161) mods.shift = true;
    if (vk === 91 || vk === 92) mods.win = true;
    draft = { ...mods, vk: null, label: '' };
  } else {
    draft = { ...mods, vk, label: labelForEvent(e, vk) };
  }
  renderDraft();
}

/* ---------- Temporização e opções ---------- */
const intervalInput = $('#intervalInput');
const fmtNum = (n, d = 1) => n.toLocaleString('pt-BR', { minimumFractionDigits: 0, maximumFractionDigits: d });
const fmtPreset = (s) => (s >= 60 ? `${s / 60}min` : `${s}s`);

function renderTiming() {
  intervalInput.value = config.intervalSec;
  const perMin = 60 / config.intervalSec;
  $('#rateHint').textContent = perMin >= 1 ? `≈ ${fmtNum(perMin)} ciclos/min` : `1 ciclo a cada ${fmtNum(config.intervalSec / 60)} min`;
  $('#presets').innerHTML = PRESETS.map(
    (s) => `<button class="preset${config.intervalSec === s ? ' active' : ''}" data-s="${s}">${fmtPreset(s)}</button>`,
  ).join('');
  setRange($('#keyDelay'), config.keyDelayMs);
  setRange($('#hold'), config.holdMs);
  $('#keyDelayOut').textContent = `${config.keyDelayMs} ms`;
  $('#holdOut').textContent = `${config.holdMs} ms`;
  $$('#repeatSeg button').forEach((b) => b.classList.toggle('active', b.dataset.v === config.repeatMode));
  $('#repeatCount').value = config.repeatCount;
  $('#repeatCountWrap').classList.toggle('off', config.repeatMode !== 'count');
  if (!running) renderRingIdle();
}

function setRange(el, v) {
  el.value = v;
  el.style.setProperty('--p', `${((v - el.min) / (el.max - el.min)) * 100}%`);
}

function setInterval_(sec) {
  const v = Math.max(0.1, Math.round(sec * 10) / 10);
  if (!Number.isFinite(v)) return;
  config.intervalSec = v;
  save();
  renderTiming();
}
intervalInput.addEventListener('change', () => setInterval_(parseFloat(String(intervalInput.value).replace(',', '.'))));
$$('.nudge').forEach((b) =>
  b.addEventListener('click', () => {
    const step = config.intervalSec < 2 ? 0.1 : config.intervalSec < 10 ? 0.5 : config.intervalSec < 60 ? 1 : 5;
    setInterval_(config.intervalSec + Number(b.dataset.nudge) * step);
  }),
);
$('#presets').addEventListener('click', (e) => {
  const b = e.target.closest('.preset');
  if (b) setInterval_(Number(b.dataset.s));
});
$('#keyDelay').addEventListener('input', (e) => {
  config.keyDelayMs = Number(e.target.value);
  save();
  renderTiming();
  renderSteps();
});
$('#hold').addEventListener('input', (e) => {
  config.holdMs = Number(e.target.value);
  save();
  renderTiming();
});
$('#repeatSeg').addEventListener('click', (e) => {
  const b = e.target.closest('button');
  if (!b) return;
  config.repeatMode = b.dataset.v;
  save();
  renderTiming();
});
$('#repeatCount').addEventListener('change', (e) => {
  config.repeatCount = Math.max(1, Math.round(Number(e.target.value) || 1));
  save();
  renderTiming();
});

function renderOptions() {
  $$('#modeCards .mode').forEach((b) => b.classList.toggle('active', b.dataset.v === config.mode));
  $('#restoreFocus').checked = config.restoreFocus;
  $('#sendOnStart').checked = config.sendOnStart;
  $('#keepMinimized').checked = config.keepMinimized;
  $('#restoreHint').textContent =
    config.mode === 'focus'
      ? 'Devolve o foco à janela anterior depois de cada envio'
      : 'Se o app alvo pular para frente sozinho, devolve o foco para você';
}
$('#modeCards').addEventListener('click', (e) => {
  const b = e.target.closest('.mode');
  if (!b) return;
  config.mode = b.dataset.v;
  save();
  renderOptions();
});
$('#restoreFocus').addEventListener('change', (e) => {
  config.restoreFocus = e.target.checked;
  save();
});
$('#keepMinimized').addEventListener('change', (e) => {
  config.keepMinimized = e.target.checked;
  save();
});
$('#sendOnStart').addEventListener('change', (e) => {
  config.sendOnStart = e.target.checked;
  save();
});

/* ---------- Execução ---------- */
const RING_LEN = 2 * Math.PI * 88;
const ringProg = $('#ringProg');
const ringValue = $('#ringValue');
const ringLabel = $('#ringLabel');

function fmtTime(ms) {
  if (ms >= 60000) {
    const s = Math.ceil(ms / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}<small>min</small>`;
  }
  return `${fmtNum(ms / 1000, 1).replace(/^(\d+)$/, '$1,0')}<small>s</small>`;
}
const setRing = (frac) => (ringProg.style.strokeDashoffset = String(RING_LEN * (1 - Math.max(0, Math.min(1, frac)))));

function renderRingIdle() {
  setRing(1);
  ringValue.innerHTML = fmtTime(config.intervalSec * 1000);
  ringLabel.textContent = finished ? 'concluído' : 'intervalo';
}

function tick() {
  if (running) {
    if (nextAt != null) {
      const rem = Math.max(0, nextAt - performance.now());
      setRing(cycleTotal ? rem / cycleTotal : 0);
      ringValue.innerHTML = fmtTime(rem);
      ringLabel.textContent = rem > 0 ? 'próximo envio' : 'enviando…';
    } else {
      setRing(0);
      ringValue.innerHTML = '…';
      ringLabel.textContent = 'enviando';
    }
    const up = Math.floor((Date.now() - startedAt) / 1000);
    const h = Math.floor(up / 3600);
    const mm = String(Math.floor((up % 3600) / 60)).padStart(2, '0');
    const ss = String(up % 60).padStart(2, '0');
    $('#statUptime').textContent = h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
  }
  requestAnimationFrame(tick);
}

function renderRunState() {
  document.body.classList.toggle('is-running', running);
  $('#statusPill').dataset.state = running ? 'running' : 'idle';
  $('#statusText').textContent = running ? 'Em execução' : 'Pronto';
  $('#runBtn').innerHTML = running
    ? `${icon('stop')}<span>Parar</span>`
    : `${icon('play')}<span>Iniciar</span>`;
  if (!running) renderRingIdle();
}

function setRunning(value) {
  if (running === value) return;
  running = value;
  if (running) {
    finished = false;
    startedAt = Date.now();
    stats = { cycles: 0, keys: 0 };
    nextAt = null;
    renderStats();
  }
  renderRunState();
}

function renderStats() {
  $('#statCycles').textContent = stats.cycles.toLocaleString('pt-BR');
  $('#statKeys').textContent = stats.keys.toLocaleString('pt-BR');
  if (!running && !startedAt) $('#statUptime').textContent = '00:00';
}

function buildRunConfig() {
  return {
    target: config.target,
    steps: config.steps.map(({ ctrl, alt, shift, win, vk, label }) => ({ ctrl, alt, shift, win, vk, label })),
    intervalMs: Math.round(config.intervalSec * 1000),
    keyDelayMs: config.keyDelayMs,
    holdMs: config.holdMs,
    mode: config.mode,
    restoreFocus: config.restoreFocus,
    keepMinimized: config.keepMinimized,
    sendOnStart: config.sendOnStart,
    repeat: config.repeatMode === 'count' ? config.repeatCount : null,
  };
}

function validate() {
  if (!config.target) return 'Selecione a janela alvo na lista à esquerda';
  if (!config.steps.length) return 'Adicione pelo menos uma combinação de teclas';
  return null;
}

async function startRun() {
  const err = validate();
  if (err) return toast(err, 'error');
  try {
    await invoke('start', { config: buildRunConfig() });
    setRunning(true);
  } catch (e) {
    toast(String(e), 'error');
    addLog('error', String(e));
  }
}

async function stopRun() {
  try {
    await invoke('stop');
  } finally {
    setRunning(false);
  }
}

const toggleRun = () => (running ? stopRun() : startRun());
$('#runBtn').addEventListener('click', toggleRun);

$('#testBtn').addEventListener('click', async () => {
  const err = validate();
  if (err) return toast(err, 'error');
  const btn = $('#testBtn');
  btn.classList.add('busy');
  try {
    const text = await invoke('test_sequence', { config: buildRunConfig() });
    addLog('ok', `Teste enviado: ${text}`);
    toast('Sequência enviada com sucesso', 'ok');
  } catch (e) {
    addLog('error', `Teste falhou: ${e}`);
    toast(String(e), 'error');
  } finally {
    btn.classList.remove('busy');
  }
});

/* ---------- Log e toasts ---------- */
const logEl = $('#log');
function addLog(level, message) {
  logEl.querySelector('.log-empty')?.remove();
  const li = document.createElement('li');
  li.className = level;
  const t = new Date().toLocaleTimeString('pt-BR');
  li.innerHTML = `<time>${t}</time><span class="dot"></span><span class="msg">${esc(message)}</span>`;
  logEl.prepend(li);
  while (logEl.children.length > 150) logEl.lastChild.remove();
}
function clearLog() {
  logEl.innerHTML = '<li class="log-empty">Nenhuma atividade ainda</li>';
}
$('#clearLog').addEventListener('click', clearLog);

function toast(message, kind = 'info', ms = 3200) {
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.innerHTML = `${icon(kind === 'error' ? 'alert' : kind === 'ok' ? 'check' : 'info')}<span>${esc(message)}</span>`;
  $('#toasts').append(el);
  setTimeout(() => {
    el.classList.add('out');
    setTimeout(() => el.remove(), 260);
  }, ms);
}

/* ---------- Eventos do backend ---------- */
listen('engine:status', ({ payload }) => setRunning(payload.running));
listen('engine:log', ({ payload }) => addLog(payload.level, payload.message));
listen('engine:cycle', ({ payload }) => {
  if (payload.cycle > 0) {
    stats.cycles = payload.cycle;
    stats.keys += payload.sent;
    renderStats();
    const wrap = $('#ringWrap');
    wrap.classList.remove('flash');
    void wrap.offsetWidth;
    wrap.classList.add('flash');
  }
  if (payload.nextInMs == null) {
    nextAt = null;
    finished = true;
  } else {
    cycleTotal = Math.round(config.intervalSec * 1000);
    nextAt = performance.now() + payload.nextInMs;
  }
});
listen('hotkey:toggle', () => {
  if (!modal.hidden) return;
  toggleRun();
});

/* ---------- Teclado e janela ---------- */
document.addEventListener('keydown', (e) => {
  if (!modal.hidden) {
    if (document.activeElement === keySearch) {
      if (e.key === 'Escape') {
        e.preventDefault();
        capture.focus();
      }
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    onRecordKey(e);
    return;
  }
  const k = e.key.toLowerCase();
  if ((e.ctrlKey && (k === 'k' || k === 'f')) && !running) {
    e.preventDefault();
    searchInput.focus();
    searchInput.select();
    return;
  }
  // Bloqueia atalhos de navegador (recarregar, imprimir, etc.).
  if (e.key === 'F5' || e.key === 'F7' || (e.ctrlKey && ['r', 'p', 'u', 'j', 'g', 's', 'o'].includes(k))) e.preventDefault();
  if (e.key === 'Escape' && document.activeElement === searchInput) {
    searchInput.value = '';
    renderWindows();
    searchInput.blur();
  }
});
document.addEventListener('keyup', (e) => held.delete(e.keyCode), true);
document.addEventListener('contextmenu', (e) => {
  if (!e.target.closest('input')) e.preventDefault();
});

$('#wcMin').addEventListener('click', () => appWindow.minimize());
$('#wcMax').addEventListener('click', () => appWindow.toggleMaximize());
$('#wcClose').addEventListener('click', () => appWindow.close());

/* ---------- Inicialização ---------- */
async function init() {
  hydrateIcons();
  $('#refreshBtn').innerHTML = icon('refresh');
  $('#modalClose').innerHTML = icon('x');
  try {
    const raw = await invoke('load_config');
    if (raw) {
      const saved = JSON.parse(raw);
      config = { ...structuredClone(DEFAULTS), ...saved };
      if (!Array.isArray(config.steps)) config.steps = structuredClone(DEFAULTS.steps);
    }
  } catch (_) {
    /* usa os padrões */
  }
  try {
    running = await invoke('is_running');
  } catch (_) {}
  renderSteps();
  renderTiming();
  renderOptions();
  renderTarget();
  renderRunState();
  renderStats();
  clearLog();
  await refreshWindows();
  setInterval(() => {
    if (!running && document.visibilityState === 'visible') refreshWindows();
  }, 2500);
  requestAnimationFrame(tick);
  if (await invoke('is_admin').catch(() => false)) addLog('info', 'Executando como administrador');
}

init();
