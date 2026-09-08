use crate::domain::{CompactionCoverage, SequenceRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefDraft {
    pub markdown: String,
    pub source_sha256: String,
    pub source_ranges: Vec<SequenceRange>,
    pub coverage: CompactionCoverage,
}
