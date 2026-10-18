use crate::{settings, text};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

const WORDS_FILE: &str = "words.txt";
const TEMPLATE: &str = "\
# Voxi pronunciations, used when the Pronunciation filter is on.
# One per line: written = spoken. Whole words only, ignoring case.
# Saved changes apply to the next reading. Lines starting with # are ignored.
#
# Ghibli = Jiblee
# SQL = sequel
";

/// Tracks the words file so edits apply on the next reading without a watcher.
#[derive(Default)]
pub(crate) struct Words {
    loaded: Option<Option<SystemTime>>,
}

impl Words {
    /// A metadata check per reading; the file is parsed again only after it changes.
    pub(crate) fn refresh(&mut self) {
        let path = settings::data_path(WORDS_FILE);
        let modified = path
            .as_ref()
            .and_then(|path| fs::metadata(path).ok())
            .and_then(|metadata| metadata.modified().ok());
        if self.loaded == Some(modified) {
            return;
        }
        self.loaded = Some(modified);
        let contents = path
            .and_then(|path| fs::read_to_string(path).ok())
            .unwrap_or_default();
        text::set_custom_words(&parse(&contents));
    }
}

/// Returns the words file, creating it with usage notes on first use.
pub(crate) fn ensure_file() -> io::Result<PathBuf> {
    let path = settings::data_path(WORDS_FILE).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Windows did not provide an AppData directory",
        )
    })?;
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, TEMPLATE)?;
    }
    Ok(path)
}

fn parse(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(written, spoken)| (written.trim().to_owned(), spoken.trim().to_owned()))
        .filter(|(written, _)| !written.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse, TEMPLATE};

    #[test]
    fn parses_pairs_and_skips_comments_blanks_and_malformed_lines() {
        let words = parse(
            "\u{feff}Ghibli = Jiblee\n# SQL = sequel\n\nno equals\n = empty\nC++=C plus plus\n",
        );
        assert_eq!(
            words,
            [
                ("Ghibli".to_owned(), "Jiblee".to_owned()),
                ("C++".to_owned(), "C plus plus".to_owned()),
            ]
        );
    }

    #[test]
    fn template_examples_are_inactive() {
        assert!(parse(TEMPLATE).is_empty());
    }
}
