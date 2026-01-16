// Voxi - High Frequency SAPI Engine
// v1.10.0 | 2026

#![windows_subsystem = "windows"]

use arboard::Clipboard;
use regex::Regex;
use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::LazyLock;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::Speech::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const APP_NAME: PCWSTR = w!("Voxi");

// --- CONFIG ---
const DLL_ICONS: PCWSTR = w!("shell32.dll");
const ICON_IDLE: u32 = 260;
const ICON_ACTIVE: u32 = 131;

// Hotkeys
const HK_READ: i32 = 1;
const HK_SPEED: i32 = 2;
const HK_VOICE: i32 = 3;
const HK_EXIT: i32 = 4;

// Virtual key codes
const VK_1: u32 = 0x31;
const VK_2: u32 = 0x32;
const VK_3: u32 = 0x33;
const VK_4: u32 = 0x34;

const SPEEDS: [i32; 3] = [0, 5, 10];
const DEFAULT_SPEED_IDX: usize = 2;

// System
const WM_TRAY_ICON: u32 = WM_USER + 1;
const ID_TRAY_ICON: u32 = 1001;
const ID_TIMER_CHECK: usize = 1002;

// Menu
const IDM_TOGGLE_READ: usize = 2000;
const IDM_NEXT_SPEED: usize = 2001;
const IDM_NEXT_VOICE: usize = 2002;
const IDM_EXIT: usize = 2003;

// SAPI constants
const SPRS_IS_SPEAKING: u32 = 2;
const SPF_ASYNC_PURGE_XML: u32 = 11; // SPF_ASYNC | SPF_PURGE | SPF_IS_XML
const SPF_PURGE: u32 = 2;

// --- DICTIONARY ---
struct Rule {
    re: Regex,
    rep: &'static str,
}

static DICTIONARY: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let mut r = Vec::new();

    // 1. COMPLEX REGEX RULES
    if let Ok(re) = Regex::new(r"((http|ftp|https)://)?(www\.)?([-a-zA-Z0-9@:%._\+~#\=]{2,256}\.[a-z]{2,6}\b)([-a-zA-Z0-9@:%_\+.~#?&//\=]*)") {
        r.push(Rule { re, rep: "$4" });
    }
    if let Ok(re) = Regex::new(r"file:\/\/\/[a-zA-Z]:\/.*") {
        r.push(Rule { re, rep: "file" });
    }

    // 2. HELPER
    let mut add = |p: &str, rep: &'static str, use_bound: bool| {
        let pat = if use_bound {
            format!(r"(?i)\b{}\b", regex::escape(p))
        } else {
            format!(r"(?i){}", regex::escape(p))
        };
        if let Ok(re) = Regex::new(&pat) {
            r.push(Rule { re, rep });
        }
    };

    // 3. STANDARD RULES
    add("View keyboard shortcuts", "", false);
    add("To view keyboard shortcuts, press question mark", "", false);
    add("Next Reply", "", false);

    // Dividers
    add("___", ".", false);
    add("###", ".", false);
    add("__", ".", false);
    add("##", ".", false);

    // Emojis (fixed UTF-8)
    add("😭", " Sob ", false);
    add("😂", " Joy ", false);
    add("🔥", " Fire ", false);
    add("❤️", " Heart ", false);
    add("👍", " Thumbs up ", false);
    add("🎉", " Party ", false);

    // Words
    add("Ableton", "Abelten", true);
    add("AOC", "A.O.C.", true);
    add("Aesop", "Ace-op", true);
    add("Aes", "Ace", true);
    add("Bastiat", "Bah-stee-aught", true);
    add("Calendly", "Cal-endly", true);
    add("Camus", "Camu", true);
    add("Carrd", "Card", true);
    add("Cerave", "CeraVee", true);
    add("Conversion", "Convursion", true);
    add("CopyQ", "CopyCue", true);
    add("Cuck", "Cuhck", true);
    add("Culinary", "Cullinary", true);
    add("Chapo", "Chap-o", true);
    add("Chatgpt", "ChatGPT", true);
    add("DeSantis", "De-Santis", true);
    add("DMing", "D-M-ing", true);
    add("Doja", "Doeja", true);
    add("Elgato", "El-got-o", true);
    add("Fage", "Fa-yay", true);
    add("Ghibli", "Jiblee", true);
    add("Giga", "Gigga", true);
    add("Github", "GitHub", true);
    add("Glutes", "Glootes", true);
    add("Goku", "Go-ku", true);
    add("Hormozi", "Hormoezee", true);
    add("Huberman", "Hewberman", true);
    add("JavaScript", "Java-Script", true);
    add("Joji", "Joegee", true);
    add("Kasa", "Casa", true);
    add("Kayfabe", "Kay-fabe", true);
    add("Kimya", "Kim-ya", true);
    add("Kobe", "Co-be", true);
    add("LeadSynth.com", "LeadSynth dot com", false);
    add("Leevi", "Levy", true);
    add("Leila", "Layla", true);
    add("Livestream", "Lyevstream", true);
    add("Monetiz", "Mahnetiz", false);
    add("Mozi", "Moezee", true);
    add("Munger", "Mun-gir", true);
    add("Pantone", "Pan-tone", true);
    add("Paracord", "Parahcord", true);
    add("PreCheck", "Pre-Check", true);
    add("Rapport", "Rapore", true);
    add("Rangeman", "Range-Man", true);
    add("RevShare", "Rev-Share", true);
    add("Schopenhauer", "Showpenhower", true);
    add("Sneako", "Sneak-o", true);
    add("Tiktok", "TikTok", true);
    add("ToDos", "To Dos", true);
    add("ToDo", "To Do", true);
    add("Toup", "Tooop", true);
    add("Upsell", "Up-sell", true);
    add("Vegeta", "Veg-eatuh", true);
    add("Webhook", "Web-hook", true);
    add("Whitespace", "White-space", true);
    add("Wordcel", "Wordcell", true);
    add("Xmas", "Christmas", true);
    add("Zherka", "Zerka", true);

    // Acronyms
    add("AFAICT", "As far as I can tell", true);
    add("AFAIK", "As far as I know", true);
    add("FR", "For Real", true);
    add("IIRC", "If I recall correctly", true);
    add("IMO", "In my opinion", true);
    add("SEO", "S-E-O", true);
    add("TBQH", "To be quite honest", true);
    add("TBH", "To be honest", true);
    add("YC", "Y-C", true);

    r
});

struct AppState {
    voice: ISpVoice,
    tokens: Vec<ISpObjectToken>,
    voice_idx: usize,
    speed_idx: usize,
    is_speaking: bool,
}

thread_local! {
    static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

fn with_state<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut AppState) -> R,
{
    STATE.with(|cell| cell.borrow_mut().as_mut().map(f))
}

fn main() -> Result<()> {
    unsafe {
        CoInitialize(None).ok();

        let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL)?;
        let audio: ISpAudio = CoCreateInstance(&SpMMAudioOut, None, CLSCTX_ALL)?;
        voice.SetOutput(&audio, true)?;

        let mut tokens = Vec::new();
        let cat: ISpObjectTokenCategory =
            CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL)?;
        cat.SetId(
            w!("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Speech\\Voices"),
            false,
        )?;
        let token_enum = cat.EnumTokens(None, None)?;
        let mut count = 0;
        token_enum.GetCount(&mut count)?;

        let mut eva_index = 0;
        for i in 0..count {
            let t = token_enum.Item(i)?;
            let name = t.GetStringValue(None)?.to_string().unwrap_or_default();
            if name.to_lowercase().contains("eva") {
                eva_index = i;
            }
            tokens.push(t);
        }

        if !tokens.is_empty() {
            voice.SetVoice(&tokens[eva_index as usize])?;
            voice.SetRate(SPEEDS[DEFAULT_SPEED_IDX])?;
        }

        STATE.with(|cell| {
            *cell.borrow_mut() = Some(AppState {
                voice,
                tokens,
                voice_idx: eva_index as usize,
                speed_idx: DEFAULT_SPEED_IDX,
                is_speaking: false,
            });
        });

        let instance = GetModuleHandleW(None)?;
        let wnd_class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: w!("Voxi_Class"),
            ..Default::default()
        };
        RegisterClassW(&wnd_class);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("Voxi_Class"),
            APP_NAME,
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        );

        init_tray(hwnd)?;
        SetTimer(hwnd, ID_TIMER_CHECK, 100, None);

        let _ = RegisterHotKey(hwnd, HK_READ, MOD_ALT, VK_1);
        let _ = RegisterHotKey(hwnd, HK_SPEED, MOD_ALT, VK_2);
        let _ = RegisterHotKey(hwnd, HK_VOICE, MOD_ALT, VK_3);
        let _ = RegisterHotKey(hwnd, HK_EXIT, MOD_ALT, VK_4);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = Shell_NotifyIconW(NIM_DELETE, &mut get_nid(hwnd));
    }
    Ok(())
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
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
                HK_EXIT => PostQuitMessage(0),
                _ => {}
            }
            LRESULT(0)
        }
        WM_TRAY_ICON => {
            if lparam.0 as u32 == WM_LBUTTONUP {
                toggle_read(hwnd);
            } else if lparam.0 as u32 == WM_RBUTTONUP {
                show_context_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            match id {
                IDM_TOGGLE_READ => toggle_read(hwnd),
                IDM_NEXT_SPEED => cycle_speed(hwnd),
                IDM_NEXT_VOICE => cycle_voice(hwnd),
                IDM_EXIT => PostQuitMessage(0),
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

unsafe fn check_icon_state(hwnd: HWND) {
    with_state(|state| {
        let mut status = SPVOICESTATUS::default();
        let _ = state.voice.GetStatus(&mut status, std::ptr::null_mut());

        if state.is_speaking && status.dwRunningState != SPRS_IS_SPEAKING {
            state.is_speaking = false;
            set_tray_icon(hwnd, ICON_IDLE);
        }
    });
}

unsafe fn toggle_read(hwnd: HWND) {
    with_state(|state| {
        let mut status = SPVOICESTATUS::default();
        let _ = state.voice.GetStatus(&mut status, std::ptr::null_mut());

        if status.dwRunningState == SPRS_IS_SPEAKING {
            let _ = state.voice.Speak(None, SPF_PURGE, None);
            state.is_speaking = false;
            set_tray_icon(hwnd, ICON_IDLE);
        } else if let Ok(mut cb) = Clipboard::new() {
            if let Ok(text) = cb.get_text() {
                if !text.trim().is_empty() {
                    speak_text_inner(hwnd, state, &text);
                }
            }
        }
    });
}

unsafe fn cycle_voice(hwnd: HWND) {
    with_state(|state| {
        if state.tokens.is_empty() {
            return;
        }
        state.voice_idx = (state.voice_idx + 1) % state.tokens.len();
        let token = &state.tokens[state.voice_idx];
        let _ = state.voice.SetVoice(token);
        let name = token
            .GetStringValue(None)
            .map(|s| s.to_string().unwrap_or_default())
            .unwrap_or_default();
        speak_text_inner(hwnd, state, &name);
    });
}

unsafe fn cycle_speed(hwnd: HWND) {
    with_state(|state| {
        state.speed_idx = (state.speed_idx + 1) % SPEEDS.len();
        let new_rate = SPEEDS[state.speed_idx];
        let _ = state.voice.SetRate(new_rate);
        speak_text_inner(hwnd, state, &format!("Speed {}", new_rate));
    });
}

unsafe fn speak_text_inner(hwnd: HWND, state: &mut AppState, text: &str) {
    // 1. Dictionary
    let mut processed = Cow::Borrowed(text);
    for rule in DICTIONARY.iter() {
        if rule.re.is_match(&processed) {
            let replaced = rule.re.replace_all(&processed, rule.rep);
            processed = Cow::Owned(replaced.into_owned());
        }
    }

    // 2. XML Escape
    let escaped = processed
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");

    // 3. XML Silence (Bluetooth Fix)
    let xml = format!(
        "<speak version='1.0'>{}<silence msec='2000'/></speak>",
        escaped
    );

    let mut wide: Vec<u16> = xml.encode_utf16().collect();
    wide.push(0);

    let _ = state.voice.Speak(PCWSTR(wide.as_ptr()), SPF_ASYNC_PURGE_XML, None);

    state.is_speaking = true;
    set_tray_icon(hwnd, ICON_ACTIVE);
}

unsafe fn set_tray_icon(hwnd: HWND, icon_id: u32) {
    let mut nid = get_nid(hwnd);
    nid.uFlags = NIF_ICON;
    if let Ok(h_inst) = GetModuleHandleW(None) {
        nid.hIcon = ExtractIconW(h_inst, DLL_ICONS, icon_id);
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

unsafe fn init_tray(hwnd: HWND) -> Result<()> {
    let mut nid = get_nid(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY_ICON;

    let h_inst = GetModuleHandleW(None)?;
    nid.hIcon = ExtractIconW(h_inst, DLL_ICONS, ICON_IDLE);

    let tip = "Voxi\0".encode_utf16().collect::<Vec<u16>>();
    nid.szTip[..tip.len()].copy_from_slice(&tip);

    Shell_NotifyIconW(NIM_ADD, &nid).ok()?;
    Ok(())
}

fn get_nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ID_TRAY_ICON;
    nid
}

unsafe fn show_context_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else { return };
    let _ = AppendMenuW(menu, MF_STRING, IDM_TOGGLE_READ, w!("Toggle Read (Alt+1)"));
    let _ = AppendMenuW(menu, MF_STRING, IDM_NEXT_SPEED, w!("Next Speed (Alt+2)"));
    let _ = AppendMenuW(menu, MF_STRING, IDM_NEXT_VOICE, w!("Next Voice (Alt+3)"));
    let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT, w!("Exit (Alt+4)"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(menu, TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, 0, hwnd, None);
    let _ = DestroyMenu(menu);
}