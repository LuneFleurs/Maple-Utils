import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./styles.css";
import appIconUrl from "./assets/app-icon.png";

const app = document.querySelector("#app");
const THEME_STORAGE_KEY = "maple-utils-theme";
const PANEL_STORAGE_KEY = "maple-utils-panel";
const FOCUS_GUARD_POLL_OPTIONS = [75, 30, 16];
const PICKER_MODE = new URLSearchParams(window.location.search).get("picker");

function loadTheme() {
  const saved = localStorage.getItem(THEME_STORAGE_KEY);
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function loadPanel() {
  return localStorage.getItem(PANEL_STORAGE_KEY) === "filter" ? "filter" : "focus";
}

const state = {
  snapshot: null,
  busy: false,
  selectedGameHwnd: null,
  selectedFocusHwnds: [],
  theme: loadTheme(),
  panel: loadPanel(),
  panelFrom: loadPanel(),
  panelMotion: false,
  helpOpen: false,
};

function applyTheme() {
  document.documentElement.dataset.theme = state.theme;
}

applyTheme();
if (PICKER_MODE) {
  document.documentElement.dataset.picker = PICKER_MODE;
}

const escapeHtml = (value) =>
  String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");

const LIQUID_TAB_WIDTH = 118;
const LIQUID_TAB_HEIGHT = 38;
const LIQUID_TAB_PADDING = 4;
const LIQUID_TAB_GAP = 3;
let liquidTabMap = null;

function smoothStep(a, b, t) {
  const clamped = Math.max(0, Math.min(1, (t - a) / (b - a)));
  return clamped * clamped * (3 - 2 * clamped);
}

function vectorLength(x, y) {
  return Math.sqrt(x * x + y * y);
}

function roundedRectSDF(x, y, width, height, radius) {
  const qx = Math.abs(x) - width + radius;
  const qy = Math.abs(y) - height + radius;
  return Math.min(Math.max(qx, qy), 0) + vectorLength(Math.max(qx, 0), Math.max(qy, 0)) - radius;
}

function texture(x, y) {
  return { x, y };
}

function liquidTabFragment(uv) {
  const px = (uv.x - 0.5) * LIQUID_TAB_WIDTH;
  const py = (uv.y - 0.5) * LIQUID_TAB_HEIGHT;
  const distanceToEdge = roundedRectSDF(
    px,
    py,
    LIQUID_TAB_WIDTH / 2 - 2,
    LIQUID_TAB_HEIGHT / 2 - 2,
    LIQUID_TAB_HEIGHT / 2 - 2,
  );
  const edgePull = smoothStep(-13, -1.2, distanceToEdge);
  const centerFalloff = 1 - smoothStep(0, 0.52, vectorLength(uv.x - 0.5, (uv.y - 0.5) * 2.1));
  const refraction = edgePull * (0.055 + centerFalloff * 0.012);

  return texture(
    uv.x - (uv.x - 0.5) * refraction,
    uv.y - (uv.y - 0.5) * refraction * 1.55,
  );
}

function createLiquidTabMap() {
  if (liquidTabMap) return liquidTabMap;

  const dpi = Math.max(1, Math.min(2, Math.round(window.devicePixelRatio || 1)));
  const canvas = document.createElement("canvas");
  const width = LIQUID_TAB_WIDTH * dpi;
  const height = LIQUID_TAB_HEIGHT * dpi;
  canvas.width = width;
  canvas.height = height;

  const context = canvas.getContext("2d");
  if (!context) {
    liquidTabMap = { url: "", scale: 0 };
    return liquidTabMap;
  }

  const data = new Uint8ClampedArray(width * height * 4);
  const rawValues = [];
  let maxDelta = 0;

  for (let i = 0; i < data.length; i += 4) {
    const index = i / 4;
    const x = index % width;
    const y = Math.floor(index / width);
    const cssX = x / dpi;
    const cssY = y / dpi;
    const pos = liquidTabFragment({ x: (x + 0.5) / width, y: (y + 0.5) / height });
    const dx = pos.x * LIQUID_TAB_WIDTH - cssX;
    const dy = pos.y * LIQUID_TAB_HEIGHT - cssY;
    maxDelta = Math.max(maxDelta, Math.abs(dx), Math.abs(dy));
    rawValues.push(dx, dy);
  }

  const scale = Math.max(maxDelta * 2, 1);
  let rawIndex = 0;
  for (let i = 0; i < data.length; i += 4) {
    data[i] = Math.max(0, Math.min(255, (rawValues[rawIndex++] / scale + 0.5) * 255));
    data[i + 1] = Math.max(0, Math.min(255, (rawValues[rawIndex++] / scale + 0.5) * 255));
    data[i + 2] = 128;
    data[i + 3] = 255;
  }

  context.putImageData(new ImageData(data, width, height), 0, 0);
  liquidTabMap = { url: canvas.toDataURL(), scale };
  return liquidTabMap;
}

function installLiquidTabFilter() {
  const image = document.getElementById("liquid-tab-map");
  const displacement = document.getElementById("liquid-tab-displacement");
  if (!image || !displacement) return;

  const map = createLiquidTabMap();
  if (!map.url) return;

  image.setAttribute("href", map.url);
  image.setAttributeNS("http://www.w3.org/1999/xlink", "href", map.url);
  displacement.setAttribute("scale", map.scale.toFixed(3));
}

async function call(command, payload = {}, options = {}) {
  if (state.busy) return;
  const renderOptions = { preserveScroll: options.preserveScroll ?? true };
  state.busy = true;
  render(renderOptions);
  try {
    state.snapshot = await invoke(command, payload);
    setStatus("적용했습니다.");
  } catch (error) {
    setStatus(String(error), true);
  } finally {
    state.busy = false;
    render(renderOptions);
    pulseGameForeground();
  }
}

function themeIcon() {
  if (state.theme === "dark") {
    return `
      <svg class="theme-icon" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M20.2 14.7A7.4 7.4 0 0 1 9.3 3.8a8.5 8.5 0 1 0 10.9 10.9Z" />
      </svg>
    `;
  }

  return `
    <svg class="theme-icon" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="4.2" />
      <path d="M12 2.5v2.1M12 19.4v2.1M4.6 4.6l1.5 1.5M17.9 17.9l1.5 1.5M2.5 12h2.1M19.4 12h2.1M4.6 19.4l1.5-1.5M17.9 6.1l1.5-1.5" />
    </svg>
  `;
}

function titleModeButton(settings) {
  if (!settings) return "";

  return `
    <button class="title-mode ${settings.settings_mode ? "active" : ""}" data-toggle="settings-mode" aria-pressed="${settings.settings_mode}" title="설정 모드">
      <span>설정 모드</span>
      <span class="title-mode-dot" aria-hidden="true"></span>
    </button>
  `;
}

function helpSheet() {
  if (!state.helpOpen) return "";

  return `
    <div class="help-overlay" data-help-backdrop role="presentation">
      <section class="help-sheet" role="dialog" aria-modal="true" aria-labelledby="help-title">
        <header class="help-head">
          <div>
            <h2 id="help-title">메이플 유틸 설명서</h2>
            <p>한번 읽고 사용해주세요.</p>
          </div>
          <button class="help-close" data-help-close aria-label="설명서 닫기">x</button>
        </header>

        <div class="help-body">
          <section class="help-section">
            <h3>기본 흐름</h3>
            <ol>
              <li>상단의 설정 모드를 켭니다.</li>
              <li>키 입력 받을 창 선택을 누르고 메이플 창 위에서 클릭합니다.</li>
              <li>포커스 유지와 게임 항상 위를 원하는 상태로 켭니다.</li>
              <li>설정 모드를 끄고 다른 프로그램을 클릭해서 확인합니다.</li>
            </ol>
          </section>

          <section class="help-section">
            <h3>포커스 설정</h3>
            <p>포커스 유지는 다른 창을 클릭해도 지정한 게임 창으로 키 입력이 계속 가도록 돕는 옵션입니다.</p>
            <p>포커스 차단 대상은 키 입력을 받지 않게 할 창 목록입니다. 게임 창은 대상에서 자동으로 제외됩니다.</p>
          </section>

          <section class="help-section">
            <h3>필터키</h3>
            <p>기본값은 Wait 0 / Delay 150 / Repeat 1 / Flags 27입니다.</p>
            <p>프리셋 저장은 현재 입력칸 값을 이름을 붙여 저장합니다. 저장한 프리셋을 누르면 입력칸에 다시 불러옵니다.</p>
            <p>필터키 적용은 Windows 필터키 값을 바꾸고, 백업 복원은 적용 전 저장해둔 값으로 되돌립니다.</p>
          </section>

          <section class="help-section">
            <h3>주의</h3>
            <p>메이플이 관리자 권한으로 실행 중이면 이 앱도 관리자 권한으로 실행해야 창 선택과 포커스 제어가 정상 동작합니다.</p>
          </section>
        </div>
      </section>
    </div>
  `;
}

function frame(content, titleStatus = "") {
  const panelFrom = state.panelFrom === "filter" ? "filter" : "focus";
  const panelTo = state.panel === "filter" ? "filter" : "focus";
  const filterTabX = LIQUID_TAB_PADDING + LIQUID_TAB_WIDTH + LIQUID_TAB_GAP;
  const liquidFromX = panelFrom === "filter" ? filterTabX : LIQUID_TAB_PADDING;
  const liquidToX = panelTo === "filter" ? filterTabX : LIQUID_TAB_PADDING;
  const liquidDirection = liquidToX > liquidFromX ? 1 : -1;
  const liquidNudgeX = liquidFromX === liquidToX ? liquidToX : liquidToX + liquidDirection * 3;
  const liquidOrigin = liquidFromX === liquidToX ? "center" : liquidDirection > 0 ? "left" : "right";

  return `
    <div class="app-frame">
      <svg class="liquid-filter-defs" aria-hidden="true" focusable="false">
        <defs>
          <filter id="liquid-glass-refraction" x="-18%" y="-18%" width="136%" height="136%" color-interpolation-filters="sRGB">
            <feTurbulence type="fractalNoise" baseFrequency="0.012 0.018" numOctaves="1" seed="9" result="liquidNoise" />
            <feGaussianBlur in="liquidNoise" stdDeviation="0.65" result="softNoise" />
            <feDisplacementMap in="SourceGraphic" in2="softNoise" scale="9" xChannelSelector="R" yChannelSelector="G" result="refracted" />
            <feSpecularLighting in="softNoise" surfaceScale="8" specularConstant="0.42" specularExponent="28" lighting-color="#ffffff" result="specular">
              <feDistantLight azimuth="-60" elevation="62" />
            </feSpecularLighting>
            <feComposite in="specular" in2="refracted" operator="in" result="specularClip" />
            <feBlend in="refracted" in2="specularClip" mode="screen" />
          </filter>
          <filter id="liquid-glass-surface" x="-12%" y="-12%" width="124%" height="124%" color-interpolation-filters="sRGB">
            <feTurbulence type="fractalNoise" baseFrequency="0.018 0.026" numOctaves="1" seed="14" result="surfaceNoise" />
            <feGaussianBlur in="surfaceNoise" stdDeviation="0.45" result="surfaceSoftNoise" />
            <feDisplacementMap in="SourceGraphic" in2="surfaceSoftNoise" scale="3.5" xChannelSelector="R" yChannelSelector="G" result="surface" />
            <feSpecularLighting in="surfaceSoftNoise" surfaceScale="6" specularConstant="0.28" specularExponent="24" lighting-color="#ffffff" result="surfaceLight">
              <feDistantLight azimuth="-55" elevation="68" />
            </feSpecularLighting>
            <feComposite in="surfaceLight" in2="surface" operator="in" result="surfaceLightClip" />
            <feBlend in="surface" in2="surfaceLightClip" mode="screen" />
          </filter>
          <filter id="liquid-tab-filter" x="0" y="0" width="${LIQUID_TAB_WIDTH}" height="${LIQUID_TAB_HEIGHT}" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
            <feImage id="liquid-tab-map" width="${LIQUID_TAB_WIDTH}" height="${LIQUID_TAB_HEIGHT}" preserveAspectRatio="none" result="liquidTabMap" />
            <feDisplacementMap id="liquid-tab-displacement" in="SourceGraphic" in2="liquidTabMap" scale="0" xChannelSelector="R" yChannelSelector="G" />
          </filter>
        </defs>
      </svg>
      <div class="titlebar" data-drag-window>
        <div class="brand" data-tauri-drag-region>
          <img class="brand-mark" src="${appIconUrl}" alt="" draggable="false" data-tauri-drag-region />
          <div data-tauri-drag-region>
            <strong data-tauri-drag-region>메이플 유틸</strong>
          </div>
        </div>
        <div class="title-status">${titleStatus}</div>
        <button class="theme-btn" data-theme-toggle title="테마 전환" aria-label="테마 전환">${themeIcon()}</button>
        <button class="help-btn ${state.helpOpen ? "active" : ""}" data-help-open title="설명서" aria-label="설명서 열기" aria-pressed="${state.helpOpen}">?</button>
        <div class="window-controls">
          <button class="window-btn" data-window-action="minimize" title="최소화">_</button>
          <button class="window-btn close" data-window-action="close" title="닫기">x</button>
        </div>
      </div>
      ${content}
      ${helpSheet()}
      <nav class="mode-tabs ${state.panelMotion ? "moving" : ""}" aria-label="설정 선택" style="--liquid-from-x: ${liquidFromX}px; --liquid-to-x: ${liquidToX}px; --liquid-nudge-x: ${liquidNudgeX}px; --liquid-origin: ${liquidOrigin};">
        <span class="mode-liquid" aria-hidden="true"></span>
        <button class="mode-tab ${state.panel === "focus" ? "active" : ""}" data-panel="focus">포커스 설정</button>
        <button class="mode-tab ${state.panel === "filter" ? "active" : ""}" data-panel="filter">필터키</button>
      </nav>
    </div>
  `;
}

function keepGameForeground() {
  const settings = state.snapshot?.settings;
  if (!settings || settings.settings_mode || !settings.helper_noactivate) return;
  invoke("keep_game_foreground").catch(() => {});
}

function pulseGameForeground() {
  keepGameForeground();
  window.setTimeout(keepGameForeground, 0);
  window.setTimeout(keepGameForeground, 30);
  window.setTimeout(keepGameForeground, 90);
  window.setTimeout(keepGameForeground, 180);
}

function setStatus(message, isError = false) {
  const status = document.querySelector("[data-status]");
  if (!status) return;
  status.textContent = message;
  status.dataset.kind = isError ? "error" : "ok";
}

async function startGameWindowPicker() {
  if (state.busy) return;
  state.busy = true;
  render({ preserveScroll: true });
  let status = "선택할 창 위로 마우스를 옮긴 뒤 클릭하세요.";
  let isError = false;
  try {
    await invoke("show_window_picker");
  } catch (error) {
    status = String(error);
    isError = true;
  } finally {
    state.busy = false;
    render({ preserveScroll: true });
    setStatus(status, isError);
  }
}

function numberValue(id, fallback) {
  const element = document.getElementById(id);
  const value = Number.parseInt(element?.value ?? "", 10);
  return Number.isFinite(value) ? value : fallback;
}

function selectedWindowHwnds() {
  return state.selectedFocusHwnds;
}

function selectedGameHwnd() {
  return state.selectedGameHwnd;
}

function windowPickList(kind, windows, selectedHwnds, emptyText) {
  if (!windows.length) {
    return `<div class="selected"><small>${emptyText}</small></div>`;
  }

  const selected = new Set(selectedHwnds);

  return `
    <div class="window-list">
      ${windows
        .map(
          (win) => `
            <button class="window-pick ${selected.has(win.hwnd) ? "active" : ""}" data-pick-${kind}-window="${win.hwnd}" aria-pressed="${selected.has(win.hwnd)}">
              <span>${escapeHtml(win.title)}</span>
            </button>
          `,
        )
        .join("")}
    </div>
  `;
}

function render(options = {}) {
  const previousScrollTop = options.preserveScroll
    ? document.querySelector(".shell")?.scrollTop ?? 0
    : null;
  const previousWindowListScrollTop = options.preserveScroll
    ? document.querySelector(".window-list")?.scrollTop ?? 0
    : null;
  const snapshot = state.snapshot;
  if (!snapshot) {
    app.innerHTML = frame(`
      <main class="shell">
        <div class="loading">초기화 중</div>
      </main>
    `);
    installLiquidTabFilter();
    bindWindowControls();
    return;
  }

  const settings = snapshot.settings;
  const game = snapshot.game;
  const filter = snapshot.filter_current;
  const preset = settings.filter_on_preset;
  const backup = settings.filter_backup;
  const namedPresets = settings.filter_presets ?? [];
  const availableWindows = snapshot.available_windows ?? [];
  const focusCandidateWindows = availableWindows.filter((win) => !game || win.hwnd !== game.hwnd);
  const focusTargets = snapshot.focus_targets ?? [];
  const focusExceptions = snapshot.focus_exceptions ?? [];
  const focusGuardPollMs = FOCUS_GUARD_POLL_OPTIONS.includes(settings.focus_guard_poll_ms)
    ? settings.focus_guard_poll_ms
    : 75;

  if (!availableWindows.some((win) => win.hwnd === state.selectedGameHwnd)) {
    state.selectedGameHwnd = null;
  }
  state.selectedFocusHwnds = state.selectedFocusHwnds.filter((hwnd) =>
    focusCandidateWindows.some((win) => win.hwnd === hwnd),
  );

  const focusPanel = `
    <section>
      <h2>보조 창</h2>
      <div class="grid two">
        ${toggleButton("포커스 유지", "helper-noactivate", settings.helper_noactivate, !settings.settings_mode)}
        ${toggleButton("항상 위", "helper-topmost", settings.helper_topmost, true)}
      </div>
      <div class="selected spaced">
        <span>감시 주기</span>
        <small>낮을수록 빠르게 복귀하지만 호출 빈도가 늘어납니다</small>
      </div>
      ${focusGuardPollOptions(focusGuardPollMs)}
    </section>

    <section>
      <h2>키 입력 받을 창</h2>
      <div class="selected">
        <span>${game ? escapeHtml(game.title) : "선택된 창 없음"}</span>
        ${game ? "" : "<small>버튼을 누른 뒤 대상 창 위에서 클릭하세요</small>"}
      </div>
      <div class="action-list" role="group" aria-label="게임 창 작업">
        <button class="action-cell" data-action="open-game-window-picker">키 입력 받을 창 선택</button>
      </div>
      <div class="grid two option-group">
        ${toggleButton("게임 항상 위", "game-topmost", settings.game_topmost, Boolean(game))}
      </div>
    </section>

    <section>
      <h2>포커스 차단 대상</h2>
      <div class="selected">
        <span>게임 창을 제외한 다른 프로그램 차단</span>
        <small>${state.selectedFocusHwnds.length ? `${state.selectedFocusHwnds.length}개 선택됨` : "차단할 창을 여러 개 선택할 수 있습니다"}</small>
      </div>
      ${windowPickList("focus", focusCandidateWindows, state.selectedFocusHwnds, "선택 가능한 창 없음")}
      <div class="action-list" role="group" aria-label="포커스 차단 작업">
        <button class="action-cell" data-action="refresh-windows">새로고침</button>
        <button class="action-cell" data-action="apply-focus-target">선택 창 차단</button>
        <button class="action-cell" data-action="add-focus-exception">선택 창 입력 허용</button>
        <button class="action-cell" data-action="apply-all-focus-targets">게임 제외 전체 차단</button>
        <button class="action-cell" data-action="clear-focus-targets">차단 해제</button>
      </div>
      <div class="selected spaced">
        <span>입력 허용 예외</span>
        <small>전체 차단을 써도 아래 창들은 키보드 입력을 받을 수 있습니다</small>
      </div>
      <div class="action-list" role="group" aria-label="예외 작업">
        <button class="action-cell destructive" data-action="clear-focus-exceptions">예외 전체 삭제</button>
      </div>
      <div class="target-list">
        ${focusExceptions.length
          ? focusExceptions
              .map(
                (exception) => `
                  <button class="target allowed" data-remove-focus-exception="${exception.hwnd}">
                    <span>${escapeHtml(exception.title)}</span>
                    <strong>예외 해제</strong>
                  </button>
                `,
              )
              .join("")
          : `<div class="selected"><small>입력 허용 예외 없음</small></div>`}
      </div>
      <div class="target-list">
        ${focusTargets.length
          ? focusTargets
              .map(
                (target) => `
                  <button class="target" data-restore-focus-target="${target.hwnd}">
                    <span>${escapeHtml(target.title)}</span>
                    <strong>해제</strong>
                  </button>
                `,
              )
              .join("")
          : `<div class="selected"><small>차단된 창 없음</small></div>`}
      </div>
    </section>
  `;

  const filterPanel = `
    <section>
      <h2>필터키</h2>
      <div class="current">
        <span>현재</span>
        <b>Wait ${filter.wait} / Delay ${filter.delay} / Repeat ${filter.repeat} / Flags ${filter.flags}</b>
      </div>
      <div class="inputs">
        <label>
          <span>Wait</span>
          <input id="accept-delay" type="number" min="0" max="10000" value="${preset.accept_delay}" />
        </label>
        <label>
          <span>Delay</span>
          <input id="repeat-delay" type="number" min="0" max="10000" value="${preset.repeat_delay}" />
        </label>
        <label>
          <span>Repeat</span>
          <input id="repeat-rate" type="number" min="0" max="10000" value="${preset.repeat_rate}" />
        </label>
        <label>
          <span>Flags</span>
          <input id="filter-flags" type="number" min="0" max="65535" value="${preset.filter_flags}" />
        </label>
      </div>
      <div class="action-list" role="group" aria-label="필터키 작업">
        <button class="action-cell" data-action="save-filter">프리셋 저장</button>
        <button class="action-cell" data-action="apply-filter">필터키 적용</button>
        <button class="action-cell" data-action="restore-filter">백업 복원</button>
      </div>
      <div class="selected spaced">
        <span>저장한 프리셋</span>
        <small>저장한 값을 불러오면 위 입력칸에 바로 반영됩니다</small>
      </div>
      <div class="preset-list">
        ${namedPresets.length
          ? namedPresets
              .map(
                (saved, index) => `
                  <div class="preset-row">
                    <button class="preset-load" data-load-filter-preset="${index}">
                      <span>${escapeHtml(saved.name)}</span>
                      <small>Wait ${saved.preset.accept_delay} / Delay ${saved.preset.repeat_delay} / Repeat ${saved.preset.repeat_rate} / Flags ${saved.preset.filter_flags}</small>
                    </button>
                    <button class="preset-delete" data-delete-filter-preset="${index}">삭제</button>
                  </div>
                `,
              )
              .join("")
          : `<div class="selected"><small>저장된 프리셋 없음</small></div>`}
      </div>
      <div class="selected">
        <span>백업 ${backup.valid ? "있음" : "없음"}</span>
        <small>${backup.valid ? `Wait ${backup.wait} / Delay ${backup.delay} / Repeat ${backup.repeat} / Flags ${backup.flags}` : "적용 전 현재 Windows 설정을 백업합니다"}</small>
      </div>
    </section>
  `;

  app.innerHTML = frame(`
    <main class="shell">
      ${state.panel === "filter" ? filterPanel : focusPanel}
      <footer>
        <span data-status data-kind="ok"></span>
      </footer>
    </main>
  `, titleModeButton(settings));

  installLiquidTabFilter();
  bindWindowControls();
  bindEvents();
  state.panelMotion = false;
  state.panelFrom = state.panel;
  if (previousScrollTop !== null) {
    const shell = document.querySelector(".shell");
    if (shell) shell.scrollTop = previousScrollTop;
  }
  if (previousWindowListScrollTop !== null) {
    const windowList = document.querySelector(".window-list");
    if (windowList) windowList.scrollTop = previousWindowListScrollTop;
  }
}

function toggleButton(label, id, checked, enabled) {
  return `
    <button class="toggle ${checked ? "active" : ""}" data-toggle="${id}" aria-pressed="${checked}" ${enabled ? "" : "disabled"}>
      <span class="toggle-label">${label}</span>
      <span class="switch" aria-hidden="true">
        <span class="switch-knob"></span>
      </span>
    </button>
  `;
}

function focusGuardPollOptions(current) {
  return `
    <div class="segmented" role="group" aria-label="포커스 감시 주기">
      ${FOCUS_GUARD_POLL_OPTIONS.map((pollMs) => `
        <button class="segment ${pollMs === current ? "active" : ""}" data-focus-guard-poll-ms="${pollMs}" aria-pressed="${pollMs === current}">
          ${pollMs}ms
        </button>
      `).join("")}
    </div>
  `;
}

function bindEvents() {
  document.querySelector('[data-toggle="helper-noactivate"]')?.addEventListener("click", () => {
    call("set_helper_noactivate", { enabled: !state.snapshot.settings.helper_noactivate });
  });

  document.querySelector('[data-toggle="helper-topmost"]')?.addEventListener("click", () => {
    call("set_helper_topmost", { enabled: !state.snapshot.settings.helper_topmost });
  });

  document.querySelector('[data-toggle="settings-mode"]')?.addEventListener("click", () => {
    call("set_settings_mode", { enabled: !state.snapshot.settings.settings_mode });
  });

  document.querySelector('[data-toggle="game-topmost"]')?.addEventListener("click", () => {
    call("set_game_topmost", { enabled: !state.snapshot.settings.game_topmost });
  });

  document.querySelectorAll("[data-focus-guard-poll-ms]").forEach((button) => {
    button.addEventListener("click", () => {
      call("set_focus_guard_poll_ms", {
        pollMs: Number.parseInt(button.dataset.focusGuardPollMs, 10),
      });
    });
  });

  document.querySelectorAll("[data-pick-game-window]").forEach((button) => {
    button.addEventListener("click", () => {
      state.selectedGameHwnd = Number.parseInt(button.dataset.pickGameWindow, 10);
      render({ preserveScroll: true });
      setStatus("게임 창 후보를 선택했습니다.");
    });
  });

  document.querySelectorAll("[data-pick-focus-window]").forEach((button) => {
    button.addEventListener("click", () => {
      const hwnd = Number.parseInt(button.dataset.pickFocusWindow, 10);
      state.selectedFocusHwnds = state.selectedFocusHwnds.includes(hwnd)
        ? state.selectedFocusHwnds.filter((selected) => selected !== hwnd)
        : [...state.selectedFocusHwnds, hwnd];
      render({ preserveScroll: true });
      setStatus(`${state.selectedFocusHwnds.length}개 창을 선택했습니다.`);
    });
  });

  document.querySelector('[data-action="open-game-window-picker"]')?.addEventListener("click", () => {
    startGameWindowPicker();
  });

  document.querySelector('[data-action="refresh-windows"]')?.addEventListener("click", () => {
    call("get_app_state");
  });

  document.querySelector('[data-action="apply-focus-target"]')?.addEventListener("click", () => {
    const hwnds = selectedWindowHwnds();
    if (!hwnds.length) {
      setStatus("차단할 창을 선택하세요.", true);
      return;
    }
    call("apply_focus_targets", { hwnds });
  });

  document.querySelector('[data-action="add-focus-exception"]')?.addEventListener("click", () => {
    const hwnds = selectedWindowHwnds();
    if (!hwnds.length) {
      setStatus("입력을 허용할 창을 선택하세요.", true);
      return;
    }
    call("add_focus_exceptions", { hwnds });
  });

  document.querySelector('[data-action="apply-all-focus-targets"]')?.addEventListener("click", () => {
    call("apply_focus_all_non_game");
  });

  document.querySelector('[data-action="clear-focus-targets"]')?.addEventListener("click", () => {
    call("clear_focus_targets");
  });

  document.querySelector('[data-action="clear-focus-exceptions"]')?.addEventListener("click", () => {
    call("clear_focus_exceptions");
  });

  document.querySelectorAll("[data-restore-focus-target]").forEach((button) => {
    button.addEventListener("click", () => {
      call("restore_focus_target", { hwnd: Number.parseInt(button.dataset.restoreFocusTarget, 10) });
    });
  });

  document.querySelectorAll("[data-remove-focus-exception]").forEach((button) => {
    button.addEventListener("click", () => {
      call("remove_focus_exception", { hwnd: Number.parseInt(button.dataset.removeFocusException, 10) });
    });
  });

  document.querySelector('[data-action="save-filter"]')?.addEventListener("click", () => {
    const name = window.prompt("저장할 프리셋 이름을 입력하세요.", "")?.trim();
    if (!name) {
      setStatus("프리셋 저장을 취소했습니다.");
      return;
    }
    call("save_named_filter_preset", { name, preset: readPreset() });
  });

  document.querySelector('[data-action="apply-filter"]')?.addEventListener("click", () => {
    call("apply_filter_on", { preset: readPreset() });
  });

  document.querySelector('[data-action="restore-filter"]')?.addEventListener("click", () => {
    call("restore_filter_backup");
  });

  document.querySelectorAll("[data-load-filter-preset]").forEach((button) => {
    button.addEventListener("click", () => {
      const preset = state.snapshot.settings.filter_presets?.[Number.parseInt(button.dataset.loadFilterPreset, 10)];
      if (!preset) {
        setStatus("프리셋을 찾지 못했습니다.", true);
        return;
      }
      call("load_named_filter_preset", { name: preset.name });
    });
  });

  document.querySelectorAll("[data-delete-filter-preset]").forEach((button) => {
    button.addEventListener("click", () => {
      const preset = state.snapshot.settings.filter_presets?.[Number.parseInt(button.dataset.deleteFilterPreset, 10)];
      if (!preset) {
        setStatus("삭제할 프리셋을 찾지 못했습니다.", true);
        return;
      }
      if (!window.confirm(`'${preset.name}' 프리셋을 삭제할까요?`)) return;
      call("delete_named_filter_preset", { name: preset.name });
    });
  });
}

function bindWindowControls() {
  document.querySelector("[data-help-open]")?.addEventListener("click", () => {
    state.helpOpen = !state.helpOpen;
    render({ preserveScroll: true });
  });

  document.querySelector("[data-help-close]")?.addEventListener("click", () => {
    state.helpOpen = false;
    render({ preserveScroll: true });
  });

  document.querySelector("[data-help-backdrop]")?.addEventListener("click", (event) => {
    if (event.target !== event.currentTarget) return;
    state.helpOpen = false;
    render({ preserveScroll: true });
  });

  document.querySelectorAll("[data-panel]").forEach((button) => {
    button.addEventListener("click", () => {
      const nextPanel = button.dataset.panel === "filter" ? "filter" : "focus";
      if (nextPanel === state.panel) return;
      state.panelFrom = state.panel;
      state.panel = nextPanel;
      state.panelMotion = true;
      localStorage.setItem(PANEL_STORAGE_KEY, state.panel);
      render();
    });
  });

  document.querySelector("[data-theme-toggle]")?.addEventListener("click", () => {
    state.theme = state.theme === "dark" ? "light" : "dark";
    localStorage.setItem(THEME_STORAGE_KEY, state.theme);
    applyTheme();
    render();
  });

  document.querySelector("[data-drag-window]")?.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    if (event.target.closest("button, input, select, textarea, a")) return;
    event.preventDefault();
    invoke("drag_app_window").catch(() => {});
  });

  document.querySelector('[data-window-action="minimize"]')?.addEventListener("click", () => {
    invoke("minimize_app").catch(() => {});
  });

  document.querySelector('[data-window-action="close"]')?.addEventListener("click", () => {
    invoke("close_app").catch(() => {});
  });
}

document.addEventListener("pointerdown", pulseGameForeground, true);
document.addEventListener("pointerup", pulseGameForeground, true);
document.addEventListener("click", pulseGameForeground, true);
document.addEventListener("focusin", pulseGameForeground, true);
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape" || !state.helpOpen) return;
  state.helpOpen = false;
  render({ preserveScroll: true });
});

function readPreset() {
  return {
    accept_delay: numberValue("accept-delay", state.snapshot.settings.filter_on_preset.accept_delay),
    repeat_delay: numberValue("repeat-delay", state.snapshot.settings.filter_on_preset.repeat_delay),
    repeat_rate: numberValue("repeat-rate", state.snapshot.settings.filter_on_preset.repeat_rate),
    filter_flags: numberValue("filter-flags", state.snapshot.settings.filter_on_preset.filter_flags),
  };
}

const pickerState = {
  current: null,
  requestId: 0,
  lastMoveAt: 0,
};

function renderPicker() {
  app.innerHTML = `
    <main class="picker-root">
      <div class="picker-shade" aria-hidden="true"></div>
      <div class="picker-guide">
        <strong>키 입력 받을 창 선택</strong>
        <span>대상 창 위로 마우스를 옮긴 뒤 클릭하세요. Esc로 취소할 수 있습니다.</span>
      </div>
      <div class="picker-highlight" hidden>
        <div class="picker-caption"></div>
      </div>
    </main>
  `;

  document.addEventListener("mousemove", handlePickerMove);
  document.addEventListener("mousedown", handlePickerMouseDown);
  document.addEventListener("keydown", handlePickerKeyDown);
  document.addEventListener("contextmenu", (event) => event.preventDefault());
}

function handlePickerMove(event) {
  const now = performance.now();
  if (now - pickerState.lastMoveAt < 35) return;
  pickerState.lastMoveAt = now;

  const requestId = ++pickerState.requestId;
  const x = Math.round(event.screenX);
  const y = Math.round(event.screenY);

  invoke("pick_window_at_point", { x, y })
    .then((pick) => {
      if (requestId !== pickerState.requestId) return;
      updatePickerHighlight(pick);
    })
    .catch(() => {
      if (requestId !== pickerState.requestId) return;
      updatePickerHighlight(null);
    });
}

function updatePickerHighlight(pick) {
  pickerState.current = pick;
  const highlight = document.querySelector(".picker-highlight");
  const caption = document.querySelector(".picker-caption");
  if (!highlight || !caption) return;

  if (!pick) {
    highlight.hidden = true;
    caption.textContent = "";
    return;
  }

  const originX = Math.round(window.screenX ?? window.screenLeft ?? 0);
  const originY = Math.round(window.screenY ?? window.screenTop ?? 0);
  highlight.hidden = false;
  highlight.style.transform = `translate(${pick.rect.left - originX}px, ${pick.rect.top - originY}px)`;
  highlight.style.width = `${pick.rect.width}px`;
  highlight.style.height = `${pick.rect.height}px`;
  caption.textContent = pick.info.title;
}

async function handlePickerMouseDown(event) {
  event.preventDefault();
  if (event.button !== 0) {
    await invoke("close_window_picker").catch(() => {});
    return;
  }

  const title = document.querySelector(".picker-guide strong");
  if (title) title.textContent = "지정 중";
  const x = Math.round(event.screenX);
  const y = Math.round(event.screenY);
  const pick = pickerState.current ?? (await invoke("pick_window_at_point", { x, y }).catch(() => null));

  if (!pick) {
    if (title) title.textContent = "창을 찾지 못했습니다";
    await invoke("close_window_picker").catch(() => {});
    return;
  }

  await invoke("select_picked_game_window", { hwnd: pick.info.hwnd }).catch(async () => {
    const title = document.querySelector(".picker-guide strong");
    if (title) title.textContent = "선택 실패";
    await invoke("close_window_picker").catch(() => {});
  });
}

async function handlePickerKeyDown(event) {
  if (event.key === "Escape") {
    await invoke("close_window_picker").catch(() => {});
  }
}

async function bootstrap() {
  try {
    await listen("game-window-picked", async () => {
      try {
        state.snapshot = await invoke("get_app_state");
        render({ preserveScroll: true });
        setStatus("키 입력 받을 창을 지정했습니다.");
      } catch (error) {
        setStatus(String(error), true);
      }
    });
    await listen("game-window-pick-cancelled", async (event) => {
      try {
        state.snapshot = await invoke("get_app_state");
        render({ preserveScroll: true });
        setStatus(String(event.payload || "창 선택을 취소했습니다."), true);
      } catch (error) {
        setStatus(String(error), true);
      }
    });
    state.snapshot = await invoke("get_app_state");
  } catch (error) {
    app.innerHTML = frame(`<main class="shell"><section><h2>초기화 실패</h2><p>${escapeHtml(error)}</p></section></main>`);
    installLiquidTabFilter();
    bindWindowControls();
    return;
  }
  render();
}

if (PICKER_MODE === "game") {
  renderPicker();
} else {
  bootstrap();
}
