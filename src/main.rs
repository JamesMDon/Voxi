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
mod text;

const APP_NAME: PCWSTR = w!("Voxi");
const APP_CLASS: PCWSTR = w!("Voxi_Class");
const APP_MUTEX: PCWSTR = w!("Local\\Voxi.SingleInstance");
const APP_ICON_IDLE_ID: usize = 1;
const APP_ICON_ACTIVE_ID: usize = 2;

const HK_READ: i32 = 1;
const HK_SPEED: i32 = 2;
const HK_VOICE: i32 = 3;
const HK_EXIT: i32 = 4;
const HOTKEY_IDS: [i32; 4] = [HK_READ, HK_SPEED, HK_VOICE, HK_EXIT];

const VK_1: u32 = 0x31;
const VK_2: u32 = 0x32;
const VK_3: u32 = 0x33;
const VK_4: u32 = 0x34;

const SPEEDS: [i32; 3] = [0, 5, 10];
const DEFAULT_SPEED_IDX: usize = 2;

const WM_TRAY_ICON: u32 = WM_USER + 1;
const ID_TRAY_ICON: u32 = 1001;
const ID_TIMER_CHECK: usize = 1002;

const IDM_TOGGLE_READ: usize = 2000;
const IDM_NEXT_SPEED: usize = 2001;
const IDM_NEXT_VOICE: usize = 2002;
const IDM_EXIT: usize = 2003;

const SPRS_IS_SPEAKING: u32 = 2;
const SPF_ASYNC_PURGE: u32 = 3; // SPF_ASYNC | SPF_PURGE
const SPF_ASYNC_PURGE_XML: u32 = 11; // SPF_ASYNC | SPF_PURGE | SPF_IS_XML
const SPF_PURGE: u32 = 2;
const SPEECH_START_GRACE: Duration = Duration::from_millis(750);

#[derive(Default)]
struct SpeechActivity {
    requested_at: Option<Instant>,
    observed_running: bool,
}

impl SpeechActivity {
    fn begin(&mut self, now: Instant) {
        self.requested_at = Some(now);
        self.observed_running = false;
    }

    fn stop(&mut self) {
        self.requested_at = None;
        self.observed_running = false;
    }

    fn is_active(&self) -> bool {
        self.requested_at.is_some()
    }

    fn observe(&mut self, sapi_is_running: bool, now: Instant) -> bool {
        let Some(requested_at) = self.requested_at else {
            return false;
        };

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

        let (voices, natural_runtime) = load_voices(SPEEDS[DEFAULT_SPEED_IDX])?;

        STATE.with(|cell| {
            *cell.borrow_mut() = Some(AppState {
                voices,
                voice_idx: 0,
                speed_idx: DEFAULT_SPEED_IDX,
                speech: SpeechActivity::default(),
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
            name: "Microsoft Guy".to_owned(),
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
                HK_READ => toggle_read(hwnd),
                HK_SPEED => cycle_speed(hwnd),
                HK_VOICE => cycle_voice(hwnd),
                HK_EXIT => request_exit(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_TRAY_ICON => {
            if lparam.0 as u32 == WM_LBUTTONUP {
                toggle_read(hwnd);
            } else if lparam.0 as u32 == WM_RBUTTONUP {
                let result = show_context_menu(hwnd).map_err(|error| error.to_string());
                report_action_result(hwnd, "Could not open the Voxi menu", Some(result));
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 & 0xFFFF {
                IDM_TOGGLE_READ => toggle_read(hwnd),
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
        if !state.speech.is_active() {
            return;
        }

        let mut status = SPVOICESTATUS::default();
        if state.voices[state.voice_idx]
            .engine
            .GetStatus(&mut status, std::ptr::null_mut())
            .is_ok()
            && state
                .speech
                .observe(status.dwRunningState == SPRS_IS_SPEAKING, Instant::now())
        {
            let _ = update_tray(hwnd, state);
        }
    });
}

unsafe fn toggle_read(hwnd: HWND) {
    let result = with_state(|state| -> std::result::Result<(), String> {
        if state.speech.is_active() {
            state.voices[state.voice_idx]
                .engine
                .Speak(None, SPF_PURGE, None)
                .map_err(|error| error.to_string())?;
            state.speech.stop();
            update_tray(hwnd, state).map_err(|error| error.to_string())?;
            return Ok(());
        }

        let mut clipboard = Clipboard::new().map_err(|error| error.to_string())?;
        let Some(clipboard_text) = readable_clipboard_text(clipboard.get_text())? else {
            return Ok(());
        };
        if !clipboard_text.trim().is_empty() {
            speak_text_inner(hwnd, state, &clipboard_text).map_err(|error| error.to_string())?;
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

        let next_idx = (state.voice_idx + 1) % state.voices.len();
        state.voice_idx = next_idx;
        state.voices[next_idx]
            .engine
            .SetRate(SPEEDS[state.speed_idx])
            .map_err(|error| error.to_string())?;
        let name = state.voices[next_idx].name.clone();
        speak_text_inner(hwnd, state, &name).map_err(|error| error.to_string())
    });
    report_action_result(hwnd, "Could not change the voice", result);
}

unsafe fn cycle_speed(hwnd: HWND) {
    let result = with_state(|state| -> std::result::Result<(), String> {
        let next_idx = (state.speed_idx + 1) % SPEEDS.len();
        let new_rate = SPEEDS[next_idx];
        state.voices[state.voice_idx]
            .engine
            .SetRate(new_rate)
            .map_err(|error| error.to_string())?;
        state.speed_idx = next_idx;
        speak_text_inner(hwnd, state, &format!("Speed {new_rate}"))
            .map_err(|error| error.to_string())
    });
    report_action_result(hwnd, "Could not change the speech speed", result);
}

unsafe fn speak_text_inner(hwnd: HWND, state: &mut AppState, value: &str) -> Result<()> {
    let first_result = speak_with_voice(&state.voices[state.voice_idx], value);
    if let Err(error) = first_result {
        if !state.voices[state.voice_idx].natural {
            return Err(error);
        }

        let Some(fallback_idx) = state.voices.iter().position(|voice| !voice.natural) else {
            return Err(error);
        };
        state.voice_idx = fallback_idx;
        state.voices[fallback_idx]
            .engine
            .SetRate(SPEEDS[state.speed_idx])?;
        speak_with_voice(&state.voices[fallback_idx], value)?;
    }

    state.speech.begin(Instant::now());
    update_tray(hwnd, state)
}

unsafe fn speak_with_voice(choice: &VoiceChoice, value: &str) -> Result<()> {
    let (payload, flags) = if choice.natural {
        (text::to_plain_text(value), SPF_ASYNC_PURGE)
    } else {
        (text::to_sapi_xml(value, true), SPF_ASYNC_PURGE_XML)
    };
    let mut wide: Vec<u16> = payload.encode_utf16().collect();
    wide.push(0);

    choice.engine.Speak(PCWSTR(wide.as_ptr()), flags, None)?;
    Ok(())
}

unsafe fn update_tray(hwnd: HWND, state: &AppState) -> Result<()> {
    let mut nid = get_nid(hwnd);
    nid.uFlags = NIF_ICON | NIF_TIP;
    apply_tray_appearance(&mut nid, state);
    Shell_NotifyIconW(NIM_MODIFY, &nid).ok()
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
    let (voice_name, speed) = with_state(|state| {
        let voice_name = state
            .voices
            .get(state.voice_idx)
            .map(|voice| voice.name.clone())
            .unwrap_or_else(|| "Default".to_owned());
        (voice_name.replace('&', "&&"), SPEEDS[state.speed_idx])
    })
    .unwrap_or_else(|| ("Unavailable".to_owned(), SPEEDS[DEFAULT_SPEED_IDX]));
    let speak_wide = wide_null("Alt+1 | Read / Stop");
    let speed_wide = wide_null(&format!("Alt+2 | Speed {speed}"));
    let voice_wide = wide_null(&format!("Alt+3 | {voice_name}"));
    let exit_wide = wide_null("Alt+4 | Exit");

    AppendMenuW(
        menu.0,
        MF_STRING,
        IDM_TOGGLE_READ,
        PCWSTR(speak_wide.as_ptr()),
    )?;
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

    let _menu_icons = menu_icons::MenuIcons::install(
        menu.0,
        [IDM_TOGGLE_READ, IDM_NEXT_SPEED, IDM_NEXT_VOICE, IDM_EXIT],
    )?;

    let mut point = POINT::default();
    GetCursorPos(&mut point)?;
    let _ = SetForegroundWindow(hwnd);
    TrackPopupMenu(
        menu.0,
        TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        None,
    )
    .ok()?;
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
        return format!("Microsoft {short_name}");
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
    use super::{friendly_voice_name, readable_clipboard_text, SpeechActivity, SPEECH_START_GRACE};
    use std::time::{Duration, Instant};

    #[test]
    fn pending_speech_does_not_immediately_return_to_idle() {
        let started_at = Instant::now();
        let mut activity = SpeechActivity::default();
        activity.begin(started_at);

        assert!(!activity.observe(false, started_at + Duration::from_millis(100)));
        assert!(activity.is_active());
        assert!(activity.observe(false, started_at + SPEECH_START_GRACE));
        assert!(!activity.is_active());
    }

    #[test]
    fn observed_speech_returns_to_idle_when_sapi_finishes() {
        let started_at = Instant::now();
        let mut activity = SpeechActivity::default();
        activity.begin(started_at);

        assert!(!activity.observe(true, started_at + Duration::from_millis(100)));
        assert!(activity.is_active());
        assert!(activity.observe(false, started_at + Duration::from_millis(200)));
        assert!(!activity.is_active());
    }

    #[test]
    fn microsoft_voice_names_are_compact() {
        assert_eq!(
            friendly_voice_name("Microsoft Ava Online (Natural)"),
            "Microsoft Ava"
        );
        assert_eq!(friendly_voice_name("Microsoft Eva Mobile"), "Microsoft Eva");
        assert_eq!(
            friendly_voice_name("Microsoft Guy(Natural) - English (United States)"),
            "Microsoft Guy"
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
