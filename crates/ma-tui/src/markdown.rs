use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::styles;

pub(super) fn render(
    markdown: &str,
    header: Vec<Span<'static>>,
    base: Style,
) -> Vec<Line<'static>> {
    Renderer::new(header, base).render(markdown)
}

struct Renderer {
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    style: Style,
    styles: Vec<Style>,
    lists: Vec<Option<u64>>,
    code_block: bool,
}

impl Renderer {
    fn new(header: Vec<Span<'static>>, base: Style) -> Self {
        Self {
            lines: Vec::new(),
            current: header,
            style: base,
            styles: Vec::new(),
            lists: Vec::new(),
            code_block: false,
        }
    }

    fn render(mut self, markdown: &str) -> Vec<Line<'static>> {
        let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        for event in Parser::new_ext(markdown, options) {
            self.event(event);
        }
        self.flush();
        if self.lines.is_empty() {
            self.lines.push(Line::default());
        }
        self.lines
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) if self.code_block => self.code_text(&text),
            Event::Text(text) => self.push(text.into_string()),
            Event::Code(code) => self.current.push(Span::styled(
                code.into_string(),
                self.style.patch(styles::code()),
            )),
            Event::SoftBreak => self.push(" ".to_owned()),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.lines.push(Line::from(Span::styled(
                    "  ────────────────────",
                    styles::muted(),
                )));
            }
            Event::TaskListMarker(checked) => self.current.push(Span::styled(
                if checked { "[✓] " } else { "[ ] " },
                styles::muted(),
            )),
            Event::Html(html) | Event::InlineHtml(html) => self.push(html.into_string()),
            Event::FootnoteReference(label) => self.push(format!("[{label}]")),
            Event::InlineMath(math) => self.push(math.into_string()),
            Event::DisplayMath(math) => {
                self.flush();
                self.push(format!("  {math}"));
                self.flush();
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { .. } => self.enter(styles::bold_emphasis()),
            Tag::Emphasis => self.enter(Style::default().add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.enter(Style::default().add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => self.enter(Style::default().add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { .. } => self.enter(styles::link()),
            Tag::CodeBlock(_) => {
                self.flush();
                self.code_block = true;
            }
            Tag::List(start) => {
                self.flush();
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                let prefix = match self.lists.last_mut() {
                    Some(Some(number)) => {
                        let prefix = format!("  {number}. ");
                        *number = number.saturating_add(1);
                        prefix
                    }
                    _ => "  • ".to_owned(),
                };
                self.current.push(Span::styled(prefix, styles::muted()));
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.current.push(Span::styled("  │ ", styles::muted()));
            }
            Tag::Image { .. } => self.push("[image: ".to_owned()),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.leave();
                self.flush();
            }
            TagEnd::Paragraph | TagEnd::Item | TagEnd::BlockQuote(_) => self.flush(),
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.leave();
            }
            TagEnd::CodeBlock => {
                self.flush();
                self.code_block = false;
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Image => self.push("]".to_owned()),
            _ => {}
        }
    }

    fn code_text(&mut self, text: &str) {
        for line in text.lines() {
            self.current.push(Span::styled(
                format!("    {line}"),
                self.style.patch(styles::code()),
            ));
            self.flush();
        }
    }

    fn push(&mut self, text: String) {
        self.current.push(Span::styled(text, self.style));
    }

    fn enter(&mut self, style: Style) {
        self.styles.push(self.style);
        self.style = self.style.patch(style);
    }

    fn leave(&mut self) {
        if let Some(style) = self.styles.pop() {
            self.style = style;
        }
    }

    fn flush(&mut self) {
        if !self.current.is_empty() {
            self.lines
                .push(Line::from(std::mem::take(&mut self.current)));
        }
    }
}
