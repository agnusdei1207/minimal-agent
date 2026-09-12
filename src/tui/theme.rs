use ratatui::style::{Color, Modifier, Style};

/// Central palette constants for pentesting TUI.
///
/// Designed for a calm, distraction-free monochrome aesthetic where structure
/// is muted grey and color is reserved strictly for semantic meaning
/// (success/failure), plus one rare accent reserved for special moments only:
/// the start banner program name and the user's prompt arrow (`❯`).
/// Nothing else may use the accent.
pub mod palette {
    use super::*;

    /// Structural elements: borders, bullet points, dimmed annotations.
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

    /// Rare accent for special moments only: the start banner program name and
    /// the user's prompt arrow (`❯` in input and transcript echo).
    /// Opaque `#C8FF00`: the reference image shows `#C8FF00E0`, but terminals
    /// have no alpha channel, so the E0 alpha is not representable.
    pub const ACCENT: Color = Color::Rgb(0xC8, 0xFF, 0x00);
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

    pub fn border() -> Style {
        Style::default().fg(MUTED)
    }

    /// Rare accent style: start banner program name + user input only.
    /// Do NOT use for status, spinners, menus, modals, tool results, or faults.
    pub fn accent() -> Style {
        Style::default().fg(ACCENT)
    }

    pub fn selected_item() -> Style {
        Style::default().fg(INVERTED).bg(EMPHASIS)
    }

    pub fn unselected_item() -> Style {
        Style::default()
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
