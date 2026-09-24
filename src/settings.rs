use crate::text::FilterOptions;
use std::fs;
use std::io;
use std::path::PathBuf;

const SETTINGS_DIRECTORY: &str = "Voxi";
const SETTINGS_FILE: &str = "settings.txt";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppSettings {
    pub(crate) voice_name: Option<String>,
    pub(crate) speed: i32,
    pub(crate) filters: FilterOptions,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            voice_name: None,
            speed: 10,
            filters: FilterOptions::STANDARD,
        }
    }
}

pub(crate) fn load() -> AppSettings {
    let Some(path) = settings_path() else {
        return AppSettings::default();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return AppSettings::default();
    };
    parse(&contents)
}

pub(crate) fn save(settings: &AppSettings) -> io::Result<()> {
    let path = settings_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Windows did not provide an AppData directory",
        )
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serialize(settings))
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|path| path.join(SETTINGS_DIRECTORY).join(SETTINGS_FILE))
}

fn parse(contents: &str) -> AppSettings {
    let mut settings = AppSettings::default();

    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "voice" if !value.trim().is_empty() => {
                settings.voice_name = Some(value.trim().to_owned());
            }
            "speed" => {
                if let Ok(speed) = value.trim().parse() {
                    settings.speed = speed;
                }
            }
            "filters" => {
                if let Ok(mask) = value.trim().parse() {
                    settings.filters = FilterOptions::from_mask(mask);
                }
            }
            _ => {}
        }
    }

    settings
}

fn serialize(settings: &AppSettings) -> String {
    let voice_name = settings
        .voice_name
        .as_deref()
        .unwrap_or_default()
        .replace(['\r', '\n'], " ");
    format!(
        "voice={voice_name}\nspeed={}\nfilters={}\n",
        settings.speed,
        settings.filters.mask()
    )
}

#[cfg(test)]
mod tests {
    use super::{parse, serialize, AppSettings};
    use crate::text::{FilterCategory, FilterOptions};

    #[test]
    fn missing_and_unknown_values_keep_safe_defaults() {
        let settings = parse("speed=not-a-number\nfuture=value\n");
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn settings_round_trip_voice_speed_and_custom_filters() {
        let settings = AppSettings {
            voice_name: Some("Microsoft Eva".to_owned()),
            speed: 5,
            filters: FilterOptions::STANDARD.toggled(FilterCategory::Emoji),
        };
        assert_eq!(parse(&serialize(&settings)), settings);
    }

    #[test]
    fn filter_masks_ignore_unknown_future_bits() {
        let settings = parse("filters=255\n");
        assert_eq!(settings.filters, FilterOptions::STANDARD);
    }
}
