use pulldown_cmark::{escape, html, BrokenLink, CowStr, Event, Options, Parser, Tag};
use rustdoc_types::Id;
use std::borrow::Cow;
use std::{fmt, io, str};

/// Convert an std::fmt::Wrte to std::io::Write
struct Adapter<'a> {
    f: &'a mut dyn fmt::Write,
}

impl<'a> io::Write for Adapter<'a> {
    fn write(&mut self, b: &[u8]) -> Result<usize, io::Error> {
        let s = str::from_utf8(b).map_err(|_| io::Error::from(io::ErrorKind::Other))?;
        self.f
            .write_str(s)
            .map_err(|_| io::Error::from(io::ErrorKind::Other))?;
        Ok(b.len())
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        Ok(())
    }
}

/// Options for rendering Markdown in the main body of documentation.
fn opts() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION
}

/// A subset of [`opts()`] used for rendering summaries.
fn summary_opts() -> Options {
    Options::ENABLE_STRIKETHROUGH | Options::ENABLE_SMART_PUNCTUATION | Options::ENABLE_TABLES
}

/// Render the all Markdown in html
pub(crate) struct Markdown<'context, 'krate, 'content>(
    &'context crate::html::render::GlobalContext<'krate>,
    &'context crate::html::render::PageContext<'context>,
    &'content String,
    &'krate std::collections::HashMap<String, Id>,
);

impl<'context, 'krate, 'content> Markdown<'context, 'krate, 'content> {
    pub(crate) fn from_docs(
        global_context: &'context crate::html::render::GlobalContext<'krate>,
        page_context: &'context crate::html::render::PageContext<'context>,
        content: &'content Option<String>,
        links: &'krate std::collections::HashMap<String, Id>,
    ) -> Option<Self> {
        if let Some(content) = content {
            Some(Self(global_context, page_context, content, links))
        } else {
            None
        }
    }
}

impl<'context, 'krate, 'content> markup::Render for Markdown<'context, 'krate, 'content> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        if !self.2.is_empty() {
            let adapter = Adapter { f: writer };

            let mut replacer = |broken_link: BrokenLink<'_>| {
                //dbg!(broken_link.reference);
                if let Some(id) = self.3.get(broken_link.reference) {
                    if let Some((external_crate_url, relative, fragment, _type_of)) =
                        crate::html::render::href(self.0, self.1, id)
                    {
                        Some((
                            {
                                let mut href = String::new();

                                if let Some(external_crate_url) = external_crate_url {
                                    href.push_str(external_crate_url);
                                }
                                href.push_str(
                                    relative.to_str().expect("cannot convert PathBuf to str"),
                                );
                                if let Some(fragment) = fragment {
                                    href.push_str(&fragment);
                                }

                                CowStr::Boxed(href.into_boxed_str())
                            },
                            CowStr::Boxed(broken_link.reference.to_string().into_boxed_str()),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            let parser = Parser::new_with_broken_link_callback(self.2, opts(), Some(&mut replacer));
            let parser = CodeBlocks::new(parser);
            let parser = Headings::new(parser, None);

            html::write_html(adapter, parser).unwrap();
        }
        Ok(())
    }
}

/// Render the all Markdown in html
pub struct MarkdownWithToc<'context, 'krate, 'content, 'vec>(
    &'context crate::html::render::GlobalContext<'krate>,
    &'context crate::html::render::PageContext<'context>,
    &'content String,
    &'krate std::collections::HashMap<String, Id>,
    // RefCell required here because of the immutable `&self` on `render`
    pub(crate) std::cell::RefCell<&'vec mut Vec<(u32, String, String)>>,
);

impl<'context, 'krate, 'content, 'vec> MarkdownWithToc<'context, 'krate, 'content, 'vec> {
    pub(crate) fn from_docs(
        global_context: &'context crate::html::render::GlobalContext<'krate>,
        page_context: &'context crate::html::render::PageContext<'context>,
        content: &'content Option<String>,
        links: &'krate std::collections::HashMap<String, Id>,
        toc: &'vec mut Vec<(u32, String, String)>,
    ) -> Option<Self> {
        if let Some(content) = content {
            Some(Self(
                global_context,
                page_context,
                content,
                links,
                std::cell::RefCell::new(toc),
            ))
        } else {
            None
        }
    }
}

impl<'context, 'krate, 'content, 'vec> markup::Render
    for MarkdownWithToc<'context, 'krate, 'content, 'vec>
{
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        if !self.2.is_empty() {
            let adapter = Adapter { f: writer };

            let gloabl_context = self.0;
            let page_context = self.1;
            let ids = self.3;
            let mut replacer = |broken_link: BrokenLink<'_>| {
                //dbg!(broken_link.reference);
                if let Some(id) = ids.get(broken_link.reference) {
                    if let Some((external_crate_url, relative, fragment, _type_of)) =
                        crate::html::render::href(gloabl_context, page_context, id)
                    {
                        Some((
                            {
                                let mut href = String::new();

                                if let Some(external_crate_url) = external_crate_url {
                                    href.push_str(external_crate_url);
                                }
                                href.push_str(
                                    relative.to_str().expect("cannot convert PathBuf to str"),
                                );
                                if let Some(fragment) = fragment {
                                    href.push_str(&fragment);
                                }

                                CowStr::Boxed(href.into_boxed_str())
                            },
                            CowStr::Boxed(broken_link.reference.to_string().into_boxed_str()),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            let parser = Parser::new_with_broken_link_callback(self.2, opts(), Some(&mut replacer));
            let parser = CodeBlocks::new(parser);

            let mut tmp = self.4.borrow_mut();

            let parser = Headings::new(parser, Some(*tmp));

            html::write_html(adapter, parser).unwrap();
        }
        Ok(())
    }
}

/// Render an summary line of the Markdown in html
pub(crate) struct MarkdownSummaryLine<'content>(
    &'content String,
);

impl<'content> MarkdownSummaryLine<'content> {
    pub(crate) fn from_docs(
        content: &'content Option<String>,
    ) -> Option<Self> {
        if let Some(content) = content {
            Some(Self(content))
        } else {
            None
        }
    }
}

impl<'content> markup::Render for MarkdownSummaryLine<'content> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        if !self.0.is_empty() {
            let adapter = Adapter { f: writer };
            let parser = Parser::new_ext(self.0, summary_opts());
            let parser = SummaryLine::new(parser);

            html::write_html(adapter, parser).unwrap();
        }
        Ok(())
    }
}

/// Extracts just the first paragraph.
struct SummaryLine<'a, I: Iterator<Item = Event<'a>>> {
    inner: I,
    started: bool,
    depth: u32,
}

impl<'a, I: Iterator<Item = Event<'a>>> SummaryLine<'a, I> {
    fn new(iter: I) -> Self {
        SummaryLine {
            inner: iter,
            started: false,
            depth: 0,
        }
    }
}

fn check_if_allowed_tag(t: &Tag<'_>) -> bool {
    matches!(
        t,
        Tag::Paragraph | Tag::Item | Tag::Emphasis | Tag::Strong | Tag::Link(..) | Tag::BlockQuote
    )
}

fn is_forbidden_tag(t: &Tag<'_>) -> bool {
    matches!(
        t,
        Tag::CodeBlock(_) | Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell
    )
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for SummaryLine<'a, I> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.started && self.depth == 0 {
            return None;
        }
        if !self.started {
            self.started = true;
        }
        if let Some(event) = self.inner.next() {
            let mut is_start = true;
            let is_allowed_tag = match event {
                Event::Start(ref c) => {
                    if is_forbidden_tag(c) {
                        return None;
                    }
                    self.depth += 1;
                    check_if_allowed_tag(c)
                }
                Event::End(ref c) => {
                    if is_forbidden_tag(c) {
                        return None;
                    }
                    self.depth -= 1;
                    is_start = false;
                    check_if_allowed_tag(c)
                }
                _ => true,
            };
            return if !is_allowed_tag {
                if is_start {
                    Some(Event::Start(Tag::Paragraph))
                } else {
                    Some(Event::End(Tag::Paragraph))
                }
            } else {
                Some(event)
            };
        }
        None
    }
}

/// Format a litle bit diffrently the Codeblocks
struct Headings<'a, 'vec, I: Iterator<Item = Event<'a>>> {
    inner: I,
    toc: Option<&'vec mut Vec<(u32, String, String)>>,
}

impl<'a, 'vec, I: Iterator<Item = Event<'a>>> Headings<'a, 'vec, I> {
    fn new(iter: I, toc: Option<&'vec mut Vec<(u32, String, String)>>) -> Self {
        Self { inner: iter, toc }
    }
}

impl<'a, 'vec, I: Iterator<Item = Event<'a>>> Iterator for Headings<'a, 'vec, I> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.inner.next();

        let level = if let Some(Event::Start(Tag::Heading(level))) = event {
            level
        } else {
            return event;
        };

        let mut original_text = String::new();
        for event in &mut self.inner {
            match event {
                Event::End(Tag::Heading(..)) => break,
                Event::Text(ref s) => {
                    original_text.push_str(s);
                }
                _ => {}
            }
        }

        let mut id = String::new();

        for c in original_text.trim().chars() {
            if c.is_alphanumeric() {
                id.push(c.to_ascii_lowercase());
            } else if c.is_whitespace() {
                id.push('-');
            }
        }

        let html = format!(
            "<h{} class=\"item-doc-heading rd-anchor\" id=\"{}\"><a href=\"#{}\">{}</a></h{}>",
            level, id, id, original_text, level
        )
        .into();

        if let Some(ref mut toc) = self.toc {
            toc.push((level, original_text, id));
        }

        Some(Event::Html(html))
    }
}

/// Format a litle bit diffrently the Codeblocks
struct CodeBlocks<'a, I: Iterator<Item = Event<'a>>> {
    inner: I,
}

impl<'a, I: Iterator<Item = Event<'a>>> CodeBlocks<'a, I> {
    fn new(iter: I) -> Self {
        Self { inner: iter }
    }
}

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for CodeBlocks<'a, I> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.inner.next();

        let kind = if let Some(Event::Start(Tag::CodeBlock(kind))) = event {
            kind
        } else {
            return event;
        };

        let _lang = match kind {
            pulldown_cmark::CodeBlockKind::Indented => Default::default(),
            pulldown_cmark::CodeBlockKind::Fenced(ref lang_string) => {
                let lang = LangString::parse(lang_string);

                if !lang.rust {
                    return Some(Event::Start(Tag::CodeBlock(kind)));
                }

                lang
            }
        };

        let mut original_code = String::new();
        for event in &mut self.inner {
            match event {
                Event::End(Tag::CodeBlock(..)) => break,
                Event::Text(ref s) => {
                    original_code.push_str(s);
                }
                _ => {}
            }
        }

        let lines = original_code.lines().filter_map(|l| {
            let trimmed = l.trim();
            if trimmed.starts_with("##") {
                Some(Cow::Owned(l.replacen("##", "#", 1)))
            } else if let Some(_) = trimmed.strip_prefix("# ") {
                // # text
                None
            } else if trimmed == "#" {
                // We cannot handle '#text' because it could be #[attr].
                None
            } else {
                Some(Cow::Borrowed(l))
            }
        });
        let stripped_code = lines.collect::<Vec<Cow<'_, str>>>().join("\n");

        let mut html = String::with_capacity(50 + stripped_code.len());
        html.push_str("<pre class=\"rust\"><code>");
        escape::escape_html(&mut html, &stripped_code).unwrap();
        html.push_str("</code></pre>");

        Some(Event::Html(html.into()))
    }
}

/// Lang string parser taken directly from rustdoc

#[derive(Eq, PartialEq, Clone, Debug)]
struct LangString {
    original: String,
    should_panic: bool,
    no_run: bool,
    ignore: Ignore,
    rust: bool,
    test_harness: bool,
    compile_fail: bool,
    error_codes: Vec<String>,
    allow_fail: bool,
    edition: Option<u8>,
}

#[derive(Eq, PartialEq, Clone, Debug)]
enum Ignore {
    All,
    None,
    Some(Vec<String>),
}

impl Default for LangString {
    fn default() -> Self {
        Self {
            original: String::new(),
            should_panic: false,
            no_run: false,
            ignore: Ignore::None,
            rust: true,
            test_harness: false,
            compile_fail: false,
            error_codes: Vec::new(),
            allow_fail: false,
            edition: None,
        }
    }
}

impl LangString {
    fn tokens(string: &str) -> impl Iterator<Item = &str> {
        // Pandoc, which Rust once used for generating documentation,
        // expects lang strings to be surrounded by `{}` and for each token
        // to be proceeded by a `.`. Since some of these lang strings are still
        // loose in the wild, we strip a pair of surrounding `{}` from the lang
        // string and a leading `.` from each token.

        let string = string.trim();

        let first = string.chars().next();
        let last = string.chars().last();

        let string = if first == Some('{') && last == Some('}') {
            &string[1..string.len() - 1]
        } else {
            string
        };

        string
            .split(|c| c == ',' || c == ' ' || c == '\t')
            .map(str::trim)
            .map(|token| {
                if token.chars().next() == Some('.') {
                    &token[1..]
                } else {
                    token
                }
            })
            .filter(|token| !token.is_empty())
    }

    fn parse(string: &str) -> LangString {
        let mut seen_rust_tags = false;
        let mut seen_other_tags = false;
        let mut data = LangString::default();
        let mut ignores = vec![];

        data.original = string.to_owned();

        let tokens = Self::tokens(string).collect::<Vec<&str>>();

        for token in tokens {
            match token {
                "should_panic" => {
                    data.should_panic = true;
                    seen_rust_tags = !seen_other_tags;
                }
                "no_run" => {
                    data.no_run = true;
                    seen_rust_tags = !seen_other_tags;
                }
                "ignore" => {
                    data.ignore = Ignore::All;
                    seen_rust_tags = !seen_other_tags;
                }
                x if x.starts_with("ignore-") => {
                    ignores.push(x.trim_start_matches("ignore-").to_owned());
                    seen_rust_tags = !seen_other_tags;
                }
                "allow_fail" => {
                    data.allow_fail = true;
                    seen_rust_tags = !seen_other_tags;
                }
                "rust" => {
                    data.rust = true;
                    seen_rust_tags = true;
                }
                "test_harness" => {
                    data.test_harness = true;
                    seen_rust_tags = !seen_other_tags || seen_rust_tags;
                }
                "compile_fail" => {
                    data.compile_fail = true;
                    seen_rust_tags = !seen_other_tags || seen_rust_tags;
                    data.no_run = true;
                }
                x if x.starts_with("edition") => {
                    data.edition = x[7..].parse::<u8>().ok();
                }
                x if x.starts_with('E') && x.len() == 5 => {
                    if x[1..].parse::<u32>().is_ok() {
                        data.error_codes.push(x.to_owned());
                        seen_rust_tags = !seen_other_tags || seen_rust_tags;
                    } else {
                        seen_other_tags = true;
                    }
                }
                x => {
                    let s = x.to_lowercase();
                    match if s == "compile-fail" || s == "compile_fail" || s == "compilefail" {
                        Some((
                            "compile_fail",
                            "the code block will either not be tested if not marked as a rust one \
                             or won't fail if it compiles successfully",
                        ))
                    } else if s == "should-panic" || s == "should_panic" || s == "shouldpanic" {
                        Some((
                            "should_panic",
                            "the code block will either not be tested if not marked as a rust one \
                             or won't fail if it doesn't panic when running",
                        ))
                    } else if s == "no-run" || s == "no_run" || s == "norun" {
                        Some((
                            "no_run",
                            "the code block will either not be tested if not marked as a rust one \
                             or will be run (which you might not want)",
                        ))
                    } else if s == "allow-fail" || s == "allow_fail" || s == "allowfail" {
                        Some((
                            "allow_fail",
                            "the code block will either not be tested if not marked as a rust one \
                             or will be run (which you might not want)",
                        ))
                    } else if s == "test-harness" || s == "test_harness" || s == "testharness" {
                        Some((
                            "test_harness",
                            "the code block will either not be tested if not marked as a rust one \
                             or the code will be wrapped inside a main function",
                        ))
                    } else {
                        None
                    } {
                        Some((flag, _help)) => {
                            tracing::warn!("unknow attribute `{}`. Did you mean `{}`?", x, flag);
                        }
                        None => {}
                    }
                    seen_other_tags = true;
                }
            }
        }

        // ignore-foo overrides ignore
        if !ignores.is_empty() {
            data.ignore = Ignore::Some(ignores);
        }

        data.rust &= !seen_other_tags || seen_rust_tags;

        data
    }
}
