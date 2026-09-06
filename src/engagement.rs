//! Authorized-engagement context injected from outside the process — a JSON
//! file and/or CLI flags — and rendered into every agent's system prompt. See
//! ADR-0002. All doctrine text here is authored clean-room for this repository.

use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Each engagement text field shares the goal byte envelope so an injected
/// engagement cannot inflate a prompt past the same bound goals already respect.
pub const MAX_ENGAGEMENT_TEXT_BYTES: usize = 16 * 1_024;
/// Bounded off-limits list; each entry also respects the text envelope.
pub const MAX_OFF_LIMITS: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum EngagementError {
    #[error("unknown engagement kind '{0}'; expected ctf, pentest, or lab")]
    UnknownKind(String),
    #[error("engagement {field} exceeds {max} bytes")]
    TextTooLong { field: &'static str, max: usize },
    #[error("engagement off-limits list exceeds {MAX_OFF_LIMITS} entries")]
    TooManyOffLimits,
    #[error("rendered engagement context exceeds {MAX_ENGAGEMENT_TEXT_BYTES} bytes")]
    ContextTooLong,
    #[error("engagement {field} cannot be empty when set")]
    EmptyField { field: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EngagementKind {
    Ctf,
    #[default]
    Pentest,
    Lab,
}

impl EngagementKind {
    pub fn parse(value: &str) -> Result<Self, EngagementError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ctf" => Ok(Self::Ctf),
            "pentest" | "pen-test" | "penetration" => Ok(Self::Pentest),
            "lab" => Ok(Self::Lab),
            other => Err(EngagementError::UnknownKind(other.to_owned())),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Ctf => "ctf",
            Self::Pentest => "pentest",
            Self::Lab => "lab",
        }
    }
}

/// Structured, externally injected engagement context. Every field except
/// `kind` is optional so a caller can inject only what a given target needs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Engagement {
    #[serde(default)]
    pub kind: EngagementKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub off_limits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
}

impl Engagement {
    /// Parse an engagement JSON document (all fields optional but validated).
    pub fn from_json(document: &str) -> anyhow::Result<Self> {
        let engagement: Self = serde_json::from_str(document)?;
        engagement.validate()?;
        Ok(engagement)
    }

    /// Load an engagement JSON file.
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let document = std::fs::read_to_string(path)?;
        Self::from_json(&document)
    }

    /// Overlay explicit flag values on top of a base (flags win over the file).
    /// A `None` override leaves the base field untouched; a `Some` off-limits
    /// list replaces the base list rather than appending to it.
    pub fn overlay(
        mut self,
        kind: Option<EngagementKind>,
        scope: Option<String>,
        off_limits: Option<Vec<String>>,
        flag_format: Option<String>,
        objective: Option<String>,
        title: Option<String>,
    ) -> Result<Self, EngagementError> {
        if let Some(kind) = kind {
            self.kind = kind;
        }
        if scope.is_some() {
            self.scope = scope;
        }
        if let Some(off_limits) = off_limits {
            self.off_limits = off_limits;
        }
        if flag_format.is_some() {
            self.flag_format = flag_format;
        }
        if objective.is_some() {
            self.objective = objective;
        }
        if title.is_some() {
            self.title = title;
        }
        self.validate()?;
        Ok(self)
    }

    /// Set or clear the target/scope (validated). Used for live `/target` changes.
    pub fn set_scope(mut self, scope: Option<String>) -> Result<Self, EngagementError> {
        self.scope = scope;
        self.validate()?;
        Ok(self)
    }

    fn validate(&self) -> Result<(), EngagementError> {
        check_optional("title", self.title.as_deref())?;
        check_optional("scope", self.scope.as_deref())?;
        check_optional("flag_format", self.flag_format.as_deref())?;
        check_optional("objective", self.objective.as_deref())?;
        if self.off_limits.len() > MAX_OFF_LIMITS {
            return Err(EngagementError::TooManyOffLimits);
        }
        for entry in &self.off_limits {
            check_required("off-limits entry", entry)?;
        }
        if self.render_context().len() > MAX_ENGAGEMENT_TEXT_BYTES {
            return Err(EngagementError::ContextTooLong);
        }
        Ok(())
    }

    /// Bounded target-context block appended to the system prompt. Never renders
    /// secrets — only the operator-provided scope and format metadata.
    pub fn render_context(&self) -> String {
        let mut block = String::from(
            "AUTHORIZED ENGAGEMENT (standing authorization; operate strictly within this scope)\n",
        );
        block.push_str(&format!("kind: {}\n", self.kind.label()));
        if let Some(title) = &self.title {
            block.push_str(&format!("title: {title}\n"));
        }
        if let Some(scope) = &self.scope {
            block.push_str(&format!("scope: {scope}\n"));
        }
        if !self.off_limits.is_empty() {
            block.push_str(&format!("off-limits: {}\n", self.off_limits.join(", ")));
        }
        if let Some(flag_format) = &self.flag_format {
            block.push_str(&format!("flag_format: {flag_format}\n"));
        }
        if let Some(objective) = &self.objective {
            block.push_str(&format!("objective: {objective}\n"));
        }
        block
    }

    /// Extract the first flag matching `flag_format` from real target output.
    /// The format is treated as a regex; if it does not compile it falls back to
    /// a literal substring search. Returns `None` when no format is configured
    /// or nothing matches — the runtime never fabricates a flag.
    pub fn extract_flag(&self, text: &str) -> Option<String> {
        let format = self.flag_format.as_deref()?;
        match Regex::new(format) {
            Ok(pattern) => pattern
                .find(text)
                .map(|matched| matched.as_str().to_owned()),
            Err(_) => text.contains(format).then(|| format.to_owned()),
        }
    }
}

fn check_optional(field: &'static str, value: Option<&str>) -> Result<(), EngagementError> {
    match value {
        Some(value) => check_required(field, value),
        None => Ok(()),
    }
}

fn check_required(field: &'static str, value: &str) -> Result<(), EngagementError> {
    if value.trim().is_empty() {
        return Err(EngagementError::EmptyField { field });
    }
    if value.len() > MAX_ENGAGEMENT_TEXT_BYTES {
        return Err(EngagementError::TextTooLong {
            field,
            max: MAX_ENGAGEMENT_TEXT_BYTES,
        });
    }
    Ok(())
}

/// Standing authorization doctrine appended to every system prompt (ADR-0002
/// §3.1). Clean-room text: keeps the model from hedging or refusing authorized
/// offensive-security work while leaving only operational boundaries in place.
pub fn authorized_engagement_doctrine() -> &'static str {
    include_str!("../prompts/authorized-engagement.md").trim_end()
}

/// Always-on execution-style directive that keeps the agent terse and
/// action-first instead of narrating, hedging, or restating the task. Clean-room
/// text authored for this repository.
pub fn execution_style_directive() -> &'static str {
    include_str!("../prompts/operating-style.md").trim_end()
}

/// CTF solve-loop doctrine, appended only for `EngagementKind::Ctf` (ADR-0002
/// §3.1). Clean-room text.
pub fn ctf_solve_loop_doctrine() -> &'static str {
    include_str!("../prompts/ctf-solve-loop.md").trim_end()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kind_case_insensitively() {
        assert_eq!(EngagementKind::parse("CTF").unwrap(), EngagementKind::Ctf);
        assert_eq!(
            EngagementKind::parse(" pentest ").unwrap(),
            EngagementKind::Pentest
        );
        assert!(EngagementKind::parse("nonsense").is_err());
    }

    #[test]
    fn flags_override_file_values() {
        let base = Engagement::from_json(r#"{"kind":"lab","scope":"10.0.0.0/24"}"#).unwrap();
        let merged = base
            .overlay(
                Some(EngagementKind::Ctf),
                Some("10.10.10.5".to_owned()),
                None,
                Some(r"flag\{[^}]+\}".to_owned()),
                None,
                None,
            )
            .unwrap();
        assert_eq!(merged.kind, EngagementKind::Ctf);
        assert_eq!(merged.scope.as_deref(), Some("10.10.10.5"));
        assert_eq!(merged.flag_format.as_deref(), Some(r"flag\{[^}]+\}"));
    }

    #[test]
    fn context_block_never_renders_when_field_absent() {
        let engagement = Engagement {
            kind: EngagementKind::Ctf,
            ..Engagement::default()
        };
        let block = engagement.render_context();
        assert!(block.contains("kind: ctf"));
        assert!(!block.contains("scope:"));
    }

    #[test]
    fn extract_flag_matches_regex_from_output() {
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Engagement::default()
        };
        let output = "nmap done\nleaked: flag{r34l_flag} on the box\n";
        assert_eq!(
            engagement.extract_flag(output).as_deref(),
            Some("flag{r34l_flag}")
        );
        assert_eq!(engagement.extract_flag("no flag here"), None);
    }

    #[test]
    fn extract_flag_none_without_format() {
        assert_eq!(Engagement::default().extract_flag("flag{x}"), None);
    }

    #[test]
    fn rejects_oversized_field() {
        let big = "a".repeat(MAX_ENGAGEMENT_TEXT_BYTES + 1);
        let engagement = Engagement {
            scope: Some(big),
            ..Engagement::default()
        };
        assert!(engagement.validate().is_err());
    }

    #[test]
    fn rejects_aggregate_prompt_inflation_from_individually_valid_fields() {
        let engagement = Engagement {
            off_limits: (0..MAX_OFF_LIMITS)
                .map(|index| format!("host-{index}-{}", "x".repeat(512)))
                .collect(),
            ..Engagement::default()
        };

        assert!(engagement.validate().is_err());
    }

    #[test]
    fn ctf_solve_loop_and_doctrines_are_embedded_and_consistent() {
        assert!(!authorized_engagement_doctrine().is_empty());
        assert!(!execution_style_directive().is_empty());
        let ctf = ctf_solve_loop_doctrine();
        assert!(!ctf.is_empty());
        assert!(ctf.contains("Diagnostic Tracer Bullets"));
        assert!(ctf.contains("Silent Wall vs. Live Seam"));
        assert!(ctf.contains("Self-Reflection & Meta-Cognitive Audit"));
        assert!(ctf.contains("Universal Client Compatibility"));
    }
}
