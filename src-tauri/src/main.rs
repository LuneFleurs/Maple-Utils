#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{
    ffi::c_void,
    fs,
    mem::size_of,
    path::PathBuf,
    sync::{
        atomic::{AtomicI32, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{Emitter, Manager, WindowEvent};

const APP_WINDOW_TITLE: &str = "Maple Utils";

const GWL_EXSTYLE: i32 = -20;
const WS_EX_TOPMOST: isize = 0x00000008;
const WS_EX_NOACTIVATE: isize = 0x08000000;

const HWND_TOPMOST: Hwnd = -1;
const HWND_NOTOPMOST: Hwnd = -2;

const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;

const SPI_GETFILTERKEYS: u32 = 0x0032;
const SPI_SETFILTERKEYS: u32 = 0x0033;
const SPIF_UPDATEINIFILE: u32 = 0x0001;
const SPIF_SENDCHANGE: u32 = 0x0002;
const WM_NCLBUTTONDOWN: u32 = 0x00A1;
const HTCAPTION: usize = 2;
const WINDOW_PICKER_LABEL: &str = "window-picker";
const WINDOW_PICKER_TITLE: &str = "키 입력 받을 창 선택";
const VK_LBUTTON: i32 = 0x01;
const VK_RBUTTON: i32 = 0x02;
const VK_ESCAPE: i32 = 0x1B;
const VK_RETURN: i32 = 0x0D;
const VK_SPACE: i32 = 0x20;
const WH_MOUSE_LL: i32 = 14;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_RBUTTONDOWN: u32 = 0x0204;
const PM_REMOVE: u32 = 0x0001;
const GA_ROOT: u32 = 2;
const WS_EX_TRANSPARENT_MOUSE: u32 = 0x00000020;
const WS_EX_LAYERED: u32 = 0x00080000;
const WS_EX_TOOLWINDOW: u32 = 0x00000080;
const WS_POPUP: u32 = 0x80000000;
const WS_VISIBLE: u32 = 0x10000000;
const SS_WHITERECT: u32 = 0x00000006;
const LWA_ALPHA: u32 = 0x00000002;
const SW_HIDE: i32 = 0;
const SW_SHOWNOACTIVATE: i32 = 4;
const HOVER_BORDER_SIZE: i32 = 4;
const HOVER_BORDER_ALPHA: u8 = 185;
const PICKER_POLL_MS: u64 = 8;
const PICKER_TIMEOUT_TICKS: usize = 3_750;
const DEFAULT_FOCUS_GUARD_POLL_MS: u64 = 75;
const FOCUS_GUARD_POLL_OPTIONS: [u64; 3] = [75, 30, 16];
type Hwnd = isize;
type Hhook = isize;
type Bool = i32;

static PICKER_MOUSE_EVENT: AtomicI32 = AtomicI32::new(0);
static PICKER_MOUSE_X: AtomicI32 = AtomicI32::new(0);
static PICKER_MOUSE_Y: AtomicI32 = AtomicI32::new(0);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct FilterKeysRaw {
    cb_size: u32,
    dw_flags: u32,
    i_wait_msec: u32,
    i_delay_msec: u32,
    i_repeat_msec: u32,
    i_bounce_msec: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RectRaw {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct PointRaw {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct MsgRaw {
    hwnd: Hwnd,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: PointRaw,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct MouseHookRaw {
    pt: PointRaw,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FilterPreset {
    accept_delay: u32,
    repeat_delay: u32,
    repeat_rate: u32,
    filter_flags: u32,
}

impl Default for FilterPreset {
    fn default() -> Self {
        Self {
            accept_delay: 0,
            repeat_delay: 150,
            repeat_rate: 1,
            filter_flags: 27,
        }
    }
}

impl FilterPreset {
    fn is_legacy_default(&self) -> bool {
        self.accept_delay == 0
            && self.repeat_delay == 90
            && self.repeat_rate == 14
            && self.filter_flags == 35
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct FilterBackup {
    valid: bool,
    wait: u32,
    delay: u32,
    repeat: u32,
    flags: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct NamedFilterPreset {
    name: String,
    preset: FilterPreset,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WindowInfo {
    hwnd: isize,
    title: String,
    pid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct WindowRect {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

#[derive(Clone, Debug, Serialize)]
struct WindowPick {
    info: WindowInfo,
    rect: WindowRect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Settings {
    helper_noactivate: bool,
    helper_topmost: bool,
    settings_mode: bool,
    game_topmost: bool,
    #[serde(default = "default_focus_guard_poll_ms")]
    focus_guard_poll_ms: u64,
    restore_filter_on_exit: bool,
    filter_on_preset: FilterPreset,
    #[serde(default)]
    filter_presets: Vec<NamedFilterPreset>,
    filter_backup: FilterBackup,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            helper_noactivate: true,
            helper_topmost: true,
            settings_mode: false,
            game_topmost: false,
            focus_guard_poll_ms: DEFAULT_FOCUS_GUARD_POLL_MS,
            restore_filter_on_exit: true,
            filter_on_preset: FilterPreset::default(),
            filter_presets: vec![NamedFilterPreset {
                name: "기본".into(),
                preset: FilterPreset::default(),
            }],
            filter_backup: FilterBackup::default(),
        }
    }
}

fn default_focus_guard_poll_ms() -> u64 {
    DEFAULT_FOCUS_GUARD_POLL_MS
}

fn sanitize_focus_guard_poll_ms(value: u64) -> u64 {
    if FOCUS_GUARD_POLL_OPTIONS.contains(&value) {
        value
    } else {
        DEFAULT_FOCUS_GUARD_POLL_MS
    }
}

#[derive(Debug)]
struct RuntimeState {
    settings: Settings,
    game: Option<WindowInfo>,
    game_original_topmost: Option<bool>,
    focus_targets: Vec<FocusTarget>,
    focus_exceptions: Vec<WindowInfo>,
}

#[derive(Clone, Debug, Serialize)]
struct FilterSnapshot {
    wait: u32,
    delay: u32,
    repeat: u32,
    flags: u32,
}

#[derive(Clone, Debug, Serialize)]
struct AppSnapshot {
    settings: Settings,
    game: Option<WindowInfo>,
    available_windows: Vec<WindowInfo>,
    focus_targets: Vec<FocusTargetSnapshot>,
    focus_exceptions: Vec<WindowInfo>,
    filter_current: FilterSnapshot,
}

#[derive(Clone, Debug)]
struct FocusTarget {
    info: WindowInfo,
    original_ex_style: isize,
    original_child_ex_styles: Vec<WindowExStyleSnapshot>,
}

#[derive(Clone, Debug)]
struct WindowExStyleSnapshot {
    hwnd: Hwnd,
    ex_style: isize,
}

#[derive(Clone, Debug, Serialize)]
struct FocusTargetSnapshot {
    hwnd: isize,
    title: String,
    pid: u32,
}

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(
        callback: Option<unsafe extern "system" fn(Hwnd, isize) -> Bool>,
        lparam: isize,
    ) -> Bool;
    fn EnumChildWindows(
        parent: Hwnd,
        callback: Option<unsafe extern "system" fn(Hwnd, isize) -> Bool>,
        lparam: isize,
    ) -> Bool;
    fn GetWindowThreadProcessId(hwnd: Hwnd, process_id: *mut u32) -> u32;
    fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
    fn GetWindowTextW(hwnd: Hwnd, text: *mut u16, max_count: i32) -> i32;
    fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
    fn SetWindowPos(
        hwnd: Hwnd,
        insert_after: Hwnd,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> Bool;
    fn IsWindow(hwnd: Hwnd) -> Bool;
    fn IsWindowVisible(hwnd: Hwnd) -> Bool;
    fn GetForegroundWindow() -> Hwnd;
    fn SetForegroundWindow(hwnd: Hwnd) -> Bool;
    fn BringWindowToTop(hwnd: Hwnd) -> Bool;
    fn SetActiveWindow(hwnd: Hwnd) -> Hwnd;
    fn SetFocus(hwnd: Hwnd) -> Hwnd;
    fn AttachThreadInput(id_attach: u32, id_attach_to: u32, attach: Bool) -> Bool;
    fn SystemParametersInfoW(action: u32, param: u32, value: *mut c_void, flags: u32) -> Bool;
    fn ReleaseCapture() -> Bool;
    fn SendMessageW(hwnd: Hwnd, message: u32, w_param: usize, l_param: isize) -> isize;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut RectRaw) -> Bool;
    fn GetCursorPos(point: *mut PointRaw) -> Bool;
    fn GetAsyncKeyState(v_key: i32) -> i16;
    fn WindowFromPoint(point: PointRaw) -> Hwnd;
    fn GetAncestor(hwnd: Hwnd, flags: u32) -> Hwnd;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: isize,
        instance: isize,
        param: *mut c_void,
    ) -> Hwnd;
    fn DestroyWindow(hwnd: Hwnd) -> Bool;
    fn ShowWindow(hwnd: Hwnd, command: i32) -> Bool;
    fn UpdateWindow(hwnd: Hwnd) -> Bool;
    fn SetLayeredWindowAttributes(hwnd: Hwnd, color_key: u32, alpha: u8, flags: u32) -> Bool;
    fn SetWindowsHookExW(
        id_hook: i32,
        callback: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        module: isize,
        thread_id: u32,
    ) -> Hhook;
    fn CallNextHookEx(hook: Hhook, code: i32, w_param: usize, l_param: isize) -> isize;
    fn UnhookWindowsHookEx(hook: Hhook) -> Bool;
    fn PeekMessageW(
        message: *mut MsgRaw,
        hwnd: Hwnd,
        filter_min: u32,
        filter_max: u32,
        remove_message: u32,
    ) -> Bool;
    fn TranslateMessage(message: *const MsgRaw) -> Bool;
    fn DispatchMessageW(message: *const MsgRaw) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let settings = load_settings();
            let runtime = RuntimeState {
                settings,
                game: None,
                game_original_topmost: None,
                focus_targets: Vec::new(),
                focus_exceptions: Vec::new(),
            };
            app.manage(Mutex::new(runtime));

            let handle = app.handle().clone();
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| "main window not found".to_string())?;

            apply_startup_window_style(&window, &handle)?;
            start_focus_guard(app.handle().clone());

            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::CloseRequested { .. }) {
                    cleanup(&handle);
                }
                if matches!(event, WindowEvent::Focused(true)) {
                    restore_game_foreground(&handle);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            set_helper_noactivate,
            set_helper_topmost,
            set_settings_mode,
            keep_game_foreground,
            set_focus_guard_poll_ms,
            apply_focus_target,
            apply_focus_targets,
            apply_focus_all_non_game,
            add_focus_exception,
            add_focus_exceptions,
            remove_focus_exception,
            clear_focus_exceptions,
            restore_focus_target,
            clear_focus_targets,
            show_window_picker,
            close_window_picker,
            pick_window_at_point,
            select_picked_game_window,
            select_foreground_game,
            select_game_window,
            set_game_topmost,
            drag_app_window,
            minimize_app,
            close_app,
            save_filter_preset,
            save_named_filter_preset,
            load_named_filter_preset,
            delete_named_filter_preset,
            apply_filter_on,
            restore_filter_backup
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn get_app_state(state: tauri::State<'_, Mutex<RuntimeState>>) -> Result<AppSnapshot, String> {
    let state = state.lock().map_err(|_| "state lock failed")?;
    snapshot(&state)
}

#[tauri::command]
fn minimize_app(app: tauri::AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?
        .minimize()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn close_app(app: tauri::AppHandle) -> Result<(), String> {
    cleanup(&app);
    app.get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?
        .close()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn drag_app_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let hwnd = window_hwnd(&window)?;

    unsafe {
        ReleaseCapture();
        SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION, 0);
    }

    Ok(())
}

#[tauri::command]
fn set_helper_noactivate(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RuntimeState>>,
    enabled: bool,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    state.settings.helper_noactivate = enabled;
    apply_helper_window_mode(&app, &state.settings)?;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn set_helper_topmost(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RuntimeState>>,
    enabled: bool,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    state.settings.helper_topmost = enabled;
    apply_helper_window_mode(&app, &state.settings)?;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn set_settings_mode(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RuntimeState>>,
    enabled: bool,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    state.settings.settings_mode = enabled;
    apply_helper_window_mode(&app, &state.settings)?;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn set_focus_guard_poll_ms(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    poll_ms: u64,
) -> Result<AppSnapshot, String> {
    if !FOCUS_GUARD_POLL_OPTIONS.contains(&poll_ms) {
        return Err("지원하는 감시 주기는 75ms, 30ms, 16ms입니다.".into());
    }

    let mut state = state.lock().map_err(|_| "state lock failed")?;
    state.settings.focus_guard_poll_ms = poll_ms;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn keep_game_foreground(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RuntimeState>>,
) -> Result<(), String> {
    let state = state.lock().map_err(|_| "state lock failed")?;
    if state.settings.settings_mode || !state.settings.helper_noactivate {
        return Ok(());
    }

    apply_helper_window_mode(&app, &state.settings)?;

    if let Some(game) = state.game.as_ref() {
        if is_window(game.hwnd) {
            force_foreground_window(game.hwnd);
        }
    }

    Ok(())
}

#[tauri::command]
fn apply_focus_target(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    hwnd: isize,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    apply_focus_target_in_state(&mut state, hwnd)?;
    snapshot(&state)
}

#[tauri::command]
fn apply_focus_targets(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    hwnds: Vec<isize>,
) -> Result<AppSnapshot, String> {
    if hwnds.is_empty() {
        return Err("차단할 창을 선택하세요.".into());
    }

    let mut state = state.lock().map_err(|_| "state lock failed")?;
    for hwnd in unique_hwnds(hwnds) {
        apply_focus_target_in_state(&mut state, hwnd)?;
    }
    snapshot(&state)
}

fn apply_focus_target_in_state(state: &mut RuntimeState, hwnd: Hwnd) -> Result<(), String> {
    ensure_window(hwnd)?;

    let info = window_info(hwnd)?;
    if info.pid == std::process::id() {
        return Err("Maple Utils 창은 대상에 추가할 수 없습니다.".into());
    }
    if state.game.as_ref().is_some_and(|game| game.hwnd == hwnd) {
        return Err(
            "게임 창은 포커스 차단 대상이 아닙니다. Firefox 같은 보조 창을 선택하세요.".into(),
        );
    }
    if state
        .focus_exceptions
        .iter()
        .any(|exception| exception.hwnd == hwnd)
    {
        return Err("입력 허용 예외로 등록된 창입니다. 먼저 예외에서 제거하세요.".into());
    }

    if let Some(target) = state
        .focus_targets
        .iter_mut()
        .find(|target| target.info.hwnd == hwnd)
    {
        target.info = info;
        remember_child_ex_styles(target);
    } else {
        let original_ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        let original_child_ex_styles = capture_child_ex_styles(hwnd);
        state.focus_targets.push(FocusTarget {
            info,
            original_ex_style,
            original_child_ex_styles,
        });
    }

    set_noactivate_tree(hwnd, true)?;
    Ok(())
}

#[tauri::command]
fn add_focus_exception(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    hwnd: isize,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    add_focus_exception_in_state(&mut state, hwnd)?;
    snapshot(&state)
}

#[tauri::command]
fn add_focus_exceptions(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    hwnds: Vec<isize>,
) -> Result<AppSnapshot, String> {
    if hwnds.is_empty() {
        return Err("입력을 허용할 창을 선택하세요.".into());
    }

    let mut state = state.lock().map_err(|_| "state lock failed")?;
    for hwnd in unique_hwnds(hwnds) {
        add_focus_exception_in_state(&mut state, hwnd)?;
    }
    snapshot(&state)
}

fn add_focus_exception_in_state(state: &mut RuntimeState, hwnd: Hwnd) -> Result<(), String> {
    ensure_window(hwnd)?;

    let info = window_info(hwnd)?;
    if info.pid == std::process::id() {
        return Err("Maple Utils 창은 예외로 등록할 필요가 없습니다.".into());
    }
    if state.game.as_ref().is_some_and(|game| game.hwnd == hwnd) {
        return Err("게임 창은 항상 예외입니다.".into());
    }

    if let Some(index) = state
        .focus_targets
        .iter()
        .position(|target| target.info.hwnd == hwnd)
    {
        let target = state.focus_targets.remove(index);
        restore_focus_target_window(target)?;
    }

    if let Some(exception) = state
        .focus_exceptions
        .iter_mut()
        .find(|exception| exception.hwnd == hwnd)
    {
        *exception = info;
    } else {
        state.focus_exceptions.push(info);
    }

    Ok(())
}

fn unique_hwnds(hwnds: Vec<isize>) -> Vec<Hwnd> {
    let mut unique = Vec::new();
    for hwnd in hwnds {
        if hwnd != 0 && !unique.contains(&hwnd) {
            unique.push(hwnd);
        }
    }
    unique
}

#[tauri::command]
fn remove_focus_exception(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    hwnd: isize,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    state
        .focus_exceptions
        .retain(|exception| exception.hwnd != hwnd);
    snapshot(&state)
}

#[tauri::command]
fn clear_focus_exceptions(
    state: tauri::State<'_, Mutex<RuntimeState>>,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    state.focus_exceptions.clear();
    snapshot(&state)
}

#[tauri::command]
fn apply_focus_all_non_game(
    state: tauri::State<'_, Mutex<RuntimeState>>,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    let game_hwnd = state
        .game
        .as_ref()
        .map(|game| game.hwnd)
        .ok_or_else(|| "먼저 게임 창을 지정하세요.".to_string())?;

    for info in enumerate_available_windows() {
        if info.hwnd == game_hwnd || info.pid == std::process::id() {
            continue;
        }
        if state
            .focus_exceptions
            .iter()
            .any(|exception| exception.hwnd == info.hwnd)
        {
            continue;
        }

        if state.focus_targets.iter_mut().any(|target| {
            if target.info.hwnd == info.hwnd {
                remember_child_ex_styles(target);
                let _ = set_noactivate_tree(info.hwnd, true);
                true
            } else {
                false
            }
        }) {
            continue;
        }

        let original_ex_style = unsafe { GetWindowLongPtrW(info.hwnd, GWL_EXSTYLE) };
        let original_child_ex_styles = capture_child_ex_styles(info.hwnd);
        if set_noactivate_tree(info.hwnd, true).is_ok() {
            state.focus_targets.push(FocusTarget {
                info,
                original_ex_style,
                original_child_ex_styles,
            });
        }
    }

    snapshot(&state)
}

#[tauri::command]
fn restore_focus_target(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    hwnd: isize,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    if let Some(index) = state
        .focus_targets
        .iter()
        .position(|target| target.info.hwnd == hwnd)
    {
        let target = state.focus_targets.remove(index);
        restore_focus_target_window(target)?;
    }
    snapshot(&state)
}

#[tauri::command]
fn clear_focus_targets(
    state: tauri::State<'_, Mutex<RuntimeState>>,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    restore_all_focus_targets(&mut state);
    snapshot(&state)
}

#[tauri::command]
fn show_window_picker(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_PICKER_LABEL) {
        let _ = window.close();
    }

    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    main.hide().map_err(|error| error.to_string())?;

    let picker_app = app.clone();
    thread::spawn(move || {
        wait_until_key_up(VK_LBUTTON);
        thread::sleep(Duration::from_millis(120));

        let mut hover_outline = HoverOutline::new();
        let mouse_hook = MouseClickHook::new();
        let mut result: Result<Hwnd, String> = Err("창 선택 시간이 초과되었습니다.".into());
        for _ in 0..PICKER_TIMEOUT_TICKS {
            pump_thread_messages();
            let hover = cursor_position().and_then(|(x, y)| window_at_point(x, y));
            hover_outline.update(hover.as_ref().map(|pick| &pick.rect));

            if let Some(click) = take_picker_mouse_event() {
                result = match click {
                    PickerMouseEvent::Left { x, y } => window_at_point(x, y)
                        .or(hover)
                        .map(|pick| pick.info.hwnd)
                        .ok_or_else(|| {
                            "클릭한 위치에서 선택 가능한 창을 찾지 못했습니다.".to_string()
                        }),
                    PickerMouseEvent::Right => Err("창 선택을 취소했습니다.".into()),
                };
                break;
            }

            if is_key_pressed_or_down(VK_ESCAPE) || is_key_pressed_or_down(VK_RBUTTON) {
                result = Err("창 선택을 취소했습니다.".into());
                break;
            }

            if is_key_pressed_or_down(VK_RETURN) || is_key_pressed_or_down(VK_SPACE) {
                result = hover
                    .map(|pick| pick.info.hwnd)
                    .ok_or_else(|| "마우스를 선택할 창 위에 올려두세요.".to_string());
                break;
            }

            if is_key_pressed_or_down(VK_LBUTTON) {
                result = hover
                    .or_else(|| cursor_position().and_then(|(x, y)| window_at_point(x, y)))
                    .map(|pick| pick.info.hwnd)
                    .ok_or_else(|| "클릭한 위치에서 선택 가능한 창을 찾지 못했습니다.".to_string());
                break;
            }

            thread::sleep(Duration::from_millis(PICKER_POLL_MS));
        }

        hover_outline.hide();
        drop(mouse_hook);

        finish_game_window_pick(&picker_app, result);
    });

    Ok(())
}

#[tauri::command]
fn close_window_picker(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_PICKER_LABEL) {
        window.close().map_err(|error| error.to_string())?;
    }
    restore_main_window_after_pick(&app);
    Ok(())
}

fn finish_game_window_pick(app: &tauri::AppHandle, result: Result<Hwnd, String>) {
    let result = result.and_then(|hwnd| {
        let state = app.state::<Mutex<RuntimeState>>();
        let mut state = state.lock().map_err(|_| "state lock failed".to_string())?;
        select_game_window_in_state(&mut state, hwnd)?;
        save_settings(&state.settings)?;
        Ok(hwnd)
    });

    restore_main_window_after_pick(app);

    match result {
        Ok(_) => {
            let _ = app.emit_to("main", "game-window-picked", ());
        }
        Err(message) => {
            let _ = app.emit_to("main", "game-window-pick-cancelled", message);
        }
    }
}

fn restore_main_window_after_pick(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let _ = window.show();

    let state = app.state::<Mutex<RuntimeState>>();
    let Ok(state) = state.lock() else {
        return;
    };
    let _ = apply_helper_window_mode_for_window(&window, &state.settings);
}

#[tauri::command]
fn pick_window_at_point(x: i32, y: i32) -> Result<Option<WindowPick>, String> {
    let (screen_x, screen_y) = cursor_position().unwrap_or((x, y));
    Ok(window_at_point(screen_x, screen_y))
}

#[tauri::command]
fn select_picked_game_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<RuntimeState>>,
    hwnd: isize,
) -> Result<(), String> {
    {
        let mut state = state.lock().map_err(|_| "state lock failed")?;
        select_game_window_in_state(&mut state, hwnd)?;
        save_settings(&state.settings)?;
    }

    close_window_picker(app.clone())?;
    app.emit_to("main", "game-window-picked", ())
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn select_foreground_game(
    state: tauri::State<'_, Mutex<RuntimeState>>,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd == 0 {
        return Err("현재 활성 창을 찾지 못했습니다.".into());
    }

    let info = window_info(hwnd)?;
    if info.pid == std::process::id() {
        return Err(
            "보조 창이 선택되었습니다. 설정 모드를 끄고 게임 창을 활성화한 뒤 다시 선택하세요."
                .into(),
        );
    }

    state.game_original_topmost = Some(is_topmost(hwnd));
    state.game = Some(info);
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn select_game_window(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    hwnd: isize,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    select_game_window_in_state(&mut state, hwnd)?;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn set_game_topmost(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    enabled: bool,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    let hwnd = state
        .game
        .as_ref()
        .map(|game| game.hwnd)
        .ok_or_else(|| "먼저 게임 창을 선택하세요.".to_string())?;

    ensure_window(hwnd)?;

    if state.game_original_topmost.is_none() {
        state.game_original_topmost = Some(is_topmost(hwnd));
    }

    set_topmost(hwnd, enabled)?;
    state.settings.game_topmost = enabled;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn save_filter_preset(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    preset: FilterPreset,
) -> Result<AppSnapshot, String> {
    validate_preset(&preset)?;
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    state.settings.filter_on_preset = preset;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn save_named_filter_preset(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    name: String,
    preset: FilterPreset,
) -> Result<AppSnapshot, String> {
    validate_preset(&preset)?;
    let name = normalize_preset_name(name)?;
    let mut state = state.lock().map_err(|_| "state lock failed")?;

    if let Some(saved) = state
        .settings
        .filter_presets
        .iter_mut()
        .find(|saved| saved.name.eq_ignore_ascii_case(&name))
    {
        saved.name = name;
        saved.preset = preset.clone();
    } else {
        state.settings.filter_presets.push(NamedFilterPreset {
            name,
            preset: preset.clone(),
        });
    }

    state.settings.filter_on_preset = preset;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn load_named_filter_preset(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    name: String,
) -> Result<AppSnapshot, String> {
    let name = normalize_preset_name(name)?;
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    let preset = state
        .settings
        .filter_presets
        .iter()
        .find(|saved| saved.name.eq_ignore_ascii_case(&name))
        .map(|saved| saved.preset.clone())
        .ok_or_else(|| "프리셋을 찾지 못했습니다.".to_string())?;

    state.settings.filter_on_preset = preset;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn delete_named_filter_preset(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    name: String,
) -> Result<AppSnapshot, String> {
    let name = normalize_preset_name(name)?;
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    let before = state.settings.filter_presets.len();
    state
        .settings
        .filter_presets
        .retain(|saved| !saved.name.eq_ignore_ascii_case(&name));
    if state.settings.filter_presets.len() == before {
        return Err("삭제할 프리셋을 찾지 못했습니다.".into());
    }

    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn apply_filter_on(
    state: tauri::State<'_, Mutex<RuntimeState>>,
    preset: FilterPreset,
) -> Result<AppSnapshot, String> {
    validate_preset(&preset)?;
    let mut state = state.lock().map_err(|_| "state lock failed")?;

    if !state.settings.filter_backup.valid {
        let current = get_filter_keys()?;
        state.settings.filter_backup = FilterBackup {
            valid: true,
            wait: current.i_wait_msec,
            delay: current.i_delay_msec,
            repeat: current.i_repeat_msec,
            flags: current.dw_flags,
        };
    }

    let raw = FilterKeysRaw {
        cb_size: size_of::<FilterKeysRaw>() as u32,
        dw_flags: preset.filter_flags,
        i_wait_msec: preset.accept_delay,
        i_delay_msec: preset.repeat_delay,
        i_repeat_msec: preset.repeat_rate,
        i_bounce_msec: 0,
    };

    set_filter_keys(raw, SPIF_UPDATEINIFILE | SPIF_SENDCHANGE)?;
    state.settings.filter_on_preset = preset;
    save_settings(&state.settings)?;
    snapshot(&state)
}

#[tauri::command]
fn restore_filter_backup(
    state: tauri::State<'_, Mutex<RuntimeState>>,
) -> Result<AppSnapshot, String> {
    let mut state = state.lock().map_err(|_| "state lock failed")?;
    restore_filter_from_state(&mut state)?;
    save_settings(&state.settings)?;
    snapshot(&state)
}

fn apply_startup_window_style(
    window: &tauri::WebviewWindow,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let state = app.state::<Mutex<RuntimeState>>();
    let state = state.lock().map_err(|_| "state lock failed")?;
    apply_helper_window_mode_for_window(window, &state.settings)?;
    Ok(())
}

fn apply_helper_window_mode(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    apply_helper_window_mode_for_window(&window, settings)
}

fn apply_helper_window_mode_for_window(
    window: &tauri::WebviewWindow,
    settings: &Settings,
) -> Result<(), String> {
    let noactivate = !settings.settings_mode && settings.helper_noactivate;
    window
        .set_focusable(!noactivate)
        .map_err(|error| error.to_string())?;

    let hwnd = window_hwnd(window)?;
    set_topmost(hwnd, settings.helper_topmost)?;
    set_noactivate_tree(hwnd, noactivate)?;
    Ok(())
}

fn cleanup(app: &tauri::AppHandle) {
    let state = app.state::<Mutex<RuntimeState>>();
    let Ok(mut state) = state.lock() else {
        return;
    };

    if state.settings.restore_filter_on_exit {
        let _ = restore_filter_from_state(&mut state);
    }

    if let Some(game) = state.game.as_ref() {
        if state.settings.game_topmost {
            if let Some(original) = state.game_original_topmost {
                if !original {
                    let _ = set_topmost(game.hwnd, false);
                }
            }
        }
    }

    restore_all_focus_targets(&mut state);
    let _ = save_settings(&state.settings);
}

fn restore_all_focus_targets(state: &mut RuntimeState) {
    let targets = std::mem::take(&mut state.focus_targets);
    for target in targets {
        let _ = restore_focus_target_window(target);
    }
}

fn restore_game_foreground(app: &tauri::AppHandle) {
    let state = app.state::<Mutex<RuntimeState>>();
    let Ok(state) = state.lock() else {
        return;
    };

    if state.settings.settings_mode || !state.settings.helper_noactivate {
        return;
    }

    if let Some(game) = state.game.as_ref() {
        if is_window(game.hwnd) {
            force_foreground_window(game.hwnd);
        }
    }
}

fn start_focus_guard(app: tauri::AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(focus_guard_poll_ms(&app)));

        let foreground = unsafe { GetForegroundWindow() };
        if foreground == 0 {
            continue;
        }

        if let Some(game_hwnd) = focus_guard_game_to_restore(&app, foreground) {
            force_foreground_window(game_hwnd);
        }
    });
}

fn focus_guard_poll_ms(app: &tauri::AppHandle) -> u64 {
    let state = app.state::<Mutex<RuntimeState>>();
    let Ok(state) = state.lock() else {
        return DEFAULT_FOCUS_GUARD_POLL_MS;
    };
    sanitize_focus_guard_poll_ms(state.settings.focus_guard_poll_ms)
}

fn focus_guard_game_to_restore(app: &tauri::AppHandle, foreground: Hwnd) -> Option<Hwnd> {
    let state = app.state::<Mutex<RuntimeState>>();
    let Ok(mut state) = state.lock() else {
        return None;
    };

    if state.settings.settings_mode || !state.settings.helper_noactivate {
        return None;
    }

    let game_hwnd = state.game.as_ref()?.hwnd;
    if !is_window(game_hwnd) || same_root_window(foreground, game_hwnd) {
        return None;
    }

    for target in state.focus_targets.iter_mut() {
        if same_root_window(foreground, target.info.hwnd) {
            remember_child_ex_styles(target);
            let _ = set_noactivate_tree(target.info.hwnd, true);
            return Some(game_hwnd);
        }
    }

    None
}

fn snapshot(state: &RuntimeState) -> Result<AppSnapshot, String> {
    let current = get_filter_keys()?;
    Ok(AppSnapshot {
        settings: state.settings.clone(),
        game: state.game.clone().filter(|game| is_window(game.hwnd)),
        available_windows: enumerate_available_windows(),
        focus_targets: state
            .focus_targets
            .iter()
            .filter(|target| is_window(target.info.hwnd))
            .map(|target| FocusTargetSnapshot {
                hwnd: target.info.hwnd,
                title: target.info.title.clone(),
                pid: target.info.pid,
            })
            .collect(),
        focus_exceptions: state
            .focus_exceptions
            .iter()
            .filter(|exception| is_window(exception.hwnd))
            .cloned()
            .collect(),
        filter_current: FilterSnapshot {
            wait: current.i_wait_msec,
            delay: current.i_delay_msec,
            repeat: current.i_repeat_msec,
            flags: current.dw_flags,
        },
    })
}

fn select_game_window_in_state(state: &mut RuntimeState, hwnd: Hwnd) -> Result<(), String> {
    ensure_window(hwnd)?;

    let info = window_info(hwnd)?;
    if info.pid == std::process::id() {
        return Err("Maple Utils 창은 키 입력 받을 창으로 지정할 수 없습니다.".into());
    }

    if let Some(index) = state
        .focus_targets
        .iter()
        .position(|target| target.info.hwnd == hwnd)
    {
        let target = state.focus_targets.remove(index);
        restore_focus_target_window(target)?;
    }
    state
        .focus_exceptions
        .retain(|exception| exception.hwnd != hwnd);

    state.game_original_topmost = Some(is_topmost(hwnd));
    state.game = Some(info);
    Ok(())
}

fn validate_preset(preset: &FilterPreset) -> Result<(), String> {
    if preset.accept_delay > 10_000 {
        return Err("Wait는 0~10000 사이여야 합니다.".into());
    }
    if preset.repeat_delay > 10_000 {
        return Err("Delay는 0~10000 사이여야 합니다.".into());
    }
    if preset.repeat_rate > 10_000 {
        return Err("Repeat는 0~10000 사이여야 합니다.".into());
    }
    if preset.filter_flags > 65_535 {
        return Err("Flags는 0~65535 사이여야 합니다.".into());
    }
    Ok(())
}

fn normalize_preset_name(name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("프리셋 이름을 입력하세요.".into());
    }
    if name.chars().count() > 32 {
        return Err("프리셋 이름은 32자 이하로 입력하세요.".into());
    }
    Ok(name.to_string())
}

fn restore_filter_from_state(state: &mut RuntimeState) -> Result<(), String> {
    if !state.settings.filter_backup.valid {
        return Err("복원할 필터키 백업이 없습니다.".into());
    }

    let backup = state.settings.filter_backup.clone();
    let raw = FilterKeysRaw {
        cb_size: size_of::<FilterKeysRaw>() as u32,
        dw_flags: backup.flags,
        i_wait_msec: backup.wait,
        i_delay_msec: backup.delay,
        i_repeat_msec: backup.repeat,
        i_bounce_msec: 0,
    };

    set_filter_keys(raw, SPIF_UPDATEINIFILE | SPIF_SENDCHANGE)?;
    Ok(())
}

fn get_filter_keys() -> Result<FilterKeysRaw, String> {
    let mut raw = FilterKeysRaw {
        cb_size: size_of::<FilterKeysRaw>() as u32,
        ..FilterKeysRaw::default()
    };

    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETFILTERKEYS,
            raw.cb_size,
            &mut raw as *mut _ as *mut c_void,
            0,
        )
    };

    if ok == 0 {
        return Err("Windows 필터키 값을 읽지 못했습니다.".into());
    }

    Ok(raw)
}

fn set_filter_keys(mut raw: FilterKeysRaw, flags: u32) -> Result<(), String> {
    raw.cb_size = size_of::<FilterKeysRaw>() as u32;
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_SETFILTERKEYS,
            raw.cb_size,
            &mut raw as *mut _ as *mut c_void,
            flags,
        )
    };

    if ok == 0 {
        return Err("Windows 필터키 값을 적용하지 못했습니다.".into());
    }

    Ok(())
}

fn set_noactivate(hwnd: Hwnd, enabled: bool) -> Result<(), String> {
    ensure_window(hwnd)?;
    let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    let next = if enabled {
        current | WS_EX_NOACTIVATE
    } else {
        current & !WS_EX_NOACTIVATE
    };

    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
    }

    let ok = unsafe {
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
    };

    if ok == 0 {
        return Err("창 스타일을 반영하지 못했습니다.".into());
    }

    Ok(())
}

fn restore_window_ex_style(hwnd: Hwnd, ex_style: isize) -> Result<(), String> {
    if !is_window(hwnd) {
        return Ok(());
    }

    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
    }

    let ok = unsafe {
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
    };

    if ok == 0 {
        return Err("창 스타일을 복원하지 못했습니다.".into());
    }

    Ok(())
}

fn restore_focus_target_window(target: FocusTarget) -> Result<(), String> {
    let mut first_error = None;

    for snapshot in target.original_child_ex_styles {
        if let Err(error) = restore_window_ex_style(snapshot.hwnd, snapshot.ex_style) {
            first_error.get_or_insert(error);
        }
    }

    if let Err(error) = restore_window_ex_style(target.info.hwnd, target.original_ex_style) {
        first_error.get_or_insert(error);
    }

    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn capture_child_ex_styles(hwnd: Hwnd) -> Vec<WindowExStyleSnapshot> {
    let mut snapshots = Vec::new();
    unsafe {
        EnumChildWindows(
            hwnd,
            Some(enum_child_ex_style_snapshot_callback),
            &mut snapshots as *mut _ as isize,
        );
    }
    snapshots
}

fn remember_child_ex_styles(target: &mut FocusTarget) {
    for snapshot in capture_child_ex_styles(target.info.hwnd) {
        if !target
            .original_child_ex_styles
            .iter()
            .any(|existing| existing.hwnd == snapshot.hwnd)
        {
            target.original_child_ex_styles.push(snapshot);
        }
    }
}

unsafe extern "system" fn enum_child_ex_style_snapshot_callback(hwnd: Hwnd, lparam: isize) -> Bool {
    let snapshots = &mut *(lparam as *mut Vec<WindowExStyleSnapshot>);
    snapshots.push(WindowExStyleSnapshot {
        hwnd,
        ex_style: GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
    });
    1
}

fn set_noactivate_tree(hwnd: Hwnd, enabled: bool) -> Result<(), String> {
    set_noactivate(hwnd, enabled)?;
    unsafe {
        EnumChildWindows(
            hwnd,
            Some(enum_child_noactivate_callback),
            if enabled { 1 } else { 0 },
        );
    }
    Ok(())
}

unsafe extern "system" fn enum_child_noactivate_callback(hwnd: Hwnd, lparam: isize) -> Bool {
    let _ = set_noactivate(hwnd, lparam != 0);
    1
}

fn set_topmost(hwnd: Hwnd, enabled: bool) -> Result<(), String> {
    ensure_window(hwnd)?;
    let insert_after = if enabled {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    let ok = unsafe {
        SetWindowPos(
            hwnd,
            insert_after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };

    if ok == 0 {
        return Err("항상 위 상태를 변경하지 못했습니다.".into());
    }

    Ok(())
}

fn force_foreground_window(hwnd: Hwnd) {
    if !is_window(hwnd) {
        return;
    }

    unsafe {
        let current_thread = GetCurrentThreadId();
        let mut target_pid = 0;
        let target_thread = GetWindowThreadProcessId(hwnd, &mut target_pid);
        let foreground = GetForegroundWindow();
        let mut foreground_pid = 0;
        let foreground_thread = if foreground != 0 {
            GetWindowThreadProcessId(foreground, &mut foreground_pid)
        } else {
            0
        };

        let attach_target = target_thread != 0 && target_thread != current_thread;
        let attach_foreground = foreground_thread != 0 && foreground_thread != current_thread;

        if attach_foreground {
            AttachThreadInput(current_thread, foreground_thread, 1);
        }
        if attach_target {
            AttachThreadInput(current_thread, target_thread, 1);
        }

        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        SetActiveWindow(hwnd);
        SetFocus(hwnd);

        if attach_target {
            AttachThreadInput(current_thread, target_thread, 0);
        }
        if attach_foreground {
            AttachThreadInput(current_thread, foreground_thread, 0);
        }
    }
}

fn is_topmost(hwnd: Hwnd) -> bool {
    if !is_window(hwnd) {
        return false;
    }
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    style & WS_EX_TOPMOST != 0
}

fn ensure_window(hwnd: Hwnd) -> Result<(), String> {
    if is_window(hwnd) {
        Ok(())
    } else {
        Err("대상 창이 더 이상 존재하지 않습니다.".into())
    }
}

fn is_window(hwnd: Hwnd) -> bool {
    hwnd != 0 && unsafe { IsWindow(hwnd) != 0 }
}

fn root_window(hwnd: Hwnd) -> Hwnd {
    if hwnd == 0 {
        return 0;
    }

    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root == 0 {
        hwnd
    } else {
        root
    }
}

fn same_root_window(left: Hwnd, right: Hwnd) -> bool {
    left != 0 && right != 0 && root_window(left) == root_window(right)
}

fn find_own_window() -> Result<Hwnd, String> {
    find_process_window(std::process::id(), Some(APP_WINDOW_TITLE))
        .ok_or_else(|| "보조 창 HWND를 찾지 못했습니다.".into())
}

fn window_hwnd(window: &tauri::WebviewWindow) -> Result<Hwnd, String> {
    window
        .hwnd()
        .map(|hwnd| hwnd.0 as isize)
        .or_else(|_| find_own_window())
}

struct FindWindowQuery {
    pid: u32,
    title: Option<String>,
    result: Hwnd,
}

fn find_process_window(pid: u32, title: Option<&str>) -> Option<Hwnd> {
    let mut query = FindWindowQuery {
        pid,
        title: title.map(ToOwned::to_owned),
        result: 0,
    };

    unsafe {
        EnumWindows(Some(enum_windows_callback), &mut query as *mut _ as isize);
    }

    if query.result == 0 {
        None
    } else {
        Some(query.result)
    }
}

unsafe extern "system" fn enum_windows_callback(hwnd: Hwnd, lparam: isize) -> Bool {
    let query = &mut *(lparam as *mut FindWindowQuery);
    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }

    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid != query.pid {
        return 1;
    }

    if let Some(expected_title) = query.title.as_ref() {
        let title = window_title(hwnd);
        if title != *expected_title {
            return 1;
        }
    }

    query.result = hwnd;
    0
}

fn window_info(hwnd: Hwnd) -> Result<WindowInfo, String> {
    ensure_window(hwnd)?;
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    Ok(WindowInfo {
        hwnd,
        title: window_title(hwnd),
        pid,
    })
}

struct WindowListQuery {
    own_pid: u32,
    windows: Vec<WindowInfo>,
}

struct WindowAtPointQuery {
    own_pid: u32,
    x: i32,
    y: i32,
    result: Option<WindowPick>,
}

fn enumerate_available_windows() -> Vec<WindowInfo> {
    let mut query = WindowListQuery {
        own_pid: std::process::id(),
        windows: Vec::new(),
    };

    unsafe {
        EnumWindows(
            Some(enum_window_list_callback),
            &mut query as *mut _ as isize,
        );
    }

    query.windows.sort_by(|a, b| a.title.cmp(&b.title));
    query.windows
}

unsafe extern "system" fn enum_window_list_callback(hwnd: Hwnd, lparam: isize) -> Bool {
    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }

    let query = &mut *(lparam as *mut WindowListQuery);
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == query.own_pid {
        return 1;
    }

    let title = window_title(hwnd);
    if is_unselectable_window_title(&title) {
        return 1;
    }

    query.windows.push(WindowInfo { hwnd, title, pid });
    1
}

fn cursor_position() -> Option<(i32, i32)> {
    let mut point = PointRaw::default();
    let ok = unsafe { GetCursorPos(&mut point) };
    if ok == 0 {
        None
    } else {
        Some((point.x, point.y))
    }
}

fn wait_until_key_up(v_key: i32) {
    for _ in 0..150 {
        if !is_key_down(v_key) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn is_key_down(v_key: i32) -> bool {
    unsafe { GetAsyncKeyState(v_key) as u16 & 0x8000 != 0 }
}

fn is_key_pressed_or_down(v_key: i32) -> bool {
    let state = unsafe { GetAsyncKeyState(v_key) as u16 };
    state & 0x8001 != 0
}

enum PickerMouseEvent {
    Left { x: i32, y: i32 },
    Right,
}

struct MouseClickHook {
    hook: Hhook,
}

impl MouseClickHook {
    fn new() -> Self {
        PICKER_MOUSE_EVENT.store(0, Ordering::SeqCst);
        PICKER_MOUSE_X.store(0, Ordering::SeqCst);
        PICKER_MOUSE_Y.store(0, Ordering::SeqCst);

        let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), 0, 0) };
        Self { hook }
    }
}

impl Drop for MouseClickHook {
    fn drop(&mut self) {
        if self.hook != 0 {
            unsafe {
                UnhookWindowsHookEx(self.hook);
            }
        }
    }
}

unsafe extern "system" fn mouse_hook_proc(code: i32, w_param: usize, l_param: isize) -> isize {
    if code >= 0 && l_param != 0 {
        let event = w_param as u32;
        if event == WM_LBUTTONDOWN || event == WM_RBUTTONDOWN {
            let mouse = &*(l_param as *const MouseHookRaw);
            PICKER_MOUSE_X.store(mouse.pt.x, Ordering::SeqCst);
            PICKER_MOUSE_Y.store(mouse.pt.y, Ordering::SeqCst);
            PICKER_MOUSE_EVENT.store(
                if event == WM_LBUTTONDOWN { 1 } else { 2 },
                Ordering::SeqCst,
            );
        }
    }

    CallNextHookEx(0, code, w_param, l_param)
}

fn take_picker_mouse_event() -> Option<PickerMouseEvent> {
    match PICKER_MOUSE_EVENT.swap(0, Ordering::SeqCst) {
        1 => Some(PickerMouseEvent::Left {
            x: PICKER_MOUSE_X.load(Ordering::SeqCst),
            y: PICKER_MOUSE_Y.load(Ordering::SeqCst),
        }),
        2 => Some(PickerMouseEvent::Right),
        _ => None,
    }
}

fn pump_thread_messages() {
    let mut message = MsgRaw::default();
    while unsafe { PeekMessageW(&mut message, 0, 0, 0, PM_REMOVE) } != 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

struct HoverOutline {
    windows: [Hwnd; 4],
    current: Option<WindowRect>,
}

impl HoverOutline {
    fn new() -> Self {
        let mut outline = Self {
            windows: [0; 4],
            current: None,
        };

        let class_name = wide_null("STATIC");
        let window_name = wide_null("");
        let ex_style = WS_EX_LAYERED
            | WS_EX_TRANSPARENT_MOUSE
            | WS_EX_TOOLWINDOW
            | WS_EX_TOPMOST as u32
            | WS_EX_NOACTIVATE as u32;
        let style = WS_POPUP | WS_VISIBLE | SS_WHITERECT;

        for hwnd in outline.windows.iter_mut() {
            *hwnd = unsafe {
                CreateWindowExW(
                    ex_style,
                    class_name.as_ptr(),
                    window_name.as_ptr(),
                    style,
                    -32000,
                    -32000,
                    1,
                    1,
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };

            if *hwnd != 0 {
                unsafe {
                    SetLayeredWindowAttributes(*hwnd, 0, HOVER_BORDER_ALPHA, LWA_ALPHA);
                    ShowWindow(*hwnd, SW_HIDE);
                }
            }
        }

        outline
    }

    fn update(&mut self, next: Option<&WindowRect>) {
        if self.current.as_ref() == next {
            return;
        }

        if let Some(rect) = next {
            self.show(rect);
            self.current = Some(rect.clone());
        } else {
            self.hide();
        }
    }

    fn show(&self, rect: &WindowRect) {
        let border = HOVER_BORDER_SIZE;
        let segments = [
            (
                rect.left - border,
                rect.top - border,
                rect.width + border * 2,
                border,
            ),
            (
                rect.left - border,
                rect.top + rect.height,
                rect.width + border * 2,
                border,
            ),
            (rect.left - border, rect.top, border, rect.height),
            (rect.left + rect.width, rect.top, border, rect.height),
        ];

        for (hwnd, (x, y, width, height)) in self.windows.iter().zip(segments) {
            if *hwnd == 0 {
                continue;
            }

            unsafe {
                SetWindowPos(
                    *hwnd,
                    HWND_TOPMOST,
                    x,
                    y,
                    width.max(1),
                    height.max(1),
                    SWP_NOACTIVATE,
                );
                ShowWindow(*hwnd, SW_SHOWNOACTIVATE);
                UpdateWindow(*hwnd);
            }
        }
    }

    fn hide(&mut self) {
        for hwnd in self.windows {
            if hwnd != 0 {
                unsafe {
                    ShowWindow(hwnd, SW_HIDE);
                }
            }
        }
        self.current = None;
    }
}

impl Drop for HoverOutline {
    fn drop(&mut self) {
        for hwnd in self.windows {
            if hwnd != 0 {
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
        }
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn window_at_point(x: i32, y: i32) -> Option<WindowPick> {
    if let Some(pick) = window_from_point_pick(x, y) {
        return Some(pick);
    }

    let mut query = WindowAtPointQuery {
        own_pid: std::process::id(),
        x,
        y,
        result: None,
    };

    unsafe {
        EnumWindows(
            Some(enum_window_at_point_callback),
            &mut query as *mut _ as isize,
        );
    }

    query.result
}

fn window_from_point_pick(x: i32, y: i32) -> Option<WindowPick> {
    let point = PointRaw { x, y };
    let hwnd = unsafe { WindowFromPoint(point) };
    if hwnd == 0 {
        return None;
    }

    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    let hwnd = if root != 0 { root } else { hwnd };
    selectable_window_pick(hwnd, x, y)
}

fn selectable_window_pick(hwnd: Hwnd, x: i32, y: i32) -> Option<WindowPick> {
    if hwnd == 0 || unsafe { IsWindowVisible(hwnd) } == 0 {
        return None;
    }

    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == std::process::id() {
        return None;
    }

    let rect = window_rect(hwnd)?;
    if !rect_contains(&rect, x, y) {
        return None;
    }

    let title = window_title(hwnd);
    if is_unselectable_window_title(&title) {
        return None;
    }

    Some(WindowPick {
        info: WindowInfo { hwnd, title, pid },
        rect,
    })
}

unsafe extern "system" fn enum_window_at_point_callback(hwnd: Hwnd, lparam: isize) -> Bool {
    let query = &mut *(lparam as *mut WindowAtPointQuery);
    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }

    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == query.own_pid {
        return 1;
    }

    let Some(rect) = window_rect(hwnd) else {
        return 1;
    };
    if !rect_contains(&rect, query.x, query.y) {
        return 1;
    }

    let title = window_title(hwnd);
    if is_unselectable_window_title(&title) {
        return 1;
    }

    query.result = Some(WindowPick {
        info: WindowInfo { hwnd, title, pid },
        rect,
    });
    0
}

fn is_unselectable_window_title(title: &str) -> bool {
    title == "제목 없는 창" || title == WINDOW_PICKER_TITLE || title.trim().is_empty()
}

fn window_rect(hwnd: Hwnd) -> Option<WindowRect> {
    let mut rect = RectRaw::default();
    let ok = unsafe { GetWindowRect(hwnd, &mut rect) };
    if ok == 0 {
        return None;
    }

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 32 || height < 32 {
        return None;
    }

    Some(WindowRect {
        left: rect.left,
        top: rect.top,
        width,
        height,
    })
}

fn rect_contains(rect: &WindowRect, x: i32, y: i32) -> bool {
    x >= rect.left && y >= rect.top && x < rect.left + rect.width && y < rect.top + rect.height
}

fn window_title(hwnd: Hwnd) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return "제목 없는 창".into();
    }

    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    if copied <= 0 {
        return "제목 없는 창".into();
    }

    String::from_utf16_lossy(&buffer[..copied as usize])
}

fn settings_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    base.join("MapleUtils").join("settings.json")
}

fn load_settings() -> Settings {
    let path = settings_path();
    let Ok(text) = fs::read_to_string(path) else {
        return Settings::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Settings::default();
    };
    let missing_named_presets = value.get("filter_presets").is_none();
    let mut settings: Settings = serde_json::from_value(value).unwrap_or_default();
    if settings.filter_on_preset.is_legacy_default() {
        settings.filter_on_preset = FilterPreset::default();
    }
    settings.focus_guard_poll_ms = sanitize_focus_guard_poll_ms(settings.focus_guard_poll_ms);
    if missing_named_presets && settings.filter_presets.is_empty() {
        settings.filter_presets.push(NamedFilterPreset {
            name: "기본".into(),
            preset: settings.filter_on_preset.clone(),
        });
    }
    settings
}

fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())
}
