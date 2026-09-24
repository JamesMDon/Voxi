use pulldown_cmark::{Event, Parser, TagEnd};
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

struct Rule {
    category: FilterCategory,
    re: Regex,
    replacement: &'static str,
}

/// Optional text filters. Raw disables all of them; whitespace normalization and
/// XML escaping for SAPI always remain, since they keep speech input valid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FilterCategory {
    Cleanup,
    Pronunciation,
    Abbreviations,
    Emoji,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FilterOptions {
    cleanup: bool,
    pronunciation: bool,
    abbreviations: bool,
    emoji: bool,
}

impl FilterOptions {
    const CLEANUP_MASK: u8 = 1 << 0;
    const PRONUNCIATION_MASK: u8 = 1 << 1;
    const ABBREVIATIONS_MASK: u8 = 1 << 2;
    const EMOJI_MASK: u8 = 1 << 3;

    pub(crate) const STANDARD: Self = Self {
        cleanup: true,
        pronunciation: true,
        abbreviations: true,
        emoji: true,
    };

    pub(crate) const RAW: Self = Self {
        cleanup: false,
        pronunciation: false,
        abbreviations: false,
        emoji: false,
    };

    /// Unknown future bits are ignored, so older builds read newer settings safely.
    pub(crate) fn from_mask(mask: u8) -> Self {
        Self {
            cleanup: mask & Self::CLEANUP_MASK != 0,
            pronunciation: mask & Self::PRONUNCIATION_MASK != 0,
            abbreviations: mask & Self::ABBREVIATIONS_MASK != 0,
            emoji: mask & Self::EMOJI_MASK != 0,
        }
    }

    pub(crate) fn mask(self) -> u8 {
        [
            (self.cleanup, Self::CLEANUP_MASK),
            (self.pronunciation, Self::PRONUNCIATION_MASK),
            (self.abbreviations, Self::ABBREVIATIONS_MASK),
            (self.emoji, Self::EMOJI_MASK),
        ]
        .into_iter()
        .filter(|(enabled, _)| *enabled)
        .fold(0, |mask, (_, bit)| mask | bit)
    }

    pub(crate) fn label(self) -> &'static str {
        if self == Self::STANDARD {
            "Standard"
        } else if self == Self::RAW {
            "Raw"
        } else {
            "Custom"
        }
    }

    pub(crate) fn is_enabled(self, category: FilterCategory) -> bool {
        match category {
            FilterCategory::Cleanup => self.cleanup,
            FilterCategory::Pronunciation => self.pronunciation,
            FilterCategory::Abbreviations => self.abbreviations,
            FilterCategory::Emoji => self.emoji,
        }
    }

    pub(crate) fn toggled(self, category: FilterCategory) -> Self {
        let mut next = self;
        let flag = match category {
            FilterCategory::Cleanup => &mut next.cleanup,
            FilterCategory::Pronunciation => &mut next.pronunciation,
            FilterCategory::Abbreviations => &mut next.abbreviations,
            FilterCategory::Emoji => &mut next.emoji,
        };
        *flag = !*flag;
        next
    }
}

static MULTIPLICATION: LazyLock<Regex> = LazyLock::new(|| {
    // Match the whole chain so shared operands in 2*3*4 are handled together.
    // Horizontal whitespace keeps separate lines from becoming multiplication.
    Regex::new(
        r"(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)(?:[\t ]*\*[\t ]*[+-]?(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+))+",
    )
    .expect("Voxi multiplication pattern must be valid")
});

static WEB_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b(?:(?:https?|ftp)://)?(?:www\.)?((?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,63})\b(?::[0-9]+)?(?:[/?#][^\s<>"']*)?"#,
    )
    .expect("Voxi web URL pattern must be valid")
});

static FILE_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bfile:///[a-z]:/[^\s<>"']*"#).expect("Voxi file URL pattern must be valid")
});

static AUTOLINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<((?:https?|ftp|file)://[^<>\r\n]+)>")
        .expect("Voxi autolink pattern must be valid")
});

static DICTIONARY: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    use FilterCategory::*;
    let mut rules = Vec::new();

    let mut add_regex = |category: FilterCategory, pattern: &str, replacement: &'static str| {
        rules.push(Rule {
            category,
            re: Regex::new(pattern).expect("Voxi dictionary patterns must be valid"),
            replacement,
        });
    };

    // Expand multi-character operators before the speech engine interprets
    // their punctuation one character at a time. Handle the longer JavaScript
    // form first so the != rule cannot split it.
    add_regex(Pronunciation, r"!==", " is not strictly equal to ");
    add_regex(Pronunciation, r"[\t ]*!=[\t ]*", " is not equal to ");
    add_regex(
        Pronunciation,
        r"[\t ]*(?:<=|≤)[\t ]*",
        " is less than or equal to ",
    );
    add_regex(
        Pronunciation,
        r"[\t ]*(?:>=|≥)[\t ]*",
        " is greater than or equal to ",
    );
    add_regex(Pronunciation, r"[\t ]*≠[\t ]*", " is not equal to ");
    add_regex(Pronunciation, r"[\t ]*≈[\t ]*", " approximately ");
    add_regex(Pronunciation, r"[\t ]*×[\t ]*", " times ");
    add_regex(Pronunciation, r"[\t ]*÷[\t ]*", " divided by ");
    add_regex(Pronunciation, r"[\t ]*±[\t ]*", " plus or minus ");

    // Copied UI prompts are clutter only on their own line; mid-sentence mentions stay.
    for prompt in [
        "To view keyboard shortcuts, press question mark",
        "View keyboard shortcuts",
        "Next Reply",
    ] {
        add_regex(
            Cleanup,
            &format!(r"(?im)^[\t ]*{}[\t ]*(?:\r?\n|$)", regex::escape(prompt)),
            "",
        );
    }

    let mut add = |category: FilterCategory,
                   phrase: &str,
                   replacement: &'static str,
                   word_boundaries: bool| {
        let pattern = if word_boundaries {
            format!(r"(?i)\b{}\b", regex::escape(phrase))
        } else {
            format!(r"(?i){}", regex::escape(phrase))
        };
        rules.push(Rule {
            category,
            re: Regex::new(&pattern).expect("escaped Voxi dictionary patterns must be valid"),
            replacement,
        });
    };

    add(Cleanup, "*", "", false);

    add(Emoji, "😭", " Sob ", false);
    add(Emoji, "😂", " Joy ", false);
    add(Emoji, "🔥", " Fire ", false);
    add(Emoji, "❤️", " Heart ", false);
    add(Emoji, "👍", " Thumbs up ", false);
    add(Emoji, "🎉", " Party ", false);

    add(Pronunciation, "Ableton", "Abelten", true);
    add(Pronunciation, "AOC", "A.O.C.", true);
    add(Pronunciation, "Aesop", "Ace-op", true);
    add(Pronunciation, "Aes", "Ace", true);
    add(Pronunciation, "Bastiat", "Bah-stee-aught", true);
    add(Pronunciation, "Calendly", "Cal-endly", true);
    add(Pronunciation, "Camus", "Camu", true);
    add(Pronunciation, "Carrd", "Card", true);
    add(Pronunciation, "Cerave", "CeraVee", true);
    add(Pronunciation, "Conversion", "Convursion", true);
    add(Pronunciation, "CopyQ", "CopyCue", true);
    add(Pronunciation, "Cuck", "Cuhck", true);
    add(Pronunciation, "Culinary", "Cullinary", true);
    add(Pronunciation, "Chapo", "Chap-o", true);
    add(Pronunciation, "Chatgpt", "ChatGPT", true);
    add(Pronunciation, "DeSantis", "De-Santis", true);
    add(Pronunciation, "DMing", "D-M-ing", true);
    add(Pronunciation, "Doja", "Doeja", true);
    add(Pronunciation, "Elgato", "El-got-o", true);
    add(Pronunciation, "Fage", "Fa-yay", true);
    add(Pronunciation, "Ghibli", "Jiblee", true);
    add(Pronunciation, "Giga", "Gigga", true);
    add(Pronunciation, "Github", "GitHub", true);
    add(Pronunciation, "Glutes", "Glootes", true);
    add(Pronunciation, "Goku", "Go-ku", true);
    add(Pronunciation, "Hormozi", "Hormoezee", true);
    add(Pronunciation, "Huberman", "Hewberman", true);
    add(Pronunciation, "JavaScript", "Java-Script", true);
    add(Pronunciation, "Joji", "Joegee", true);
    add(Pronunciation, "Kasa", "Casa", true);
    add(Pronunciation, "Kayfabe", "Kay-fabe", true);
    add(Pronunciation, "Kimya", "Kim-ya", true);
    add(Pronunciation, "Kobe", "Co-be", true);
    add(Pronunciation, "LeadSynth.com", "LeadSynth dot com", false);
    add(Pronunciation, "Leevi", "Levy", true);
    add(Pronunciation, "Leila", "Layla", true);
    add(Pronunciation, "Livestream", "Lyevstream", true);
    add(Pronunciation, "Monetiz", "Mahnetiz", false);
    add(Pronunciation, "Mozi", "Moezee", true);
    add(Pronunciation, "Munger", "Mun-gir", true);
    add(Pronunciation, "Pantone", "Pan-tone", true);
    add(Pronunciation, "Paracord", "Parahcord", true);
    add(Pronunciation, "PreCheck", "Pre-Check", true);
    add(Pronunciation, "Rapport", "Rapore", true);
    add(Pronunciation, "Rangeman", "Range-Man", true);
    add(Pronunciation, "RevShare", "Rev-Share", true);
    add(Pronunciation, "Schopenhauer", "Showpenhower", true);
    add(Pronunciation, "Sneako", "Sneak-o", true);
    add(Pronunciation, "Tiktok", "TikTok", true);
    add(Pronunciation, "ToDos", "To Dos", true);
    add(Pronunciation, "ToDo", "To Do", true);
    add(Pronunciation, "Toup", "Tooop", true);
    add(Pronunciation, "Upsell", "Up-sell", true);
    add(Pronunciation, "Vegeta", "Veg-eatuh", true);
    add(Pronunciation, "Webhook", "Web-hook", true);
    add(Pronunciation, "Whitespace", "White-space", true);
    add(Pronunciation, "Wordcel", "Wordcell", true);
    add(Pronunciation, "Xmas", "Christmas", true);
    add(Pronunciation, "Zherka", "Zerka", true);

    add(Abbreviations, "AFAICT", "As far as I can tell", true);
    add(Abbreviations, "AFAIK", "As far as I know", true);
    add(Abbreviations, "IIRC", "If I recall correctly", true);
    add(Abbreviations, "IMO", "In my opinion", true);
    add(Abbreviations, "SEO", "S-E-O", true);
    add(Abbreviations, "TBQH", "To be quite honest", true);
    add(Abbreviations, "TBH", "To be honest", true);
    add(Abbreviations, "YC", "Y-C", true);

    rules
});

// Uppercase only, and not beside a dot, so "example.fr" & "fr" are left alone.
static FR_ABBREVIATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bFR\b").expect("Voxi FR pattern must be valid"));

pub(crate) fn initialize() {
    LazyLock::force(&FR_ABBREVIATION);
    LazyLock::force(&MULTIPLICATION);
    LazyLock::force(&WEB_URL);
    LazyLock::force(&FILE_URL);
    LazyLock::force(&AUTOLINK);
    LazyLock::force(&DICTIONARY);
}

fn trailing_url_punctuation(url: &str) -> &str {
    let mut parentheses = url.matches(')').count() as isize - url.matches('(').count() as isize;
    let mut brackets = url.matches(']').count() as isize - url.matches('[').count() as isize;
    let mut braces = url.matches('}').count() as isize - url.matches('{').count() as isize;
    let mut end = url.len();
    for (offset, character) in url.char_indices().rev() {
        match character {
            '.' | ',' | '!' | '?' | ';' | ':' | '。' | '！' | '？' => {}
            ')' if parentheses > 0 => parentheses -= 1,
            ']' if brackets > 0 => brackets -= 1,
            '}' if braces > 0 => braces -= 1,
            _ => break,
        }
        end = offset;
    }
    &url[end..]
}

fn expand_multiplication(text: &str) -> Cow<'_, str> {
    MULTIPLICATION.replace_all(text, |captures: &regex::Captures<'_>| {
        captures[0]
            .split('*')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" times ")
    })
}

fn markdown_text(text: &str) -> String {
    // Entities were already decoded once. Protect remaining ampersands from a
    // second decode by the Markdown parser, including inside code/HTML events.
    let protected = text.replace('&', "&amp;");
    let mut result = String::with_capacity(text.len());
    for event in Parser::new(&protected) {
        match event {
            Event::Text(value) => result.push_str(&value),
            Event::Code(value) | Event::Html(value) | Event::InlineHtml(value) => {
                result.push_str(&html_escape::decode_html_entities(&value));
            }
            Event::SoftBreak | Event::HardBreak | Event::Rule => result.push('\n'),
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock) => {
                result.push_str("\n\n");
            }
            Event::End(TagEnd::Item) if !result.ends_with('\n') => {
                result.push('\n');
            }
            _ => {}
        }
    }
    result.trim_end_matches('\n').to_owned()
}

fn expand_standalone_fr(text: &str) -> Cow<'_, str> {
    FR_ABBREVIATION.replace_all(text, |captures: &regex::Captures<'_>| {
        let found = captures.get(0).expect("a match has a whole capture");
        let before = text[..found.start()].chars().next_back();
        let after = text[found.end()..].chars().next();
        if before == Some('.') || after == Some('.') {
            "FR".to_owned()
        } else {
            "For Real".to_owned()
        }
    })
}

fn preprocess_text(text: &str, options: FilterOptions) -> String {
    let decoded = if options.cleanup {
        html_escape::decode_html_entities(text)
    } else {
        Cow::Borrowed(text)
    };
    let normalized: String = decoded
        .chars()
        .map(|character| match character {
            // Copied web text often uses nonbreaking or thin spaces and U+2212.
            '−' => '-',
            '\u{0085}' | '\u{2028}' | '\u{2029}' => '\n',
            c if c.is_whitespace() && !matches!(c, '\r' | '\n') => ' ',
            // NUL would truncate the null-terminated input passed to SAPI.
            '\0' => ' ',
            c => c,
        })
        .collect();
    let mut processed = normalized;
    if options.cleanup {
        let autolinks = AUTOLINK.replace_all(&processed, "$1");
        let files = FILE_URL.replace_all(&autolinks, |captures: &regex::Captures<'_>| {
            format!("file{}", trailing_url_punctuation(&captures[0]))
        });
        let urls = WEB_URL.replace_all(&files, |captures: &regex::Captures<'_>| {
            format!("{}{}", &captures[1], trailing_url_punctuation(&captures[0]))
        });
        processed = urls.into_owned();
    }
    if options.pronunciation {
        // Expand numeric multiplication before the dictionary mutes other asterisks.
        // Do this before Markdown too: 2*3*4 otherwise looks like emphasis.
        processed = expand_multiplication(&processed).into_owned();
    }
    if options.cleanup {
        processed = markdown_text(&processed);
    }
    if options.pronunciation {
        // Formatting or escaped stars can hide operands until Markdown is removed.
        processed = expand_multiplication(&processed).into_owned();
    }
    for rule in DICTIONARY.iter() {
        if options.is_enabled(rule.category) && rule.re.is_match(&processed) {
            processed = rule
                .re
                .replace_all(&processed, rule.replacement)
                .into_owned();
        }
    }
    if options.abbreviations {
        processed = expand_standalone_fr(&processed).into_owned();
    }
    processed
}

// The caller passes already-cleaned text, so restarting speech cannot apply
// dictionary substitutions or entity decoding twice.
pub(crate) fn to_sapi_xml(processed: &str) -> String {
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

    format!("<speak version='1.0'>{escaped}</speak>")
}

pub(crate) fn to_plain_text(text: &str, options: FilterOptions) -> String {
    preprocess_text(text, options)
}

#[cfg(test)]
mod tests {
    use super::{to_sapi_xml, FilterCategory, FilterOptions};

    fn preprocess_text(text: &str) -> String {
        super::preprocess_text(text, FilterOptions::STANDARD)
    }

    fn to_plain_text(text: &str) -> String {
        super::to_plain_text(text, FilterOptions::STANDARD)
    }

    #[test]
    fn raw_mode_reads_text_as_is_but_stays_xml_safe() {
        let raw = |text| super::to_plain_text(text, FilterOptions::RAW);
        assert_eq!(
            raw("AFAIK <ready> **2*3** https://example.com/a 😂"),
            "AFAIK <ready> **2*3** https://example.com/a 😂"
        );
        assert_eq!(raw("Tom &amp; Jerry"), "Tom &amp; Jerry");
        assert_eq!(raw("before\0after"), "before after");
        assert_eq!(
            to_sapi_xml(&raw("AFAIK <ready> &")),
            "<speak version='1.0'>AFAIK &lt;ready&gt; &amp;</speak>"
        );
    }

    #[test]
    fn each_category_can_be_isolated() {
        let only = |category| FilterOptions::RAW.toggled(category);
        let text = "AFAIK Ghibli 😂 at https://example.com/a";
        assert_eq!(
            super::to_plain_text(text, only(FilterCategory::Cleanup)),
            "AFAIK Ghibli 😂 at example.com"
        );
        assert_eq!(
            super::to_plain_text(text, only(FilterCategory::Pronunciation)),
            "AFAIK Jiblee 😂 at https://example.com/a"
        );
        assert_eq!(
            super::to_plain_text(text, only(FilterCategory::Abbreviations)),
            "As far as I know Ghibli 😂 at https://example.com/a"
        );
        assert_eq!(
            super::to_plain_text(text, only(FilterCategory::Emoji)),
            "AFAIK Ghibli  Joy  at https://example.com/a"
        );
    }

    #[test]
    fn filter_options_label_presets_and_survive_the_settings_mask() {
        assert_eq!(FilterOptions::STANDARD.label(), "Standard");
        assert_eq!(FilterOptions::RAW.label(), "Raw");
        let custom = FilterOptions::STANDARD.toggled(FilterCategory::Emoji);
        assert_eq!(custom.label(), "Custom");
        assert!(!custom.is_enabled(FilterCategory::Emoji));
        assert_eq!(FilterOptions::from_mask(custom.mask()), custom);
        assert_eq!(FilterOptions::from_mask(0xFF), FilterOptions::STANDARD);
    }

    #[test]
    fn fr_expands_only_as_a_standalone_uppercase_word() {
        assert_eq!(preprocess_text("FR that works"), "For Real that works");
        assert_eq!(
            preprocess_text("Visit example.fr today"),
            "Visit example.fr today"
        );
        assert_eq!(preprocess_text("fr is lowercase"), "fr is lowercase");
    }

    #[test]
    fn ui_prompts_are_removed_only_from_their_own_line() {
        assert_eq!(preprocess_text("Hello\nNext Reply\nWorld"), "Hello\nWorld");
        assert_eq!(
            preprocess_text("Click Next Reply to continue"),
            "Click Next Reply to continue"
        );
    }

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
            "status is not equal to ready"
        );
        assert_eq!(to_plain_text("a!=b"), "a is not equal to b");
        assert_eq!(
            to_sapi_xml(&to_plain_text("a != b")),
            "<speak version='1.0'>a is not equal to b</speak>"
        );
        assert_eq!(to_plain_text("a!==b"), "a is not strictly equal to b");
    }

    #[test]
    fn reads_numeric_multiplication_before_muting_asterisks() {
        for (input, expected) in [
            ("2*3", "2 times 3"),
            ("2 * 3", "2 times 3"),
            ("12\t*\t34", "12 times 34"),
            ("2*3*4*5", "2 times 3 times 4 times 5"),
            ("12.5 * 3.25", "12.5 times 3.25"),
            ("-2 * -3 * +4", "-2 times -3 times +4"),
            (".5*.25", ".5 times .25"),
            ("2\u{a0}*\u{202f}3", "2 times 3"),
            ("2 * −3", "2 times -3"),
            ("2&nbsp;*&nbsp;3", "2 times 3"),
            ("2&#x20;*&#32;3*4", "2 times 3 times 4"),
            ("**2** * **3**", "2 times 3"),
            (r"2\*3\*4", "2 times 3 times 4"),
            ("**2 * 3** & *4*", "2 times 3 & 4"),
        ] {
            assert_eq!(to_plain_text(input), expected, "input: {input}");
        }
        assert_eq!(
            to_sapi_xml(&to_plain_text("**2*3*4** & *ready*")),
            "<speak version='1.0'>2 times 3 times 4 &amp; ready</speak>"
        );
    }

    #[test]
    fn mutes_non_multiplication_asterisks_without_crossing_lines() {
        for (input, expected) in [
            ("*", ""),
            ("***", ""),
            ("*hello* **world**", "hello world"),
            ("* bullet\n* next", "bullet\nnext"),
            ("Note 2*", "Note 2"),
            ("a*b", "ab"),
            ("2**3", "23"),
            ("2\n* 3", "2\n\n3"),
            ("2 *\r\n3", "2 \n3"),
        ] {
            assert_eq!(to_plain_text(input), expected, "input: {input}");
        }
    }

    #[test]
    fn escapes_xml_before_wrapping_for_sapi() {
        assert_eq!(
            to_sapi_xml("A < B & C's \"quote\""),
            "<speak version='1.0'>A &lt; B &amp; C&apos;s &quot;quote&quot;</speak>"
        );
    }

    #[test]
    fn leaves_silence_to_the_shared_audio_queue() {
        assert_eq!(to_sapi_xml("Ready"), "<speak version='1.0'>Ready</speak>");
    }

    #[test]
    fn natural_voice_plain_text_keeps_dictionary_without_xml() {
        assert_eq!(to_plain_text("AFAIK <ready>"), "As far as I know <ready>");
    }

    #[test]
    fn simplifies_urls_without_swallowing_punctuation_or_following_words() {
        for (input, expected) in [
            (
                "See https://example.com/article. Next sentence.",
                "See example.com. Next sentence.",
            ),
            (
                "Open file:///C:/notes.txt then continue reading.",
                "Open file then continue reading.",
            ),
            ("Open file:///C:/My%20Notes.txt. Next.", "Open file. Next."),
            ("See (https://example.com/a).", "See (example.com)."),
            (
                "Visit https://example.com/a, then https://other.org/b!",
                "Visit example.com, then other.org!",
            ),
            (
                "https://example.com/?x=2*3. Continue.",
                "example.com. Continue.",
            ),
            ("Email james@example.com.", "Email james@example.com."),
            (
                "See https://example.com/a_(b). Next.",
                "See example.com. Next.",
            ),
            ("See (https://example.com/a_(b)).", "See (example.com)."),
        ] {
            assert_eq!(to_plain_text(input), expected, "input: {input}");
        }
    }

    #[test]
    fn reads_markdown_content_and_link_labels_without_formatting() {
        for (input, expected) in [
            ("# Heading", "Heading"),
            ("#### Heading", "Heading"),
            ("__important__ & _clear_", "important & clear"),
            ("Read `hello_world` now.", "Read hello_world now."),
            ("[Read this](https://example.com/article)", "Read this"),
            ("[Read this](https://example.com/a_(b))", "Read this"),
            ("<https://example.com/a>", "example.com"),
            (
                "[**Useful** guide](https://example.com/a \"Title\")",
                "Useful guide",
            ),
            (
                "Read [guide][g].\n\n[g]: https://example.com/a",
                "Read guide.",
            ),
            (
                "## Heading\n\nFirst paragraph.\n\nSecond paragraph.",
                "Heading\n\nFirst paragraph.\n\nSecond paragraph.",
            ),
            (
                "```rust\nlet hello_world = 2*3;\n```",
                "let hello_world = 2 times 3;",
            ),
            ("snake_case C# F# #1", "snake_case C# F# #1"),
        ] {
            assert_eq!(to_plain_text(input), expected, "input: {input}");
        }
    }

    #[test]
    fn decodes_entities_once_without_turning_them_into_speech_commands() {
        for (input, expected) in [
            ("Tom &amp; Jerry &#x20; next", "Tom & Jerry   next"),
            ("A&nbsp;B &#169; &copy;", "A B © ©"),
            ("&amp;lt;ready&amp;gt;", "&lt;ready&gt;"),
            ("`Tom &amp; Jerry`", "Tom & Jerry"),
            ("&unknown; &#x110000;", "&unknown; &#x110000;"),
            ("before\0after", "before after"),
        ] {
            assert_eq!(to_plain_text(input), expected, "input: {input}");
        }
        assert_eq!(
            to_sapi_xml(&to_plain_text("&lt;silence msec='99999'/&gt;")),
            "<speak version='1.0'>&lt;silence msec=&apos;99999&apos;/&gt;</speak>"
        );
    }

    #[test]
    fn expands_complete_math_symbols_and_comparisons() {
        for (input, expected) in [
            ("2≤3", "2 is less than or equal to 3"),
            ("2 <= 3", "2 is less than or equal to 3"),
            ("4≥3", "4 is greater than or equal to 3"),
            ("4 >= 3", "4 is greater than or equal to 3"),
            ("2≠3", "2 is not equal to 3"),
            ("2≈3", "2 approximately 3"),
            ("2×3", "2 times 3"),
            ("6 ÷ 2", "6 divided by 2"),
            ("10 ± 5", "10 plus or minus 5"),
            ("2 &le; 3", "2 is less than or equal to 3"),
        ] {
            assert_eq!(to_plain_text(input), expected, "input: {input}");
        }
    }
}
