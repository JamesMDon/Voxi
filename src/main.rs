// Voxi - lightweight Windows SAPI tray app

#![windows_subsystem = "windows"]

use arboard::Clipboard;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::Speech::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

mod menu_icons;
mod natural;
mod playback;
mod text;

const APP_NAME: PCWSTR = w!("Voxi");
const APP_CLASS: PCWSTR = w!("Voxi_Class");
const APP_MUTEX: PCWSTR = w!("Local\\Voxi.SingleInstance");
const APP_ICON_ACTIVE_ID: usize = 1;
const APP_ICON_IDLE_ID: usize = 2;

const HK_READ: i32 = 1;
const HK_SPEED: i32 = 2;
const HK_VOICE: i32 = 3;
const HK_EXIT: i32 = 4;
const HOTKEY_IDS: [i32; 4] = [HK_READ, HK_SPEED, HK_VOICE, HK_EXIT];

const VK_1: u32 = 0x31;
const VK_2: u32 = 0x32;
const VK_3: u32 = 0x33;
const VK_4: u32 = 0x34;

struct Speed {
    rate: i32,
    label: &'static str,
}

const SPEEDS: [Speed; 3] = [
    Speed {
        rate: 0,
        label: "Slow",
    },
    Speed {
        rate: 5,
        label: "Mid",
    },
    Speed {
        rate: 10,
        label: "Fast",
    },
];
const DEFAULT_SPEED_IDX: usize = 2;

const WM_TRAY_ICON: u32 = WM_USER + 1;
const ID_TRAY_ICON: u32 = 1001;
const ID_TIMER_CHECK: usize = 1002;

const IDM_READ: usize = 2000;
const IDM_NEXT_SPEED: usize = 2001;
const IDM_NEXT_VOICE: usize = 2002;
const IDM_EXIT: usize = 2003;
const IDM_STOP: usize = 2004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadAction {
    Read,
    Stop,
}

impl ReadAction {
    fn menu_item(self) -> (usize, &'static str) {
        match self {
            Self::Read => (IDM_READ, "Alt+1 | Read"),
            Self::Stop => (IDM_STOP, "Alt+1 | Stop"),
        }
    }
}

const SPRS_IS_SPEAKING: u32 = 2;
const SPF_PURGE: u32 = 2;
const SPEECH_START_GRACE: Duration = Duration::from_millis(750);

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SpeechKind {
    #[default]
    Reading,
    Announcement,
}

#[derive(Default)]
struct SpeechActivity {
    requested_at: Option<Instant>,
    observed_running: bool,
    kind: SpeechKind,
}

impl SpeechActivity {
    fn begin(&mut self, kind: SpeechKind, now: Instant) {
        self.requested_at = Some(now);
        self.observed_running = false;
        self.kind = kind;
    }

    fn stop(&mut self) {
        self.requested_at = None;
        self.observed_running = false;
    }

    fn is_active(&self) -> bool {
        self.requested_at.is_some()
    }

    fn is_reading(&self) -> bool {
        self.is_active() && self.kind == SpeechKind::Reading
    }

    fn read_action(&self) -> ReadAction {
        if self.is_active() {
            ReadAction::Stop
        } else {
            ReadAction::Read
        }
    }

    fn observe(&mut self, sapi_is_running: bool, in_tail: bool, now: Instant) -> bool {
        let Some(requested_at) = self.requested_at else {
            return false;
        };

        // The device can still be playing protective silence after the speech
        // has finished. Presentation and controls are already idle at that point.
        if in_tail {
            self.stop();
            return true;
        }

        if sapi_is_running {
            self.observed_running = true;
            return false;
        }

        if self.observed_running
            || now.saturating_duration_since(requested_at) >= SPEECH_START_GRACE
        {
            self.stop();
            return true;
        }

        false
    }
}

struct AppState {
    voices: Vec<VoiceChoice>,
    voice_idx: usize,
    speed_idx: usize,
    speech: SpeechActivity,
    playback: Option<playback::Playback>,
    open_menu: Option<OpenMenu>,
    idle_icon: HICON,
    active_icon: HICON,
    taskbar_created_message: u32,
    _natural_runtime: Option<natural::NaturalRuntime>,
}

struct VoiceChoice {
    engine: ISpVoice,
    _token: ISpObjectToken,
    name: String,
    natural: bool,
}

struct OpenMenu {
    handle: HMENU,
    icons: menu_icons::MenuIcons,
}

thread_local! {
    static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

struct StateGuard;

impl Drop for StateGuard {
    fn drop(&mut self) {
        STATE.with(|cell| {
            cell.borrow_mut().take();
        });
    }
}

struct ComGuard;

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct InstanceGuard(HANDLE);

impl InstanceGuard {
    unsafe fn acquire() -> Result<Option<Self>> {
        let handle = CreateMutexW(None, false, APP_MUTEX)?;
        let already_running = matches!(
            GetLastError(),
            Err(error) if error.code() == HRESULT::from_win32(ERROR_ALREADY_EXISTS.0)
        );

        if already_running {
            CloseHandle(handle)?;
            Ok(None)
        } else {
            Ok(Some(Self(handle)))
        }
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct MenuGuard(HMENU);

impl Drop for MenuGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

fn with_state<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut AppState) -> R,
{
    STATE.with(|cell| cell.borrow_mut().as_mut().map(f))
}

fn main() {
    if let Err(error) = run() {
        show_message(
            None,
            "Voxi could not start",
            &error.to_string(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn run() -> Result<()> {
    unsafe {
        let Some(_instance_guard) = InstanceGuard::acquire()? else {
            return Ok(());
        };

        text::initialize();

        CoInitialize(None)?;
        let _com_guard = ComGuard;

        let instance = GetModuleHandleW(None)?;
        let hinstance: HINSTANCE = instance.into();
        let idle_icon = LoadIconW(hinstance, icon_resource(APP_ICON_IDLE_ID))?;
        let active_icon = LoadIconW(hinstance, icon_resource(APP_ICON_ACTIVE_ID))?;
        let taskbar_created_message = RegisterWindowMessageW(w!("TaskbarCreated"));
        if taskbar_created_message == 0 {
            return Err(Error::from_win32());
        }

        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            hIcon: idle_icon,
            lpszClassName: APP_CLASS,
            ..Default::default()
        };
        if RegisterClassW(&wnd_class) == 0 {
            return Err(Error::from_win32());
        }

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            APP_CLASS,
            APP_NAME,
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        );
        if hwnd.0 == 0 {
            return Err(Error::from_win32());
        }

        let (voices, natural_runtime) = load_voices(SPEEDS[DEFAULT_SPEED_IDX].rate)?;

        STATE.with(|cell| {
            *cell.borrow_mut() = Some(AppState {
                voices,
                voice_idx: 0,
                speed_idx: DEFAULT_SPEED_IDX,
                speech: SpeechActivity::default(),
                playback: None,
                open_menu: None,
                idle_icon,
                active_icon,
                taskbar_created_message,
                _natural_runtime: natural_runtime,
            });
        });
        let _state_guard = StateGuard;

        let modifiers = MOD_ALT | MOD_NOREPEAT;
        RegisterHotKey(hwnd, HK_READ, modifiers, VK_1)?;
        RegisterHotKey(hwnd, HK_SPEED, modifiers, VK_2)?;
        RegisterHotKey(hwnd, HK_VOICE, modifiers, VK_3)?;
        RegisterHotKey(hwnd, HK_EXIT, modifiers, VK_4)?;

        if SetTimer(hwnd, ID_TIMER_CHECK, 100, None) == 0 {
            return Err(Error::from_win32());
        }

        with_state(|state| init_tray(hwnd, state)).transpose()?;
        let loop_result = message_loop();

        let _ = KillTimer(hwnd, ID_TIMER_CHECK);
        for id in HOTKEY_IDS {
            let _ = UnregisterHotKey(hwnd, id);
        }
        let _ = Shell_NotifyIconW(NIM_DELETE, &get_nid(hwnd));

        loop_result
    }
}

unsafe fn load_voices(
    default_rate: i32,
) -> Result<(Vec<VoiceChoice>, Option<natural::NaturalRuntime>)> {
    let mut voices = Vec::with_capacity(2);
    let mut natural_runtime = None;

    if let Ok(Some(voice)) = natural::load_guy(default_rate) {
        let natural::NaturalVoice {
            engine,
            token,
            runtime,
            ..
        } = voice;
        voices.push(VoiceChoice {
            engine,
            _token: token,
            name: "MS Guy".to_owned(),
            natural: true,
        });
        natural_runtime = Some(runtime);
    }

    let category: ISpObjectTokenCategory =
        CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL)?;
    category.SetId(
        w!("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Speech\\Voices"),
        false,
    )?;
    let token_enum = category.EnumTokens(None, None)?;
    let mut count = 0;
    token_enum.GetCount(&mut count)?;

    let mut eva = None;
    for index in 0..count {
        let token = token_enum.Item(index)?;
        let name = token_name(&token)?;
        if name.to_lowercase().contains("eva") {
            eva = Some((name, token));
            break;
        }
    }

    if let Some((name, token)) = eva {
        voices.push(make_system_voice(name, token, default_rate)?);
    }

    if voices.is_empty() {
        return Err(Error::new(
            E_FAIL,
            "Neither Microsoft Guy nor Microsoft Eva is available.".into(),
        ));
    }

    Ok((voices, natural_runtime))
}

unsafe fn token_name(token: &ISpObjectToken) -> Result<String> {
    let value = token.GetStringValue(None)?;
    let name = value.to_string();
    CoTaskMemFree(Some(value.as_ptr().cast()));
    Ok(name?)
}

unsafe fn make_system_voice(name: String, token: ISpObjectToken, rate: i32) -> Result<VoiceChoice> {
    let engine: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL)?;
    let audio: ISpAudio = CoCreateInstance(&SpMMAudioOut, None, CLSCTX_ALL)?;
    engine.SetOutput(&audio, true)?;
    engine.SetVoice(&token)?;
    engine.SetRate(rate)?;
    Ok(VoiceChoice {
        engine,
        _token: token,
        name: friendly_voice_name(&name),
        natural: false,
    })
}

unsafe fn message_loop() -> Result<()> {
    let mut msg = MSG::default();
    loop {
        let result = GetMessageW(&mut msg, None, 0, 0);
        if result.0 == -1 {
            return Err(Error::from_win32());
        }
        if result.0 == 0 {
            return Ok(());
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if with_state(|state| msg == state.taskbar_created_message).unwrap_or(false) {
        let result = with_state(|state| init_tray(hwnd, state).map_err(|error| error.to_string()));
        report_action_result(hwnd, "Could not restore the tray icon", result);
        return LRESULT(0);
    }

    match msg {
        WM_TIMER => {
            if wparam.0 == ID_TIMER_CHECK {
                check_icon_state(hwnd);
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            match wparam.0 as i32 {
                HK_READ => control_reading(hwnd, None),
                HK_SPEED => cycle_speed(hwnd),
                HK_VOICE => cycle_voice(hwnd),
                HK_EXIT => request_exit(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_TRAY_ICON => {
            if lparam.0 as u32 == WM_LBUTTONUP {
                control_reading(hwnd, None);
            } else if lparam.0 as u32 == WM_RBUTTONUP {
                let result = show_context_menu(hwnd).map_err(|error| error.to_string());
                report_action_result(hwnd, "Could not open the Voxi menu", Some(result));
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 & 0xFFFF {
                IDM_READ => control_reading(hwnd, Some(ReadAction::Read)),
                IDM_STOP => control_reading(hwnd, Some(ReadAction::Stop)),
                IDM_NEXT_SPEED => cycle_speed(hwnd),
                IDM_NEXT_VOICE => cycle_voice(hwnd),
                IDM_EXIT => request_exit(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn request_exit(hwnd: HWND) {
    if let Err(error) = DestroyWindow(hwnd) {
        show_message(
            Some(hwnd),
            "Voxi",
            &format!("Could not exit Voxi.\n\n{error}"),
            MB_OK | MB_ICONERROR,
        );
    }
}

unsafe fn check_icon_state(hwnd: HWND) {
    with_state(|state| {
        let _ = refresh_speech_state(hwnd, state);
    });
}

// The timer and controls share the same speech state. Refresh on input too, so
// a hotkey at the speech/tail boundary does not depend on the next timer tick.
unsafe fn refresh_speech_state(hwnd: HWND, state: &mut AppState) -> Result<Option<SPVOICESTATUS>> {
    if state.playback.is_none() && !state.speech.is_active() {
        return Ok(None);
    }

    let mut status = SPVOICESTATUS::default();
    state.voices[state.voice_idx]
        .engine
        .GetStatus(&mut status, std::ptr::null_mut())?;
    let in_tail = state
        .playback
        .as_ref()
        .is_some_and(|playback| playback.is_in_tail(&status));
    let changed = state.speech.observe(
        status.dwRunningState == SPRS_IS_SPEAKING,
        in_tail,
        Instant::now(),
    );
    // Becoming idle must not cancel or release the queued protective tail.
    if status.dwRunningState == SPRS_DONE.0 as u32 && !state.speech.is_active() {
        state.playback = None;
    }
    if changed {
        update_tray(hwnd, state)?;
    }
    Ok(Some(status))
}

unsafe fn control_reading(hwnd: HWND, requested: Option<ReadAction>) {
    let result = with_state(|state| -> std::result::Result<(), String> {
        let _ = refresh_speech_state(hwnd, state);
        // Menu commands retain their displayed intent even if speech finishes
        // between rendering the item and clicking it. Hotkeys and tray clicks toggle.
        let action = requested.unwrap_or_else(|| state.speech.read_action());
        if action == ReadAction::Stop {
            state.voices[state.voice_idx]
                .engine
                .Speak(None, SPF_PURGE, None)
                .map_err(|error| error.to_string())?;
            state.speech.stop();
            state.playback = None;
            update_tray(hwnd, state).map_err(|error| error.to_string())?;
            return Ok(());
        }

        let mut clipboard = Clipboard::new().map_err(|error| error.to_string())?;
        let Some(clipboard_text) = readable_clipboard_text(clipboard.get_text())? else {
            return Ok(());
        };
        if !clipboard_text.trim().is_empty() {
            speak_text_inner(hwnd, state, &clipboard_text, SpeechKind::Reading)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    });
    report_action_result(hwnd, "Could not read the clipboard", result);
}

fn readable_clipboard_text(
    result: std::result::Result<String, arboard::Error>,
) -> std::result::Result<Option<String>, String> {
    match result {
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

unsafe fn cycle_voice(hwnd: HWND) {
    let result = with_state(|state| -> std::result::Result<(), String> {
        if state.voices.is_empty() {
            return Err("No voices are available.".to_owned());
        }

        state.voices[state.voice_idx]
            .engine
            .Speak(None, SPF_PURGE, None)
            .map_err(|error| error.to_string())?;
        state.speech.stop();
        state.playback = None;

        let next_idx = (state.voice_idx + 1) % state.voices.len();
        state.voice_idx = next_idx;
        state.voices[next_idx]
            .engine
            .SetRate(SPEEDS[state.speed_idx].rate)
            .map_err(|error| error.to_string())?;
        let voice_name = &state.voices[next_idx].name;
        let name = voice_name
            .strip_prefix("MS ")
            .unwrap_or(voice_name)
            .to_owned();
        speak_text_inner(hwnd, state, &name, SpeechKind::Announcement)
            .map_err(|error| error.to_string())
    });
    report_action_result(hwnd, "Could not change the voice", result);
}

unsafe fn cycle_speed(hwnd: HWND) {
    let result = with_state(|state| -> std::result::Result<(), String> {
        let status = refresh_speech_state(hwnd, state).map_err(|error| error.to_string())?;
        let reading = state.speech.is_reading();
        let remaining = if reading && state.voices[state.voice_idx].natural {
            state
                .playback
                .as_ref()
                .zip(status.as_ref())
                .and_then(|(playback, status)| playback.remaining_text(status))
                .map(str::to_owned)
        } else {
            None
        };
        let next_idx = (state.speed_idx + 1) % SPEEDS.len();
        let speed = &SPEEDS[next_idx];
        state.voices[state.voice_idx]
            .engine
            .SetRate(speed.rate)
            .map_err(|error| error.to_string())?;
        state.speed_idx = next_idx;
        if reading {
            // Guy cannot change rate mid-utterance. Resume its current word;
            // Eva applies SetRate directly. Never interrupt with an announcement.
            if let Some(remaining) = remaining {
                speak_processed_text(hwnd, state, &remaining, SpeechKind::Reading)
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        } else {
            // Announcements are replaceable feedback, never resumable reading.
            speak_text_inner(hwnd, state, speed.label, SpeechKind::Announcement)
                .map_err(|error| error.to_string())
        }
    });
    report_action_result(hwnd, "Could not change the speech speed", result);
}

unsafe fn speak_text_inner(
    hwnd: HWND,
    state: &mut AppState,
    value: &str,
    kind: SpeechKind,
) -> Result<()> {
    let processed = text::to_plain_text(value);
    if processed.trim().is_empty() {
        return Ok(());
    }
    speak_processed_text(hwnd, state, &processed, kind)
}

unsafe fn speak_processed_text(
    hwnd: HWND,
    state: &mut AppState,
    processed: &str,
    kind: SpeechKind,
) -> Result<()> {
    let choice = &state.voices[state.voice_idx];
    let result = playback::start(&choice.engine, choice.natural, processed);
    let playback = match result {
        Ok(playback) => playback,
        Err(error) => {
            if !state.voices[state.voice_idx].natural {
                return Err(error);
            }

            let Some(fallback_idx) = state.voices.iter().position(|voice| !voice.natural) else {
                return Err(error);
            };
            state.voice_idx = fallback_idx;
            state.voices[fallback_idx]
                .engine
                .SetRate(SPEEDS[state.speed_idx].rate)?;
            playback::start(&state.voices[fallback_idx].engine, false, processed)?
        }
    };

    state.playback = Some(playback);
    state.speech.begin(kind, Instant::now());
    update_tray(hwnd, state)
}

unsafe fn update_tray(hwnd: HWND, state: &AppState) -> Result<()> {
    if let Some(menu) = &state.open_menu {
        update_read_menu_item(menu.handle, state.speech.read_action(), &menu.icons)?;
        let _ = DrawMenuBar(hwnd);
    }
    let mut nid = get_nid(hwnd);
    nid.uFlags = NIF_ICON | NIF_TIP;
    apply_tray_appearance(&mut nid, state);
    Shell_NotifyIconW(NIM_MODIFY, &nid).ok()
}

unsafe fn update_read_menu_item(
    menu: HMENU,
    action: ReadAction,
    icons: &menu_icons::MenuIcons,
) -> Result<()> {
    let (command, label) = action.menu_item();
    let mut text = wide_null(label);
    SetMenuItemInfoW(
        menu,
        0,
        true,
        &MENUITEMINFOW {
            cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_STRING | MIIM_ID | MIIM_BITMAP,
            wID: command as u32,
            dwTypeData: PWSTR(text.as_mut_ptr()),
            hbmpItem: icons.read_bitmap(action == ReadAction::Stop),
            ..Default::default()
        },
    )
}

unsafe fn init_tray(hwnd: HWND, state: &AppState) -> Result<()> {
    let mut nid = get_nid(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY_ICON;
    apply_tray_appearance(&mut nid, state);
    Shell_NotifyIconW(NIM_ADD, &nid).ok()
}

fn apply_tray_appearance(nid: &mut NOTIFYICONDATAW, state: &AppState) {
    nid.hIcon = if state.speech.is_active() {
        state.active_icon
    } else {
        state.idle_icon
    };
    let tip = if state.speech.is_active() {
        "Voxi: speaking"
    } else {
        "Voxi: ready"
    };
    set_tray_tip(nid, tip);
}

fn set_tray_tip(nid: &mut NOTIFYICONDATAW, tip: &str) {
    nid.szTip.fill(0);
    let max_length = nid.szTip.len().saturating_sub(1);
    for (destination, source) in nid
        .szTip
        .iter_mut()
        .take(max_length)
        .zip(tip.encode_utf16())
    {
        *destination = source;
    }
}

fn get_nid(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID_TRAY_ICON,
        ..Default::default()
    }
}

unsafe fn show_context_menu(hwnd: HWND) -> Result<()> {
    let menu = MenuGuard(CreatePopupMenu()?);
    let (voice_name, speed, read_action) = with_state(|state| {
        let _ = refresh_speech_state(hwnd, state);
        let voice_name = state
            .voices
            .get(state.voice_idx)
            .map(|voice| voice.name.clone())
            .unwrap_or_else(|| "Default".to_owned());
        (
            voice_name.replace('&', "&&"),
            SPEEDS[state.speed_idx].label,
            state.speech.read_action(),
        )
    })
    .unwrap_or_else(|| {
        (
            "Unavailable".to_owned(),
            SPEEDS[DEFAULT_SPEED_IDX].label,
            ReadAction::Read,
        )
    });
    let (read_command, read_label) = read_action.menu_item();
    let speak_wide = wide_null(read_label);
    let speed_wide = wide_null(&format!("Alt+2 | {speed}"));
    let voice_wide = wide_null(&format!("Alt+3 | {voice_name}"));
    let exit_wide = wide_null("Alt+4 | Exit");

    AppendMenuW(menu.0, MF_STRING, read_command, PCWSTR(speak_wide.as_ptr()))?;
    AppendMenuW(
        menu.0,
        MF_STRING,
        IDM_NEXT_SPEED,
        PCWSTR(speed_wide.as_ptr()),
    )?;
    AppendMenuW(
        menu.0,
        MF_STRING,
        IDM_NEXT_VOICE,
        PCWSTR(voice_wide.as_ptr()),
    )?;
    AppendMenuW(menu.0, MF_STRING, IDM_EXIT, PCWSTR(exit_wide.as_ptr()))?;

    let menu_icons = menu_icons::MenuIcons::install(
        menu.0,
        [read_command, IDM_NEXT_SPEED, IDM_NEXT_VOICE, IDM_EXIT],
    )?;
    update_read_menu_item(menu.0, read_action, &menu_icons)?;

    let mut point = POINT::default();
    GetCursorPos(&mut point)?;
    let _ = SetForegroundWindow(hwnd);
    with_state(|state| {
        state.open_menu = Some(OpenMenu {
            handle: menu.0,
            icons: menu_icons,
        })
    });
    let command = TrackPopupMenu(
        menu.0,
        TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
        point.x,
        point.y,
        0,
        hwnd,
        None,
    )
    .0 as usize;
    with_state(|state| state.open_menu = None);
    // With TPM_RETURNCMD, zero means dismissal, not a failed action.
    if command != 0 {
        PostMessageW(hwnd, WM_COMMAND, WPARAM(command), LPARAM(0))?;
    }
    PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0))?;
    Ok(())
}

fn friendly_voice_name(full_name: &str) -> String {
    let trimmed = full_name.trim();
    if let Some(microsoft_name) = trimmed.strip_prefix("Microsoft ") {
        let short_name = microsoft_name
            .split(|character: char| character.is_whitespace() || character == '(')
            .next()
            .unwrap_or("Voice")
            .trim_matches(|character: char| !character.is_alphanumeric());
        return format!("MS {short_name}");
    }

    let base_name = trimmed
        .split(" - ")
        .next()
        .unwrap_or(trimmed)
        .split(" (")
        .next()
        .unwrap_or(trimmed);
    let mut characters = base_name.chars();
    let shortened: String = characters.by_ref().take(20).collect();
    if characters.next().is_some() {
        format!("{shortened}…")
    } else if shortened.is_empty() {
        "Default".to_owned()
    } else {
        shortened
    }
}

fn report_action_result(
    hwnd: HWND,
    context: &str,
    result: Option<std::result::Result<(), String>>,
) {
    if let Some(Err(error)) = result {
        show_message(
            Some(hwnd),
            "Voxi",
            &format!("{context}.\n\n{error}"),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn show_message(hwnd: Option<HWND>, title: &str, message: &str, style: MESSAGEBOX_STYLE) {
    let title_wide = wide_null(title);
    let message_wide = wide_null(message);

    unsafe {
        MessageBoxW(
            hwnd.unwrap_or_default(),
            PCWSTR(message_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            style,
        );
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn icon_resource(id: usize) -> PCWSTR {
    PCWSTR(id as *const u16)
}

#[cfg(test)]
mod tests {
    use super::{
        friendly_voice_name, readable_clipboard_text, ReadAction, SpeechActivity, SpeechKind,
        SPEECH_START_GRACE,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn pending_speech_does_not_immediately_return_to_idle() {
        let started_at = Instant::now();
        let mut activity = SpeechActivity::default();
        activity.begin(SpeechKind::Reading, started_at);

        assert!(!activity.observe(false, false, started_at + Duration::from_millis(100)));
        assert!(activity.is_active());
        assert!(activity.observe(false, false, started_at + SPEECH_START_GRACE));
        assert!(!activity.is_active());
    }

    #[test]
    fn observed_speech_returns_to_idle_when_sapi_finishes() {
        let started_at = Instant::now();
        let mut activity = SpeechActivity::default();
        activity.begin(SpeechKind::Reading, started_at);

        assert!(!activity.observe(true, false, started_at + Duration::from_millis(100)));
        assert!(activity.is_active());
        assert!(activity.observe(false, false, started_at + Duration::from_millis(200)));
        assert!(!activity.is_active());
    }

    #[test]
    fn silent_tail_is_idle_even_while_the_audio_device_is_running() {
        let started_at = Instant::now();
        let mut activity = SpeechActivity::default();
        activity.begin(SpeechKind::Reading, started_at);
        assert!(!activity.observe(true, false, started_at + Duration::from_millis(100)));
        assert!(activity.observe(true, true, started_at + Duration::from_millis(200)));
        assert!(!activity.is_active());
        assert!(!activity.observe(true, true, started_at + Duration::from_millis(300)));
        assert!(!activity.is_active());

        // A speed announcement or new reading can immediately replace the tail.
        activity.begin(
            SpeechKind::Announcement,
            started_at + Duration::from_millis(400),
        );
        assert!(activity.is_active());
        assert!(!activity.observe(true, false, started_at + Duration::from_millis(500)));
        assert!(activity.is_active());
    }

    #[test]
    fn short_speech_can_reach_the_tail_before_the_first_status_poll() {
        let started_at = Instant::now();
        let mut activity = SpeechActivity::default();
        activity.begin(SpeechKind::Reading, started_at);
        assert!(activity.observe(true, true, started_at + Duration::from_millis(100)));
        assert!(!activity.is_active());
    }

    #[test]
    fn rapid_speed_announcements_are_replaceable_not_resumable() {
        let started_at = Instant::now();
        let mut activity = SpeechActivity::default();
        for press in 0..30 {
            let now = started_at + Duration::from_millis(press * 40);
            activity.begin(SpeechKind::Announcement, now);
            assert!(activity.is_active());
            assert!(!activity.is_reading());
            activity.observe(true, false, now + Duration::from_millis(10));
            assert!(!activity.is_reading());
        }
    }

    #[test]
    fn only_reading_content_is_resumed_when_speed_changes() {
        let now = Instant::now();
        let mut activity = SpeechActivity::default();
        activity.begin(SpeechKind::Reading, now);
        assert!(activity.is_reading());
        activity.observe(true, false, now + Duration::from_millis(100));
        assert!(activity.is_reading());
        activity.observe(true, true, now + Duration::from_millis(200));
        assert!(!activity.is_reading());
        activity.begin(SpeechKind::Announcement, now + Duration::from_millis(300));
        assert!(!activity.is_reading());
    }

    #[test]
    fn read_action_tracks_pending_speech_and_returns_to_read_during_the_tail() {
        let now = Instant::now();
        let mut activity = SpeechActivity::default();
        assert_eq!(activity.read_action(), ReadAction::Read);
        activity.begin(SpeechKind::Reading, now);
        assert_eq!(activity.read_action(), ReadAction::Stop);
        activity.observe(true, false, now + Duration::from_millis(100));
        assert_eq!(activity.read_action(), ReadAction::Stop);
        activity.observe(true, true, now + Duration::from_millis(200));
        assert_eq!(activity.read_action(), ReadAction::Read);
        activity.begin(SpeechKind::Announcement, now + Duration::from_millis(300));
        assert_eq!(activity.read_action(), ReadAction::Stop);
        activity.stop();
        assert_eq!(activity.read_action(), ReadAction::Read);
    }

    #[test]
    fn native_menu_updates_label_command_and_icon_together() {
        use super::{
            menu_icons, update_read_menu_item, wide_null, MenuGuard, IDM_EXIT, IDM_NEXT_SPEED,
            IDM_NEXT_VOICE,
        };
        use windows::core::{PCWSTR, PWSTR};
        use windows::Win32::UI::WindowsAndMessaging::*;

        unsafe {
            let menu = MenuGuard(CreatePopupMenu().unwrap());
            let ids = [
                ReadAction::Read.menu_item().0,
                IDM_NEXT_SPEED,
                IDM_NEXT_VOICE,
                IDM_EXIT,
            ];
            let label = wide_null("Test");
            for id in ids {
                AppendMenuW(menu.0, MF_STRING, id, PCWSTR(label.as_ptr())).unwrap();
            }
            let icons = menu_icons::MenuIcons::install(menu.0, ids).unwrap();
            let mut original = MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_BITMAP,
                ..Default::default()
            };
            GetMenuItemInfoW(menu.0, 0, true, &mut original).unwrap();
            assert_ne!(original.hbmpItem.0, 0);
            assert_ne!(icons.read_bitmap(true), original.hbmpItem);
            for action in [ReadAction::Stop, ReadAction::Read, ReadAction::Stop] {
                update_read_menu_item(menu.0, action, &icons).unwrap();
                let mut text = [0u16; 64];
                let mut item = MENUITEMINFOW {
                    cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_STRING | MIIM_ID | MIIM_BITMAP,
                    dwTypeData: PWSTR(text.as_mut_ptr()),
                    cch: text.len() as u32,
                    ..Default::default()
                };
                GetMenuItemInfoW(menu.0, 0, true, &mut item).unwrap();
                assert_eq!(item.wID as usize, action.menu_item().0);
                assert_eq!(
                    String::from_utf16_lossy(&text[..item.cch as usize]),
                    action.menu_item().1
                );
                assert_eq!(item.hbmpItem, icons.read_bitmap(action == ReadAction::Stop));
            }
        }
    }

    #[test]
    fn microsoft_voice_names_are_compact() {
        assert_eq!(
            friendly_voice_name("Microsoft Ava Online (Natural)"),
            "MS Ava"
        );
        assert_eq!(friendly_voice_name("Microsoft Eva Mobile"), "MS Eva");
        assert_eq!(
            friendly_voice_name("Microsoft Guy(Natural) - English (United States)"),
            "MS Guy"
        );
    }

    #[test]
    fn non_text_clipboard_content_is_a_silent_no_op() {
        assert!(
            readable_clipboard_text(Err(arboard::Error::ContentNotAvailable))
                .expect("non-text clipboard content should not be an error")
                .is_none()
        );
    }

    #[test]
    fn other_long_voice_names_are_truncated() {
        assert_eq!(
            friendly_voice_name("Acme Extremely Long Voice Name - English"),
            "Acme Extremely Long …"
        );
    }
}
