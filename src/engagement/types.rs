use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::constants::{MAX_ENGAGEMENT_TEXT_BYTES, MAX_OFF_LIMITS};
use super::error::EngagementError;
use super::validation::{check_optional, check_required};

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

    pub fn label(self) -> &'static str {
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

    pub fn validate(&self) -> Result<(), EngagementError> {
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
