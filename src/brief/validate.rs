use crate::domain::{AgentId, InsightLabel, SequenceRange};

use super::constants::{
    MAIN_REQUIRED_HEADINGS, RUNTIME_END, RUNTIME_START, WORKER_REQUIRED_HEADINGS,
};
use super::error::BriefError;

pub fn validate_structure(agent: &AgentId, markdown: &str) -> Result<(), BriefError> {
    let required = if agent.is_main() {
        MAIN_REQUIRED_HEADINGS
    } else {
        WORKER_REQUIRED_HEADINGS
    };
    if markdown.trim().is_empty() || required.iter().any(|heading| !markdown.contains(heading)) {
        return Err(BriefError::InvalidStructure);
    }
    if agent.is_main() {
        validate_runtime_block(markdown)?;
    }
    Ok(())
}

pub fn validate_runtime_block(markdown: &str) -> Result<(), BriefError> {
    if markdown.matches(RUNTIME_START).count() != 1 || markdown.matches(RUNTIME_END).count() != 1 {
        return Err(BriefError::MissingRuntimeBlock);
    }
    let start = markdown
        .find(RUNTIME_START)
        .ok_or(BriefError::MissingRuntimeBlock)?;
    let end = markdown
        .find(RUNTIME_END)
        .ok_or(BriefError::MissingRuntimeBlock)?;
    if start >= end {
        return Err(BriefError::MissingRuntimeBlock);
    }
    Ok(())
}

pub fn validate_knowledge(markdown: &str) -> Result<(), BriefError> {
    let mut section = "";
    for line in markdown.lines() {
        if line.starts_with("### ") {
            section = line;
            continue;
        }
        let trimmed = line.trim_start();
        if !trimmed.starts_with("- [") {
            continue;
        }
        let label_end = trimmed
            .find(']')
            .ok_or_else(|| BriefError::InvalidKnowledge(line.to_owned()))?;
        let label = &trimmed[3..label_end];
        // Only the canonical SCREAMING_SNAKE_CASE token is accepted (parse is
        // case-insensitive, so re-check the exact form).
        let Some(kind) = InsightLabel::parse(label).filter(|kind| kind.as_str() == label) else {
            return Err(BriefError::InvalidKnowledge(line.to_owned()));
        };
        let misfiled = (section.contains("Hypotheses")
            && matches!(kind, InsightLabel::Fact | InsightLabel::Success))
            || (section.contains("Facts")
                && matches!(kind, InsightLabel::Hypothesis | InsightLabel::Direction));
        let missing_provenance = matches!(kind, InsightLabel::Fact | InsightLabel::Success)
            && !trimmed.contains("journal:");
        let missing_reason = kind == InsightLabel::DeadEnd && !trimmed.contains("reason:");
        if misfiled || missing_provenance || missing_reason {
            return Err(BriefError::InvalidKnowledge(line.to_owned()));
        }
    }
    Ok(())
}

pub fn validate_source_metadata(
    source_sha256: &str,
    source_ranges: &[SequenceRange],
) -> Result<(), BriefError> {
    let digest_valid =
        source_sha256.len() == 64 && source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !digest_valid || source_ranges.is_empty() {
        return Err(BriefError::InvalidSourceMetadata);
    }
    Ok(())
}
