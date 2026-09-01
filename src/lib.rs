//! Umbrella facade re-exporting the workspace crates as the historical module
//! layout (`minimal_agent::domain`, `::runtime`, ...), so the binary and the
//! integration tests address one crate while the code lives in focused crates.
pub use ma_context::{brief, compaction};
pub use ma_coordinator as coordinator;
pub use ma_core::{domain, engagement};
pub use ma_journal as journal;
pub use ma_provider::{provider, settings};
pub use ma_runtime::{runtime, tools};
pub use ma_tui as tui;
