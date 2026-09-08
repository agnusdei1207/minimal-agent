pub const COMPACTION_SYSTEM_PROMPT: &str = r#"You curate one agent's current brief.
Summarize meaning, never discard a supplied source range, and return JSON only.
Group repetitive attempts by domain or technique. Preserve exact FACT and SUCCESS values with
journal references. Keep HYPOTHESIS and DIRECTION unverified. Record why DEAD_END paths failed.
Do not invent facts, credentials, completion, or progress. Do not copy raw payload lists.
Return a complete replacement brief, not a fragment. A main brief must retain exactly one
minimal-agent:runtime start/end block from current_brief and these headings: Main Agent Brief,
Goal & Constraints, Battlefield, Curated Knowledge, Facts & Successes, Hypotheses & Directions,
Dead Ends, Blockers, Next Moves. A worker brief must retain: Worker Agent Brief, Assignment,
Current State, Attempts by Domain, Curated Knowledge, Facts & Successes, Hypotheses & Directions,
Dead Ends, Integrated Messages, Blockers, Next Move.
Return markdown, source_sha256, covered_ranges, covered_insight_ids, and
superseded_insight_ids. Tools are unavailable during compaction."#;

pub const DEFAULT_PARTITION_TOKENS: u64 = 24_000;
pub const DEFAULT_RETRY_PARTITION_TOKENS: u64 = 12_000;
pub const DEFAULT_OVERLAP_TOKENS: u64 = 128;
