use std::mem::size_of;
use std::path::{Path, PathBuf};
use windows::core::{Error, Result, GUID, PCWSTR};
use windows::Win32::Foundation::{E_FAIL, HANDLE};
use windows::Win32::Media::Speech::{
    IEnumSpObjectTokens, ISpAudio, ISpObjectToken, ISpVoice, SpMMAudioOut, SpVoice, SPF_DEFAULT,
};
use windows::Win32::System::ApplicationInstallationAndServicing::{
    ActivateActCtx, CreateActCtxW, DeactivateActCtx, ReleaseActCtx, ACTCTXW,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_DWORD, REG_OPTION_VOLATILE,
};

const ENUMERATOR: GUID = GUID::from_u128(0xb8b9e38f_e5a2_4661_9fde_4ac7377aa6f6);
const CONFIG_PATH: PCWSTR = windows::core::w!("Software\\NaturalVoiceSAPIAdapter\\Enumerator");
const CONFIG_ROOT: PCWSTR = windows::core::w!("Software\\NaturalVoiceSAPIAdapter");

pub(crate) struct NaturalVoice {
    pub(crate) engine: ISpVoice,
    pub(crate) token: ISpObjectToken,
    pub(crate) runtime: NaturalRuntime,
}

pub(crate) struct NaturalRuntime {
    activation: HANDLE,
    cookie: usize,
}

impl Drop for NaturalRuntime {
    fn drop(&mut self) {
        unsafe {
            let _ = DeactivateActCtx(0, self.cookie);
            ReleaseActCtx(self.activation);
        }
    }
}

struct AdapterConfigGuard;

impl AdapterConfigGuard {
    unsafe fn create() -> Result<Self> {
        let mut existing = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, CONFIG_ROOT, 0, KEY_READ, &mut existing).is_ok() {
            let _ = RegCloseKey(existing);
            return Err(Error::new(
                E_FAIL,
                "A NaturalVoiceSAPIAdapter user configuration already exists, so Voxi left it untouched."
                    .into(),
            ));
        }

        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            CONFIG_PATH,
            0,
            PCWSTR::null(),
            REG_OPTION_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )?;

        let enabled = 1u32.to_ne_bytes();
        let result = RegSetValueExW(
            key,
            windows::core::w!("NoEdgeVoices"),
            0,
            REG_DWORD,
            Some(&enabled),
        )
        .and_then(|_| {
            RegSetValueExW(
                key,
                windows::core::w!("NoAzureVoices"),
                0,
                REG_DWORD,
                Some(&enabled),
            )
        });
        let _ = RegCloseKey(key);
        if let Err(error) = result {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, CONFIG_ROOT);
            return Err(error);
        }

        Ok(Self)
    }
}

impl Drop for AdapterConfigGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, CONFIG_ROOT);
        }
    }
}

pub(crate) unsafe fn load_guy(default_rate: i32) -> Result<Option<NaturalVoice>> {
    let Some(runtime_dir) = runtime_directory() else {
        return Ok(None);
    };
    let manifest = runtime_dir.join("adapter.manifest");
    if !manifest.is_file() || !runtime_dir.join("NaturalVoiceSAPIAdapter.dll").is_file() {
        return Ok(None);
    }

    let _config = AdapterConfigGuard::create()?;
    let runtime = activate(&manifest)?;
    let enumerator: IEnumSpObjectTokens = CoCreateInstance(&ENUMERATOR, None, CLSCTX_ALL)?;
    let mut count = 0;
    enumerator.GetCount(&mut count)?;

    let mut guy = None;
    for index in 0..count {
        let token = enumerator.Item(index)?;
        let name = token_name(&token)?;
        if name.to_lowercase().contains("guy") {
            guy = Some((name, token));
            break;
        }
    }

    let Some((_, token)) = guy else {
        return Err(Error::new(
            E_FAIL,
            "The bundled Microsoft Guy voice was not found.".into(),
        ));
    };

    let engine: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL)?;
    let audio: ISpAudio = CoCreateInstance(&SpMMAudioOut, None, CLSCTX_ALL)?;
    engine.SetOutput(&audio, true)?;
    engine.SetVoice(&token)?;
    engine.SetRate(default_rate)?;

    engine.SetVolume(0)?;
    let warmup = engine.Speak(windows::core::w!("Voxi ready"), SPF_DEFAULT.0 as u32, None);
    let _ = engine.SetVolume(100);
    warmup?;

    Ok(Some(NaturalVoice {
        engine,
        token,
        runtime,
    }))
}

unsafe fn token_name(token: &ISpObjectToken) -> Result<String> {
    let value = token.GetStringValue(None)?;
    let name = value.to_string();
    CoTaskMemFree(Some(value.as_ptr().cast()));
    Ok(name?)
}

unsafe fn activate(manifest: &Path) -> Result<NaturalRuntime> {
    let source = wide(&manifest.to_string_lossy());
    let activation = CreateActCtxW(&ACTCTXW {
        cbSize: size_of::<ACTCTXW>() as u32,
        lpSource: PCWSTR(source.as_ptr()),
        ..Default::default()
    })?;
    let mut cookie = 0;
    if let Err(error) = ActivateActCtx(activation, &mut cookie) {
        ReleaseActCtx(activation);
        return Err(error);
    }
    Ok(NaturalRuntime { activation, cookie })
}

unsafe fn runtime_directory() -> Option<PathBuf> {
    let mut buffer = [0u16; 32_768];
    let length = GetModuleFileNameW(None, &mut buffer) as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    let executable = PathBuf::from(String::from_utf16_lossy(&buffer[..length]));
    executable
        .parent()
        .map(|path| path.join("runtime").join("natural"))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
