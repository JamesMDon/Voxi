use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub(crate) const HK_READ: i32 = 1;
pub(crate) const HK_SPEED: i32 = 2;
pub(crate) const HK_VOICE: i32 = 3;
pub(crate) const HK_EXIT: i32 = 4;
const HOTKEYS: [(i32, u32); 4] = [
    (HK_READ, 0x31),
    (HK_SPEED, 0x32),
    (HK_VOICE, 0x33),
    (HK_EXIT, 0x34),
];

// Unassigned virtual key, the same menu mask key AutoHotkey uses by default.
const VK_MENU_MASK: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

/// Modifier presets for the fixed 1-4 hotkeys. Shift+Alt is left out because it is
/// the default Windows shortcut for switching keyboard layouts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Modifier {
    #[default]
    Alt,
    CtrlAlt,
    CtrlShift,
}

impl Modifier {
    pub(crate) const ALL: [Self; 3] = [Self::Alt, Self::CtrlAlt, Self::CtrlShift];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Alt => "Alt",
            Self::CtrlAlt => "Ctrl+Alt",
            Self::CtrlShift => "Ctrl+Shift",
        }
    }

    pub(crate) fn from_label(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|modifier| modifier.label().eq_ignore_ascii_case(value.trim()))
    }

    fn flags(self) -> HOT_KEY_MODIFIERS {
        let keys = match self {
            Self::Alt => MOD_ALT,
            Self::CtrlAlt => MOD_CONTROL | MOD_ALT,
            Self::CtrlShift => MOD_CONTROL | MOD_SHIFT,
        };
        keys | MOD_NOREPEAT
    }
}

/// Registers all four hotkeys or none, so a clash never leaves a partial set.
pub(crate) unsafe fn register(hwnd: HWND, modifier: Modifier) -> Result<(), String> {
    for (index, (id, key)) in HOTKEYS.into_iter().enumerate() {
        if RegisterHotKey(hwnd, id, modifier.flags(), key).is_err() {
            for (registered, _) in &HOTKEYS[..index] {
                let _ = UnregisterHotKey(hwnd, *registered);
            }
            return Err(format!(
                "{}+{} is already used by another app.",
                modifier.label(),
                index + 1
            ));
        }
    }
    Ok(())
}

pub(crate) unsafe fn unregister(hwnd: HWND) {
    for (id, _) in HOTKEYS {
        let _ = UnregisterHotKey(hwnd, id);
    }
}

// RegisterHotKey swallows the 1-4 key, so the foreground app sees a bare Alt tap and
// activates its menu bar when Alt is released. Tapping an unassigned key while Alt is
// still held turns that into an Alt chord, which leaves the menu alone.
pub(crate) fn mask_alt_menu() {
    unsafe {
        if GetAsyncKeyState(VK_MENU.0 as i32) >= 0 {
            return;
        }
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU_MASK,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::Modifier;

    #[test]
    fn modifier_labels_round_trip_ignoring_case() {
        for modifier in Modifier::ALL {
            assert_eq!(Modifier::from_label(modifier.label()), Some(modifier));
        }
        assert_eq!(Modifier::from_label(" ctrl+alt "), Some(Modifier::CtrlAlt));
        assert_eq!(Modifier::from_label("Shift+Alt"), None);
    }
}
