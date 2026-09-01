use ratatui::style::{Color, Modifier, Style};

/// Central palette constants for minimal-agent TUI.
///
/// Designed for a calm, distraction-free monochrome aesthetic where structure
/// is muted grey and color is reserved strictly for semantic meaning (success/failure).
pub mod palette {
    use super::*;

    /// Structural elements: borders, bullet points, dimmed annotations, input prompt arrow.
    pub const MUTED: Color = Color::DarkGray;

    /// Primary emphasis: active titles, selected items, highlighted spinner text.
    pub const EMPHASIS: Color = Color::White;

    /// Inverted foreground used on highlighted backgrounds (e.g. selected menu row).
    pub const INVERTED: Color = Color::Black;

    /// Normal conversation text (resets to terminal default).
    pub const TEXT: Color = Color::Reset;

    /// Successful tool results.
    pub const SUCCESS: Color = Color::Green;

    /// Errors, faults, and failed tool results.
    pub const ERROR: Color = Color::Red;

    /// Inline code and codeblock text.
    pub const CODE: Color = Color::Yellow;

    /// Hyperlinks.
    pub const LINK: Color = Color::Blue;
}

pub mod styles {
    use super::palette::*;
    use super::*;

    pub fn muted() -> Style {
        Style::default().fg(MUTED)
    }

    pub fn dim() -> Style {
        Style::default().add_modifier(Modifier::DIM)
    }

    pub fn emphasis() -> Style {
        Style::default().fg(EMPHASIS)
    }

    pub fn bold_emphasis() -> Style {
        Style::default().fg(EMPHASIS).add_modifier(Modifier::BOLD)
    }

    pub fn border() -> Style {
        Style::default().fg(MUTED)
    }

    pub fn prompt_arrow() -> Style {
        Style::default().fg(MUTED)
    }

    pub fn selected_item() -> Style {
        Style::default()
            .fg(INVERTED)
            .bg(EMPHASIS)
            .add_modifier(Modifier::BOLD)
    }

    pub fn unselected_item() -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    pub fn success() -> Style {
        Style::default().fg(SUCCESS)
    }

    pub fn error() -> Style {
        Style::default().fg(ERROR)
    }

    pub fn code() -> Style {
        Style::default().fg(CODE).bg(MUTED)
    }

    pub fn link() -> Style {
        Style::default().fg(LINK).add_modifier(Modifier::UNDERLINED)
    }
}
