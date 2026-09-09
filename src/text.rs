use pulldown_cmark::{Event, Parser, TagEnd};
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

struct Rule {
    re: Regex,
    replacement: &'static str,
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
    let mut rules = Vec::new();

    let mut add_regex = |pattern: &str, replacement: &'static str| {
        rules.push(Rule {
            re: Regex::new(pattern).expect("Voxi dictionary patterns must be valid"),
            replacement,
        });
    };

    // Expand multi-character operators before the speech engine interprets
    // their punctuation one character at a time. Handle the longer JavaScript
    // form first so the != rule cannot split it.
    add_regex(r"!==", " is not strictly equal to ");
    add_regex(r"[\t ]*!=[\t ]*", " is not equal to ");
    add_regex(r"[\t ]*(?:<=|≤)[\t ]*", " is less than or equal to ");
    add_regex(r"[\t ]*(?:>=|≥)[\t ]*", " is greater than or equal to ");
    add_regex(r"[\t ]*≠[\t ]*", " is not equal to ");
    add_regex(r"[\t ]*≈[\t ]*", " approximately ");
    add_regex(r"[\t ]*×[\t ]*", " times ");
    add_regex(r"[\t ]*÷[\t ]*", " divided by ");
    add_regex(r"[\t ]*±[\t ]*", " plus or minus ");

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
    add("*", "", false);

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

fn preprocess_text(text: &str) -> String {
    let decoded = html_escape::decode_html_entities(text);
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
    let autolinks = AUTOLINK.replace_all(&normalized, "$1");
    let files = FILE_URL.replace_all(&autolinks, |captures: &regex::Captures<'_>| {
        format!("file{}", trailing_url_punctuation(&captures[0]))
    });
    let urls = WEB_URL.replace_all(&files, |captures: &regex::Captures<'_>| {
        format!("{}{}", &captures[1], trailing_url_punctuation(&captures[0]))
    });
    // Expand numeric multiplication before the dictionary mutes other asterisks.
    // Do this before Markdown too: 2*3*4 otherwise looks like emphasis.
    let multiplied = expand_multiplication(&urls);
    let markdown = markdown_text(&multiplied);
    // Formatting or escaped stars can hide operands until Markdown is removed.
    let mut processed = expand_multiplication(&markdown).into_owned();
    for rule in DICTIONARY.iter() {
        if rule.re.is_match(&processed) {
            processed = rule
                .re
                .replace_all(&processed, rule.replacement)
                .into_owned();
        }
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

pub(crate) fn to_plain_text(text: &str) -> String {
    preprocess_text(text)
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
