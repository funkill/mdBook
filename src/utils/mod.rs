#![allow(missing_docs)] // FIXME: Document this

pub mod fs;
mod string;
use crate::errors::Error;
use regex::Regex;

use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};

use std::borrow::Cow;

pub use self::string::take_lines;

/// Replaces multiple consecutive whitespace characters with a single space character.
pub fn collapse_whitespace(text: &str) -> Cow<'_, str> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"\s\s+").unwrap();
    }
    RE.replace_all(text, " ")
}

/// Convert the given string to a valid HTML element ID.
/// The only restriction is that the ID must not contain any ASCII whitespace.
pub fn normalize_id(content: &str) -> String {
    content
        .chars()
        .filter_map(|ch| {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
}

/// Generate an ID for use with anchors which is derived from a "normalised"
/// string.
pub fn id_from_content(content: &str) -> String {
    let mut content = content.to_string();

    // Skip any tags or html-encoded stuff
    const REPL_SUB: &[&str] = &[
        "<em>",
        "</em>",
        "<code>",
        "</code>",
        "<strong>",
        "</strong>",
        "&lt;",
        "&gt;",
        "&amp;",
        "&#39;",
        "&quot;",
    ];
    for sub in REPL_SUB {
        content = content.replace(sub, "");
    }

    // Remove spaces and hashes indicating a header
    let trimmed = content.trim().trim_start_matches('#').trim();

    normalize_id(trimmed)
}

fn adjust_links<'a>(event: Event<'a>, with_base: &str) -> Event<'a> {
    lazy_static! {
        static ref SCHEME_LINK: Regex = Regex::new(r"^[a-z][a-z0-9+.-]*:").unwrap();
        static ref MD_LINK: Regex = Regex::new(r"(?P<link>.*)\.md(?P<anchor>#.*)?").unwrap();
    }

    fn fix<'a>(dest: CowStr<'a>, base: &str) -> CowStr<'a> {
        // Don't modify links with schemes like `https`.
        if !SCHEME_LINK.is_match(&dest) {
            // This is a relative link, adjust it as necessary.
            let mut fixed_link = String::new();
            if !base.is_empty() {
                fixed_link.push_str(base);
                fixed_link.push_str("/");
            }

            if let Some(caps) = MD_LINK.captures(&dest) {
                fixed_link.push_str(&caps["link"]);
                fixed_link.push_str(".html");
                if let Some(anchor) = caps.name("anchor") {
                    fixed_link.push_str(anchor.as_str());
                }
            } else {
                fixed_link.push_str(&dest);
            };
            return CowStr::from(fixed_link);
        }
        dest
    }

    match event {
        Event::Start(Tag::Link(link_type, dest, title)) => {
            Event::Start(Tag::Link(link_type, fix(dest, with_base), title))
        }
        Event::Start(Tag::Image(link_type, dest, title)) => {
            Event::Start(Tag::Image(link_type, fix(dest, with_base), title))
        }
        _ => event,
    }
}

/// Wrapper around the pulldown-cmark parser for rendering markdown to HTML.
pub fn render_markdown(text: &str, curly_quotes: bool) -> String {
    render_markdown_with_base(text, curly_quotes, "")
}

pub fn new_cmark_parser(text: &str) -> Parser<'_> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    Parser::new_ext(text, opts)
}

pub fn render_markdown_with_base(text: &str, curly_quotes: bool, base: &str) -> String {
    let mut s = String::with_capacity(text.len() * 3 / 2);
    let p = new_cmark_parser(text);
    let mut converter = EventQuoteConverter::new(curly_quotes);
    let events = p
        .map(clean_codeblock_headers)
        .map(|event| adjust_links(event, base))
        .map(|event| converter.convert(event));

    html::push_html(&mut s, events);
    s
}

struct EventQuoteConverter {
    enabled: bool,
    convert_text: bool,
}

impl EventQuoteConverter {
    fn new(enabled: bool) -> Self {
        EventQuoteConverter {
            enabled,
            convert_text: true,
        }
    }

    fn convert<'a>(&mut self, event: Event<'a>) -> Event<'a> {
        if !self.enabled {
            return event;
        }

        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                self.convert_text = false;
                event
            }
            Event::End(Tag::CodeBlock(_)) => {
                self.convert_text = true;
                event
            }
            Event::Text(ref text) if self.convert_text => {
                Event::Text(CowStr::from(convert_quotes_to_curly(text)))
            }
            _ => event,
        }
    }
}

fn clean_codeblock_headers(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Start(Tag::CodeBlock(ref info)) => {
            let info: String = info.chars().filter(|ch| !ch.is_whitespace()).collect();

            Event::Start(Tag::CodeBlock(CowStr::from(info)))
        }
        _ => event,
    }
}

fn convert_quotes_to_curly(original_text: &str) -> String {
    // We'll consider the start to be "whitespace".
    let mut preceded_by_whitespace = true;

    original_text
        .chars()
        .map(|original_char| {
            let converted_char = match original_char {
                '\'' => {
                    if preceded_by_whitespace {
                        '‘'
                    } else {
                        '’'
                    }
                }
                '"' => {
                    if preceded_by_whitespace {
                        '“'
                    } else {
                        '”'
                    }
                }
                _ => original_char,
            };

            preceded_by_whitespace = original_char.is_whitespace();

            converted_char
        })
        .collect()
}

/// Prints a "backtrace" of some `Error`.
pub fn log_backtrace(e: &Error) {
    error!("Error: {}", e);

    for cause in e.iter().skip(1) {
        error!("\tCaused By: {}", cause);
    }
}

#[cfg(test)]
mod tests {
    mod render_markdown {
        use super::super::render_markdown;

        #[test]
        fn preserves_external_links() {
            assert_eq!(
                render_markdown("[example](https://www.rust-lang.org/)", false),
                "<p><a href=\"https://www.rust-lang.org/\">example</a></p>\n"
            );
        }

        #[test]
        fn it_can_adjust_markdown_links() {
            assert_eq!(
                render_markdown("[example](example.md)", false),
                "<p><a href=\"example.html\">example</a></p>\n"
            );
            assert_eq!(
                render_markdown("[example_anchor](example.md#anchor)", false),
                "<p><a href=\"example.html#anchor\">example_anchor</a></p>\n"
            );

            // this anchor contains 'md' inside of it
            assert_eq!(
                render_markdown("[phantom data](foo.html#phantomdata)", false),
                "<p><a href=\"foo.html#phantomdata\">phantom data</a></p>\n"
            );
        }

        #[test]
        fn it_can_keep_quotes_straight() {
            assert_eq!(render_markdown("'one'", false), "<p>'one'</p>\n");
        }

        #[test]
        fn it_can_make_quotes_curly_except_when_they_are_in_code() {
            let input = r#"
'one'
```
'two'
```
`'three'` 'four'"#;
            let expected = r#"<p>‘one’</p>
<pre><code>'two'
</code></pre>
<p><code>'three'</code> ‘four’</p>
"#;
            assert_eq!(render_markdown(input, true), expected);
        }

        #[test]
        fn whitespace_outside_of_codeblock_header_is_preserved() {
            let input = r#"
some text with spaces
```rust
fn main() {
// code inside is unchanged
}
```
more text with spaces
"#;

            let expected = r#"<p>some text with spaces</p>
<pre><code class="language-rust">fn main() {
// code inside is unchanged
}
</code></pre>
<p>more text with spaces</p>
"#;
            assert_eq!(render_markdown(input, false), expected);
            assert_eq!(render_markdown(input, true), expected);
        }

        #[test]
        fn rust_code_block_properties_are_passed_as_space_delimited_class() {
            let input = r#"
```rust,no_run,should_panic,property_3
```
"#;

            let expected =
                r#"<pre><code class="language-rust,no_run,should_panic,property_3"></code></pre>
"#;
            assert_eq!(render_markdown(input, false), expected);
            assert_eq!(render_markdown(input, true), expected);
        }

        #[test]
        fn rust_code_block_properties_with_whitespace_are_passed_as_space_delimited_class() {
            let input = r#"
```rust,    no_run,,,should_panic , ,property_3
```
"#;

            let expected =
                r#"<pre><code class="language-rust,no_run,,,should_panic,,property_3"></code></pre>
"#;
            assert_eq!(render_markdown(input, false), expected);
            assert_eq!(render_markdown(input, true), expected);
        }

        #[test]
        fn rust_code_block_without_properties_has_proper_html_class() {
            let input = r#"
```rust
```
"#;

            let expected = r#"<pre><code class="language-rust"></code></pre>
"#;
            assert_eq!(render_markdown(input, false), expected);
            assert_eq!(render_markdown(input, true), expected);

            let input = r#"
```rust
```
"#;
            assert_eq!(render_markdown(input, false), expected);
            assert_eq!(render_markdown(input, true), expected);
        }
    }

    mod html_munging {
        use super::super::{id_from_content, normalize_id};

        #[test]
        fn it_generates_anchors() {
            assert_eq!(
                id_from_content("## Method-call expressions"),
                "method-call-expressions"
            );
            assert_eq!(id_from_content("## **Bold** title"), "bold-title");
            assert_eq!(id_from_content("## `Code` title"), "code-title");
        }

        #[test]
        fn it_generates_anchors_from_non_ascii_initial() {
            assert_eq!(
                id_from_content("## `--passes`: add more rustdoc passes"),
                "--passes-add-more-rustdoc-passes"
            );
            assert_eq!(
                id_from_content("## 中文標題 CJK title"),
                "中文標題-cjk-title"
            );
            assert_eq!(id_from_content("## Über"), "Über");
        }

        #[test]
        fn it_normalizes_ids() {
            assert_eq!(
                normalize_id("`--passes`: add more rustdoc passes"),
                "--passes-add-more-rustdoc-passes"
            );
            assert_eq!(
                normalize_id("Method-call 🐙 expressions \u{1f47c}"),
                "method-call--expressions-"
            );
            assert_eq!(normalize_id("_-_12345"), "_-_12345");
            assert_eq!(normalize_id("12345"), "12345");
            assert_eq!(normalize_id("中文"), "中文");
            assert_eq!(normalize_id("にほんご"), "にほんご");
            assert_eq!(normalize_id("한국어"), "한국어");
            assert_eq!(normalize_id(""), "");
        }
    }

    mod convert_quotes_to_curly {
        use super::super::convert_quotes_to_curly;

        #[test]
        fn it_converts_single_quotes() {
            assert_eq!(
                convert_quotes_to_curly("'one', 'two'"),
                "‘one’, ‘two’"
            );
        }

        #[test]
        fn it_converts_double_quotes() {
            assert_eq!(
                convert_quotes_to_curly(r#""one", "two""#),
                "“one”, “two”"
            );
        }

        #[test]
        fn it_treats_tab_as_whitespace() {
            assert_eq!(convert_quotes_to_curly("\t'one'"), "\t‘one’");
        }
    }
}
