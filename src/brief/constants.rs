pub const RUNTIME_START: &str = "<!-- minimal-agent:runtime:start -->";
pub const RUNTIME_END: &str = "<!-- minimal-agent:runtime:end -->";

pub const MAX_PROJECTION_BYTES_LIMIT: usize = 4 * 1024 * 1024;
pub const BACKUP_FILE_NAME: &str = ".brief.backup";
pub const AGENTS_DIR_NAME: &str = "agents";
pub const BRIEF_FILE_NAME: &str = "brief.md";
pub const BATTLEFIELD_FILE_NAME: &str = "battlefield.md";

pub const MAIN_REQUIRED_HEADINGS: &[&str] = &[
    "# Main Agent Brief",
    "## Goal & Constraints",
    "## Battlefield",
    "## Curated Knowledge",
    "### Facts & Successes",
    "### Hypotheses & Directions",
    "### Dead Ends",
    "## Blockers",
    "## Next Moves",
];

pub const WORKER_REQUIRED_HEADINGS: &[&str] = &[
    "# Worker Agent Brief",
    "## Assignment",
    "## Current State",
    "## Attempts by Domain",
    "## Curated Knowledge",
    "### Facts & Successes",
    "### Hypotheses & Directions",
    "### Dead Ends",
    "## Integrated Messages",
    "## Blockers",
    "## Next Move",
];
