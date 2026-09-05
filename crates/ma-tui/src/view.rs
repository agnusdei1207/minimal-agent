use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::markdown;
use super::model_setup::ModelSetup;
use super::theme::{palette, styles};
use super::{LineTone, Modal, TuiState};

pub(super) fn input_projection(state: &TuiState) -> String {
    match &state.model_setup {
        Some(ModelSetup::ApiKey { .. }) => "*".repeat(state.input.chars().count()),
        _ => state.input.clone(),
    }
}

fn input_cursor_projection(state: &TuiState) -> String {
    let before_cursor = &state.input[..state.input_cursor];
    match &state.model_setup {
        Some(ModelSetup::ApiKey { .. }) => "*".repeat(before_cursor.chars().count()),
        _ => before_cursor.to_owned(),
    }
}

fn input_hint(state: &TuiState) -> &'static str {
    match state.model_setup {
        Some(ModelSetup::ApiKey { .. }) => "API key - hidden; type, then Enter",
        Some(ModelSetup::CustomUrl { .. }) => "Base URL - type, then Enter",
        Some(ModelSetup::Model { .. }) => "Model name - type, then Enter",
        Some(ModelSetup::ContextTokens { .. }) => {
            "Context tokens - type a positive number (e.g. 128k, 1m)"
        }
        None => "",
    }
}

/// Upper bound on how tall the input grows before it stops expanding, so a long
/// paste never swallows the transcript. Beyond this the input wraps internally.
const MAX_INPUT_ROWS: u16 = 8;

/// How many rows the input occupies. It grows with the typed text (wrapped at
/// the terminal width) so long input stays visible instead of clipping to one
/// row. Char-packed estimate; good enough to keep the caret and text on screen.
fn input_rows(state: &TuiState, width: u16) -> u16 {
    let usable = usize::from(width.max(1));
    // "❯ " is 2 columns; the rest is the (masked) input display width.
    let total = 2 + Line::raw(input_projection(state)).width();
    let rows = total.max(1).div_ceil(usable);
    u16::try_from(rows)
        .unwrap_or(MAX_INPUT_ROWS)
        .clamp(1, MAX_INPUT_ROWS)
}

pub fn render(frame: &mut Frame<'_>, state: &mut TuiState) {
    // 3-row layout: transcript (fills), status, input (grows with its content).
    let rows = input_rows(state, frame.area().width);
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(rows),
        ])
        .split(frame.area());

    render_transcript(frame, state, areas[0]);
    frame.render_widget(Paragraph::new(status_line(state)), areas[1]);
    let input_area = areas[2];
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("❯ ", styles::prompt_arrow()),
            Span::raw(input_projection(state)),
        ]))
        .wrap(Wrap { trim: false }),
        input_area,
    );
    let menu = crate::command::command_menu_matches(&state.input);
    if state.model_setup.is_none() && state.modal.is_none() && !menu.is_empty() {
        render_command_menu(frame, input_area, &menu, state.command_selection);
    }
    if let Some(modal) = &state.modal {
        render_modal(frame, modal);
        return;
    }
    // Caret tracks the wrapped position: prompt + text-before-caret, folded at
    // the input width into (row, col).
    let width = input_area.width.max(1);
    let before = 2u16.saturating_add(
        u16::try_from(Line::raw(input_cursor_projection(state)).width()).unwrap_or(u16::MAX),
    );
    let row = (before / width).min(input_area.height.saturating_sub(1));
    let col = before % width;
    frame.set_cursor_position((
        input_area.x.saturating_add(col),
        input_area.y.saturating_add(row),
    ));
}

/// Centered overlay box (80% × 75% of the screen) for a scrollable modal.
fn modal_area(area: Rect) -> Rect {
    let width = (area.width * 8 / 10).max(1);
    let height = (area.height * 3 / 4).max(3);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

fn render_command_menu(
    frame: &mut Frame<'_>,
    input_area: Rect,
    matches: &[(&'static str, &'static str)],
    selection: usize,
) {
    const VISIBLE: usize = 8;
    let selected = selection.min(matches.len().saturating_sub(1));
    let start = if selected < VISIBLE {
        0
    } else {
        selected + 1 - VISIBLE
    };
    let rows = matches.len().saturating_sub(start).min(VISIBLE);
    let height = u16::try_from(rows).unwrap_or(VISIBLE as u16) + 2;
    let width = input_area.width.clamp(24, 56);
    let area = Rect {
        x: input_area.x,
        y: input_area.y.saturating_sub(height),
        width,
        height,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(styles::border())
        .title(" commands · ↑↓ select · Tab complete · Enter run ");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let lines = matches
        .iter()
        .enumerate()
        .skip(start)
        .take(rows)
        .map(|(index, (name, desc))| {
            if index == selected {
                Line::from(vec![
                    Span::styled(format!(" {name} "), styles::selected_item()),
                    Span::styled(format!("  {desc}"), styles::emphasis()),
                ])
            } else {
                Line::from(vec![
                    Span::styled(format!(" {name} "), styles::unselected_item()),
                    Span::styled(format!("  {desc}"), styles::dim()),
                ])
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_modal(frame: &mut Frame<'_>, modal: &Modal) {
    let area = modal_area(frame.area());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(styles::border())
        .title(format!(" {} ", modal.title))
        .title_bottom(" ↑↓/PgUp/PgDn scroll · Esc/x close ");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let text = modal.lines.join("\n");
    let scroll = u16::try_from(modal.scroll).unwrap_or(u16::MAX);
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        inner,
    );
}

fn render_transcript(frame: &mut Frame<'_>, state: &mut TuiState, area: ratatui::layout::Rect) {
    let visible_lines = usize::from(area.height).max(1);
    let transcript = Paragraph::new(Text::from(display_lines(state))).wrap(Wrap { trim: false });
    let rendered_lines = transcript.line_count(area.width);
    let maximum_scroll = rendered_lines.saturating_sub(visible_lines);
    // Top-anchored: a scrolled-up view keeps its offset from the top regardless
    // of streaming or growth at the bottom. Following pins to the newest line.
    let offset = if state.scroll_follow {
        maximum_scroll
    } else {
        state.scroll_offset.min(maximum_scroll)
    };
    state.scroll_max = maximum_scroll;
    state.scroll_offset = offset;
    let offset = u16::try_from(offset).unwrap_or(u16::MAX);
    frame.render_widget(transcript.scroll((offset, 0)), area);
}

fn spinner(frame: usize) -> &'static str {
    // A calm rotating circle rather than a braille wheel or a blinking star.
    const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
    FRAMES[frame % FRAMES.len()]
}

fn queued_projection(state: &TuiState) -> String {
    let active_main_submission = usize::from(
        state
            .active_turns
            .contains_key(&ma_core::domain::AgentId::main())
            && state.pending_submissions > 0,
    );
    match state
        .pending_submissions
        .saturating_sub(active_main_submission)
    {
        0 => String::new(),
        1 => " · 1 queued".to_owned(),
        n => format!(" · {n} queued"),
    }
}

/// The status line: while agents are working, the working segment shimmers (a
/// bright band sweeps across it) so the running state reads as alive.
/// Human-readable elapsed time: `9s`, `1m17s` — so a long wait does not read as a
/// single large second count.
fn fmt_elapsed(secs: u64) -> String {
    if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

/// Compact human-readable token count: `123`, `12.3k`, `1.2M`.
fn fmt_tokens(n: u64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

fn status_line(state: &TuiState) -> Line<'static> {
    if state.model_setup.is_some() {
        let hint = input_hint(state);
        return Line::from(format!("{hint} · Ctrl+C/Esc cancel"));
    }
    let mut spans = Vec::new();
    if state.active_turns.is_empty() {
        spans.push(Span::raw(state.status.clone()));
    } else {
        let elapsed = state
            .active_turns
            .values()
            .map(std::time::Instant::elapsed)
            .max()
            .unwrap_or_default()
            .as_secs();
        // Spinner on the left, then a shimmering "working", then the counter.
        // The team roster (who is active) is shown in the bar above, so this stays
        // a plain busy indicator without agent names.
        spans.push(Span::styled(
            format!("{} ", spinner(state.spinner_frame)),
            Style::default().fg(Color::White),
        ));
        // Show what main is actually doing (thinking / running <tool> / responding).
        // When only workers are busy the label is empty, so fall back to a plain
        // "working" — the spinner still turns while any agent has a live turn.
        let label = if state.activity.is_empty() {
            "working"
        } else {
            state.activity.as_str()
        };
        spans.extend(shimmer_spans(label, state.spinner_frame));
        let steps = if state.turn_steps > 0 {
            format!(" · step {}", state.turn_steps)
        } else {
            String::new()
        };
        spans.push(Span::styled(
            format!("{steps} · {}", fmt_elapsed(elapsed)),
            Style::default().add_modifier(Modifier::DIM),
        ));
    }
    let workers = state
        .team
        .iter()
        .filter(|agent| !agent.id.is_main())
        .count();
    let worker_count = match workers {
        0 => String::new(),
        1 => " · 1 worker".to_owned(),
        n => format!(" · {n} workers"),
    };
    let tokens = if state.input_tokens + state.output_tokens > 0 {
        format!(
            " · tok {}/{}",
            fmt_tokens(state.input_tokens),
            fmt_tokens(state.output_tokens),
        )
    } else {
        String::new()
    };
    let tail = format!(
        "{worker_count}{} · auto:{}{tokens} · Esc interrupt · Ctrl+C clear",
        queued_projection(state),
        if state.auto { "on" } else { "off" },
    );
    spans.push(Span::styled(
        tail,
        Style::default().add_modifier(Modifier::DIM),
    ));
    Line::from(spans)
}

/// Style each character by its distance from a band centre that sweeps left to
/// right with `phase`, producing a moving shimmer highlight.
fn shimmer_spans(text: &str, phase: usize) -> Vec<Span<'static>> {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return Vec::new();
    }
    let period = chars.len() + 8;
    let center = phase % period;
    chars
        .into_iter()
        .enumerate()
        .map(|(index, character)| {
            let distance = center.abs_diff(index);
            // A gentle monochrome white sweep: the band is bright, the rest dim.
            let style = match distance {
                0 | 1 => styles::emphasis(),
                2 => Style::default(),
                _ => styles::dim(),
            };
            Span::styled(character.to_string(), style)
        })
        .collect()
}

struct Entry {
    speaker: String,
    text: String,
    tone: LineTone,
    subtitle: Option<String>,
}

pub(super) fn display_lines(state: &TuiState) -> Vec<Line<'static>> {
    let mut entries = state
        .transcript
        .iter()
        .map(|line| Entry {
            speaker: line.speaker.clone(),
            text: line.text.clone(),
            tone: line.tone,
            subtitle: line.subtitle.clone(),
        })
        .collect::<Vec<_>>();

    let mut partial = state.partial.iter().collect::<Vec<_>>();
    partial.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));
    entries.extend(partial.into_iter().map(|(_, text)| Entry {
        speaker: String::new(),
        text: text.clone(),
        tone: LineTone::Agent,
        subtitle: None,
    }));

    // Separate entries with a blank line for breathing room, but keep a tool
    // result grouped with the call it belongs to (no gap before a result).
    let mut lines = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        let is_result = matches!(entry.tone, LineTone::ToolResult { .. });
        if index > 0 && !is_result {
            lines.push(Line::default());
        }
        lines.extend(render_entry(entry));
    }
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines
}

/// Calm palette: structure is muted grey; colour is reserved for meaning —
/// green for a successful result, red for a failure or fault.
fn tone_style(tone: LineTone) -> (&'static str, Color) {
    match tone {
        LineTone::Agent => ("", palette::TEXT),
        LineTone::Tool => ("•", palette::MUTED),
        LineTone::ToolResult { success: true } => ("↳", palette::SUCCESS),
        LineTone::ToolResult { success: false } => ("↳", palette::ERROR),
        LineTone::Message => ("↳", palette::MUTED),
        LineTone::Fault => ("!", palette::ERROR),
    }
}

fn render_entry(entry: Entry) -> Vec<Line<'static>> {
    let Entry {
        speaker,
        text,
        tone,
        subtitle,
    } = entry;
    if speaker == "help" {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled("• ", styles::muted()),
            Span::styled("commands", styles::bold_emphasis()),
        ]));
        for line in text.lines() {
            if let Some((cmd, desc)) = line.split_once("  ") {
                lines.push(Line::from(vec![
                    Span::styled(format!("    {cmd:<18}"), styles::bold_emphasis()),
                    Span::styled(desc.trim_start().to_owned(), styles::muted()),
                ]));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("    {line}"),
                    styles::muted(),
                )));
            }
        }
        return lines;
    }
    let (glyph, color) = tone_style(tone);

    // Labels and structure recede (dim); the content itself stays at full weight
    // so the eye follows the conversation, not the annotations.
    let mut header = Vec::new();
    if !glyph.is_empty() {
        header.push(Span::styled(
            format!("{glyph} "),
            Style::default().fg(color),
        ));
    }
    if !speaker.is_empty() {
        header.push(Span::styled(
            speaker.clone(),
            Style::default().fg(color).add_modifier(Modifier::DIM),
        ));
    }
    if let Some(subtitle) = subtitle {
        header.push(Span::styled(format!("  {subtitle}"), styles::dim()));
    }
    if !text.is_empty() && !speaker.is_empty() {
        header.push(Span::raw("  "));
    }
    markdown::render(&text, header, Style::default())
}
