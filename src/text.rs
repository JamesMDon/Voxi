use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

struct Rule {
    re: Regex,
    replacement: &'static str,
}

static DICTIONARY: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let mut rules = Vec::new();

    let mut add_regex = |pattern: &str, replacement: &'static str| {
        rules.push(Rule {
            re: Regex::new(pattern).expect("Voxi dictionary patterns must be valid"),
            replacement,
        });
    };

    add_regex(
        r"(?i)(?:(?:https?|ftp)://)?(?:www\.)?([-a-z0-9@:%._+~#=]{2,256}\.[a-z]{2,63}\b)(?:[-a-z0-9@:%_+.~#?&/=]*)",
        "$1",
    );
    add_regex(r"(?i)file:///[a-z]:/[^\r\n]*", "file");

    // Expand multi-character operators before the speech engine interprets
    // their punctuation one character at a time. Handle the longer JavaScript
    // form first so the != rule cannot split it.
    add_regex(r"!==", " not strictly equal to ");
    add_regex(r"[\t ]*!=[\t ]*", " not equal to ");

    let mut add = |phrase: &str, replacement: &'static str, word_boundaries: bool| {
        let pattern = if word_boundaries {
            format!(r"(?i)\b{}\b", regex::escape(phrase))
        } else {
            format!(r"(?i){}", regex::escape(phrase))
        };
        rules.push(Rule {
            re: Regex::new(&pattern).expect("escaped Voxi dictionary patterns must be valid"),
            replacement,
        });
    };

    // Longer phrases must precede their substrings.
    add("To view keyboard shortcuts, press question mark", "", false);
    add("View keyboard shortcuts", "", false);
    add("Next Reply", "", false);

    add("___", ".", false);
    add("###", ".", false);
    add("__", ".", false);
    add("##", ".", false);

    add("😭", " Sob ", false);
    add("😂", " Joy ", false);
    add("🔥", " Fire ", false);
    add("❤️", " Heart ", false);
    add("👍", " Thumbs up ", false);
    add("🎉", " Party ", false);

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

    add("AFAICT", "As far as I can tell", true);
    add("AFAIK", "As far as I know", true);
    add("FR", "For Real", true);
    add("IIRC", "If I recall correctly", true);
    add("IMO", "In my opinion", true);
    add("SEO", "S-E-O", true);
    add("TBQH", "To be quite honest", true);
    add("TBH", "To be honest", true);
    add("YC", "Y-C", true);

    rules
});

pub(crate) fn initialize() {
    LazyLock::force(&DICTIONARY);
}

fn preprocess_text(text: &str) -> Cow<'_, str> {
    let mut processed = Cow::Borrowed(text);
    for rule in DICTIONARY.iter() {
        if rule.re.is_match(&processed) {
            processed = Cow::Owned(
                rule.re
                    .replace_all(&processed, rule.replacement)
                    .into_owned(),
            );
        }
    }
    processed
}

pub(crate) fn to_sapi_xml(text: &str, trailing_silence: bool) -> String {
    let processed = preprocess_text(text);
    let mut escaped = String::with_capacity(processed.len());
    for character in processed.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }

    if trailing_silence {
        format!("<speak version='1.0'>{escaped}<silence msec='2000'/></speak>")
    } else {
        format!("<speak version='1.0'>{escaped}</speak>")
    }
}

pub(crate) fn to_plain_text(text: &str) -> String {
    preprocess_text(text).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{preprocess_text, to_plain_text, to_sapi_xml};

    #[test]
    fn removes_long_keyboard_shortcut_prompt_before_shorter_rule() {
        assert_eq!(
            preprocess_text("To view keyboard shortcuts, press question mark"),
            ""
        );
    }

    #[test]
    fn simplifies_modern_and_uppercase_urls() {
        assert_eq!(
            preprocess_text("See HTTPS://WWW.EXAMPLE.TECHNOLOGY/a?b=1 now"),
            "See EXAMPLE.TECHNOLOGY now"
        );
    }

    #[test]
    fn simplifies_windows_file_urls() {
        assert_eq!(
            preprocess_text("Open file:///C:/Users/James/note.txt"),
            "Open file"
        );
    }

    #[test]
    fn applies_dictionary_at_word_boundaries() {
        assert_eq!(
            preprocess_text("SEO and AFAIK"),
            "S-E-O and As far as I know"
        );
        assert_eq!(preprocess_text("freshness"), "freshness");
    }

    #[test]
    fn reads_not_equal_operator_as_a_complete_operator() {
        assert_eq!(
            preprocess_text("status != ready"),
            "status not equal to ready"
        );
        assert_eq!(to_plain_text("a!=b"), "a not equal to b");
        assert_eq!(
            to_sapi_xml("a != b", false),
            "<speak version='1.0'>a not equal to b</speak>"
        );
        assert_eq!(to_plain_text("a!==b"), "a not strictly equal to b");
    }

    #[test]
    fn escapes_xml_before_wrapping_for_sapi() {
        assert_eq!(
            to_sapi_xml("A < B & C's \"quote\"", true),
            "<speak version='1.0'>A &lt; B &amp; C&apos;s &quot;quote&quot;<silence msec='2000'/></speak>"
        );
    }

    #[test]
    fn can_omit_trailing_silence_for_embedded_voices() {
        assert_eq!(
            to_sapi_xml("Ready", false),
            "<speak version='1.0'>Ready</speak>"
        );
    }

    #[test]
    fn natural_voice_plain_text_keeps_dictionary_without_xml() {
        assert_eq!(to_plain_text("AFAIK <ready>"), "As far as I know <ready>");
    }
}
