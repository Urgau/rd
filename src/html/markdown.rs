//! Markdown handling for HTML output

use log::warn;
use pulldown_cmark::{escape, html, BrokenLink, CowStr, Event, HeadingLevel, Options, Parser, Tag};
use rustdoc_types::Id;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::convert::TryFrom;
use std::{fmt, io, str};

use super::id::Id as HtmlId;
use super::render::{GlobalContext, PageContext};
use super::utils::*;

/// Convert an std::fmt::Write to std::io::Write
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
pub(super) struct Markdown<'context, 'krate, 'content>(
    &'context GlobalContext<'krate>,
    &'context PageContext<'context>,
    Option<&'context HtmlId>,
    &'content String,
    &'krate HashMap<String, Id>,
);

impl<'context, 'krate, 'content> Markdown<'context, 'krate, 'content> {
    /// Create a [`Markdown`] struct from some context and a content
    pub(super) fn from_docs(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        parent_id: Option<&'context HtmlId>,
        content: &'content Option<String>,
        links: &'krate HashMap<String, Id>,
    ) -> Option<Self> {
        content
            .as_ref()
            .map(|content| Self(global_context, page_context, parent_id, content, links))
    }
}

impl<'context, 'krate, 'content> markup::Render for Markdown<'context, 'krate, 'content> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        if !self.3.is_empty() {
            let adapter = Adapter { f: writer };

            let mut replacer = |broken_link: BrokenLink<'_>| {
                if let Some(id) = self.4.get(broken_link.reference.as_ref()) {
                    if let Some((external_crate_url, relative, fragment, _type_of)) =
                        href(self.0, self.1, id)
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

            let parser = Parser::new_with_broken_link_callback(self.3, opts(), Some(&mut replacer));
            let parser = CodeBlocks::new(parser);
            let parser = Headings::new(parser, self.2, self.1, None);

            html::write_html(adapter, parser).unwrap();
        }
        Ok(())
    }
}

/// Render the all Markdown in html
pub struct MarkdownWithToc<'context, 'krate, 'content>(
    &'context GlobalContext<'krate>,
    &'context PageContext<'context>,
    &'content String,
    &'krate HashMap<String, Id>,
    // RefCell required here because of the immutable `&self` on `render`
    pub(crate) RefCell<Vec<(u32, String, &'context HtmlId)>>,
);

impl<'context, 'krate, 'content> MarkdownWithToc<'context, 'krate, 'content> {
    /// Create a [`MarkdownWithToc`] struct from some context and a content
    pub(super) fn from_docs(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        content: &'content Option<String>,
        links: &'krate HashMap<String, Id>,
    ) -> Option<Self> {
        content.as_ref().map(move |content| {
            Self(
                global_context,
                page_context,
                content,
                links,
                RefCell::new(Default::default()),
            )
        })
    }
}

impl<'context, 'krate, 'content> markup::Render for MarkdownWithToc<'context, 'krate, 'content> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        if !self.2.is_empty() {
            let adapter = Adapter { f: writer };

            let gloabl_context = self.0;
            let page_context = self.1;
            let ids = self.3;
            let mut replacer = |broken_link: BrokenLink<'_>| {
                if let Some(id) = ids.get(broken_link.reference.as_ref()) {
                    if let Some((external_crate_url, relative, fragment, _type_of)) =
                        href(gloabl_context, page_context, id)
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

            let mut toc_borrow = self.4.borrow_mut();
            let parser = Headings::new(parser, None, page_context, Some(&mut toc_borrow));

            html::write_html(adapter, parser).unwrap();
        }
        Ok(())
    }
}

/// Render an summary line of the Markdown in html
pub(super) struct MarkdownSummaryLine<'context, 'krate, 'content>(
    &'context GlobalContext<'krate>,
    &'context PageContext<'context>,
    &'content String,
    &'krate HashMap<String, Id>,
);

impl<'context, 'krate, 'content> MarkdownSummaryLine<'context, 'krate, 'content> {
    /// Create a [`MarkdownSummaryLine`] struct from some context and a content
    pub(super) fn from_docs(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        content: &'content Option<String>,
        links: &'krate HashMap<String, Id>,
    ) -> Option<Self> {
        content
            .as_ref()
            .map(|content| Self(global_context, page_context, content, links))
    }
}

impl<'context, 'krate, 'content> markup::Render
    for MarkdownSummaryLine<'context, 'krate, 'content>
{
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        if !self.2.is_empty() {
            let adapter = Adapter { f: writer };

            let mut replacer = |broken_link: BrokenLink<'_>| {
                if let Some(id) = self.3.get(broken_link.reference.as_ref()) {
                    if let Some((external_crate_url, relative, fragment, _type_of)) =
                        href(self.0, self.1, id)
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

            let parser =
                Parser::new_with_broken_link_callback(self.2, summary_opts(), Some(&mut replacer));
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

impl<'a, I: Iterator<Item = Event<'a>>> Iterator for SummaryLine<'a, I> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        fn check_if_allowed_tag(t: &Tag<'_>) -> bool {
            matches!(
                t,
                Tag::Paragraph
                    | Tag::Item
                    | Tag::Emphasis
                    | Tag::Strong
                    | Tag::Link(..)
                    | Tag::BlockQuote
            )
        }

        fn is_forbidden_tag(t: &Tag<'_>) -> bool {
            matches!(
                t,
                Tag::CodeBlock(_) | Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell
            )
        }

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
struct Headings<'a, 'toc, 'context, I: Iterator<Item = Event<'a>>> {
    inner: I,
    buf: VecDeque<Event<'a>>,
    parent_id: Option<&'context HtmlId>,
    page_context: &'context PageContext<'context>,
    toc: Option<&'toc mut Vec<(u32, String, &'context HtmlId)>>,
}

impl<'a, 'toc, 'context, I: Iterator<Item = Event<'a>>> Headings<'a, 'toc, 'context, I> {
    fn new(
        iter: I,
        parent_id: Option<&'context HtmlId>,
        page_context: &'context PageContext<'context>,
        toc: Option<&'toc mut Vec<(u32, String, &'context HtmlId)>>,
    ) -> Self {
        Self {
            inner: iter,
            buf: Default::default(),
            parent_id,
            page_context,
            toc,
        }
    }
}

impl<'a, 'toc, 'vec, I: Iterator<Item = Event<'a>>> Iterator for Headings<'a, 'toc, 'vec, I> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.buf.pop_front() {
            return Some(event);
        }

        let event = self.inner.next();
        let level = if let Some(Event::Start(Tag::Heading(level, ..))) = event {
            level
        } else {
            return event;
        };

        let mut original_text = String::new();
        for event in &mut self.inner {
            match event {
                Event::End(Tag::Heading(..)) => break,
                Event::Start(Tag::Link(_, _, _)) | Event::End(Tag::Link(..)) => {}
                Event::Text(ref s) | Event::Code(ref s) => {
                    original_text.push_str(s);
                    self.buf.push_back(event);
                }
                _ => self.buf.push_back(event),
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

        let mut id = HtmlId::new(id);
        if let Some(parent_id) = self.parent_id {
            id = parent_id + id;
        }

        let level = HeadingLevel::try_from(level as usize + 1)
            .expect("unable to increase the heading level");

        let start_html = format!("<{} class=\"rd-anchor\" id=\"{}\">", level, id);

        let end_html = format!(
            "<a aria-label=\"anchor\" href=\"{}\">\
                               <i class=\"bi bi-hash\"></i></a></{}>",
            id.with_pound(),
            level
        );

        self.buf.push_back(Event::Html(end_html.into()));
        if let Some(ref mut toc) = self.toc {
            let id = self.page_context.ids.alloc(id);
            toc.push((level as u32, original_text, &*id));
        }

        Some(Event::Html(start_html.into()))
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
            } else if trimmed.strip_prefix("# ").is_some() || trimmed == "#" {
                // We cannot handle '#text' because it could be #[attr].
                None
            } else {
                Some(Cow::Borrowed(l))
            }
        });
        let stripped_code = lines.collect::<Vec<Cow<'_, str>>>().join("\n");

        let mut html = String::with_capacity(50 + stripped_code.len());
        html.push_str("<pre><code class=\"language-rust\">");
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
            .map(|token| token.strip_prefix('.').unwrap_or(token))
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
                    if let Some((flag, _help)) =
                        if s == "compile-fail" || s == "compile_fail" || s == "compilefail" {
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
                        }
                    {
                        warn!("unknow attribute `{}`. Did you mean `{}`?", x, flag);
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
