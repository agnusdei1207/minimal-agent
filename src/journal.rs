use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{AgentId, AgentMessage, AgentState, CompactionCoverage, SequenceRange};
use crate::engagement::Engagement;

pub const DEFAULT_SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_INLINE_PAYLOAD_BYTES: usize = 16 * 1024;
pub const DEFAULT_MAX_RUN_BYTES: u64 = 512 * 1024 * 1024;
pub const DEFAULT_RESERVED_FAULT_BYTES: u64 = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalConfig {
    pub segment_bytes: u64,
    pub inline_payload_bytes: usize,
    pub max_run_bytes: u64,
    pub reserved_fault_bytes: u64,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            segment_bytes: DEFAULT_SEGMENT_BYTES,
            inline_payload_bytes: DEFAULT_INLINE_PAYLOAD_BYTES,
            max_run_bytes: DEFAULT_MAX_RUN_BYTES,
            reserved_fault_bytes: DEFAULT_RESERVED_FAULT_BYTES,
        }
    }
}

impl JournalConfig {
    fn validate(self) -> Result<Self, JournalError> {
        let valid = self.segment_bytes > 0
            && self.inline_payload_bytes > 0
            && self.reserved_fault_bytes > 0
            && self.max_run_bytes > self.reserved_fault_bytes;
        valid.then_some(self).ok_or(JournalError::InvalidConfig)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalEvent {
    AgentCreated {
        agent_id: AgentId,
        role: String,
        task: String,
    },
    AgentAssigned {
        agent_id: AgentId,
        role: String,
        task: String,
    },
    AgentStateChanged {
        agent_id: AgentId,
        from: AgentState,
        to: AgentState,
        reason: String,
    },
    AgentMessage {
        message: AgentMessage,
    },
    MessageConsumed {
        recipient: AgentId,
        message_ids: Vec<Uuid>,
    },
    Transcript {
        agent_id: AgentId,
        role: TranscriptRole,
        content: String,
        complete: bool,
        atomic_group: Option<String>,
    },
    ToolCall {
        agent_id: AgentId,
        call_id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        agent_id: AgentId,
        call_id: String,
        content: String,
        success: bool,
    },
    BriefCheckpoint {
        agent_id: AgentId,
        markdown: String,
        markdown_sha256: String,
        source_sha256: String,
        source_ranges: Vec<SequenceRange>,
        coverage: CompactionCoverage,
    },
    /// A battlefield note the agent authored directly via the `brief` tool,
    /// decoupled from coverage-proven compaction so a weak model can always keep
    /// its strategy current (INTENT-0001 §9.1). Last write wins.
    BriefNote {
        agent_id: AgentId,
        markdown: String,
    },
    Finding {
        agent_id: AgentId,
        title: String,
        body: String,
    },
    Final {
        agent_id: AgentId,
        body: String,
    },
    Fault {
        agent_id: Option<AgentId>,
        code: String,
        message: String,
    },
    /// Durable authorized-engagement context so a resumed run recovers the same
    /// target/scope/flag doctrine it was created with (INTENT-0002 §4).
    EngagementSet {
        engagement: Engagement,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEventKind {
    AgentCreated,
    AgentAssigned,
    AgentStateChanged,
    AgentMessage,
    MessageConsumed,
    Transcript,
    ToolCall,
    ToolResult,
    BriefCheckpoint,
    BriefNote,
    Finding,
    Final,
    Fault,
    EngagementSet,
}

impl JournalEvent {
    fn kind(&self) -> JournalEventKind {
        match self {
            Self::AgentCreated { .. } => JournalEventKind::AgentCreated,
            Self::AgentAssigned { .. } => JournalEventKind::AgentAssigned,
            Self::AgentStateChanged { .. } => JournalEventKind::AgentStateChanged,
            Self::AgentMessage { .. } => JournalEventKind::AgentMessage,
            Self::MessageConsumed { .. } => JournalEventKind::MessageConsumed,
            Self::Transcript { .. } => JournalEventKind::Transcript,
            Self::ToolCall { .. } => JournalEventKind::ToolCall,
            Self::ToolResult { .. } => JournalEventKind::ToolResult,
            Self::BriefCheckpoint { .. } => JournalEventKind::BriefCheckpoint,
            Self::BriefNote { .. } => JournalEventKind::BriefNote,
            Self::Finding { .. } => JournalEventKind::Finding,
            Self::Final { .. } => JournalEventKind::Final,
            Self::Fault { .. } => JournalEventKind::Fault,
            Self::EngagementSet { .. } => JournalEventKind::EngagementSet,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventStorage {
    Inline,
    Blob { sha256: String, bytes: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalAck {
    pub sequence: u64,
    pub storage: EventStorage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayedEvent {
    pub sequence: u64,
    pub recorded_at: DateTime<Utc>,
    pub event: JournalEvent,
    pub storage: EventStorage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "storage", rename_all = "snake_case")]
enum StoredEvent {
    Inline {
        event: JournalEvent,
    },
    Blob {
        kind: JournalEventKind,
        sha256: String,
        bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredRecord {
    sequence: u64,
    recorded_at: DateTime<Utc>,
    event: StoredEvent,
    checksum: String,
}

#[derive(Serialize)]
struct ChecksumInput<'a> {
    sequence: u64,
    recorded_at: &'a DateTime<Utc>,
    event: &'a StoredEvent,
}

#[derive(Debug)]
struct WriterState {
    next_sequence: u64,
    segment_number: u64,
    segment_file: File,
    segment_len: u64,
    total_bytes: u64,
    stopped: bool,
}

#[derive(Debug)]
pub struct RunJournal {
    root: PathBuf,
    journal_dir: PathBuf,
    blobs_dir: PathBuf,
    config: JournalConfig,
    _writer_lock: File,
    writer: Mutex<WriterState>,
}

impl RunJournal {
    pub fn open(root: impl AsRef<Path>, config: JournalConfig) -> Result<Self, JournalError> {
        let config = config.validate()?;
        let root = root.as_ref().to_path_buf();
        let journal_dir = root.join("journal");
        let blobs_dir = root.join("blobs");
        fs::create_dir_all(&journal_dir)?;
        fs::create_dir_all(&blobs_dir)?;

        let lock_path = root.join("writer.lock");
        let writer_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        writer_lock
            .try_lock_exclusive()
            .map_err(|_| JournalError::WriterActive)?;

        let total_bytes = directory_bytes(&journal_dir)? + directory_bytes(&blobs_dir)?;
        if total_bytes > config.max_run_bytes {
            return Err(JournalError::ExistingStorageTooLarge {
                actual: total_bytes,
                max: config.max_run_bytes,
            });
        }
        let scan = scan_records(&journal_dir, &blobs_dir, true)?;
        let segment_number = scan.last_segment.max(1);
        let segment_path = segment_path(&journal_dir, segment_number);
        let segment_file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&segment_path)?;
        let segment_len = segment_file.metadata()?.len();
        let stopped = total_bytes
            > config
                .max_run_bytes
                .saturating_sub(config.reserved_fault_bytes)
            || scan.events.last().is_some_and(|entry| {
                matches!(
                    &entry.event,
                    JournalEvent::Fault { code, .. } if code == "storage_cap"
                )
            });

        Ok(Self {
            root,
            journal_dir,
            blobs_dir,
            config,
            _writer_lock: writer_lock,
            writer: Mutex::new(WriterState {
                next_sequence: scan.last_sequence.saturating_add(1),
                segment_number,
                segment_file,
                segment_len,
                total_bytes,
                stopped,
            }),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Last committed sequence, without replaying or hydrating any payloads.
    pub fn latest_sequence(&self) -> Result<u64, JournalError> {
        let writer = self.writer.lock().map_err(|_| JournalError::LockPoisoned)?;
        Ok(writer.next_sequence.saturating_sub(1))
    }

    pub async fn append(&self, event: JournalEvent) -> Result<JournalAck, JournalError> {
        tokio::task::yield_now().await;
        self.append_sync(event)
    }

    pub fn append_sync(&self, event: JournalEvent) -> Result<JournalAck, JournalError> {
        self.append_sync_inner(event, false)
    }

    pub fn append_first_sync(&self, event: JournalEvent) -> Result<JournalAck, JournalError> {
        self.append_sync_inner(event, true)
    }

    fn append_sync_inner(
        &self,
        event: JournalEvent,
        require_empty: bool,
    ) -> Result<JournalAck, JournalError> {
        let serialized = serde_json::to_vec(&event)?;
        let mut writer = self.writer.lock().map_err(|_| JournalError::LockPoisoned)?;
        if require_empty && writer.next_sequence != 1 {
            return Err(JournalError::ExpectedEmpty);
        }
        if writer.stopped {
            return Err(JournalError::StorageFull);
        }

        let (stored, storage, pending_blob) = if serialized.len() > self.config.inline_payload_bytes
        {
            let sha256 = digest(&serialized);
            let path = self.blobs_dir.join(&sha256);
            let is_new = !path.exists();
            (
                StoredEvent::Blob {
                    kind: event.kind(),
                    sha256: sha256.clone(),
                    bytes: serialized.len() as u64,
                },
                EventStorage::Blob {
                    sha256,
                    bytes: serialized.len() as u64,
                },
                Some((path, serialized, is_new)),
            )
        } else {
            (StoredEvent::Inline { event }, EventStorage::Inline, None)
        };

        let record = make_record(writer.next_sequence, stored)?;
        let line = encode_record(&record)?;
        let blob_bytes = pending_blob
            .as_ref()
            .filter(|(_, _, is_new)| *is_new)
            .map_or(0, |(_, bytes, _)| bytes.len() as u64);
        let required = line.len() as u64 + blob_bytes;
        let usable_limit = self
            .config
            .max_run_bytes
            .saturating_sub(self.config.reserved_fault_bytes);
        if writer.total_bytes.saturating_add(required) > usable_limit {
            append_storage_fault(&mut writer, &self.journal_dir, self.config)?;
            writer.stopped = true;
            return Err(JournalError::StorageFull);
        }

        if let Some((path, bytes, _)) = &pending_blob {
            write_blob(path, bytes)?;
        }
        if let Err(error) = append_record(&mut writer, &self.journal_dir, self.config, &line) {
            if let Some((path, _, true)) = &pending_blob {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
        writer.total_bytes = writer.total_bytes.saturating_add(required);
        writer.next_sequence = writer.next_sequence.saturating_add(1);
        Ok(JournalAck {
            sequence: record.sequence,
            storage,
        })
    }

    pub fn replay(&self) -> Result<Vec<ReplayedEvent>, JournalError> {
        Ok(scan_records(&self.journal_dir, &self.blobs_dir, false)?.events)
    }

    pub fn replay_range(
        &self,
        start: u64,
        end: u64,
        max_bytes: usize,
    ) -> Result<Vec<ReplayedEvent>, JournalError> {
        if start == 0 || start > end || max_bytes == 0 {
            return Err(JournalError::InvalidReplayRange);
        }
        Ok(scan_records_selected(
            &self.journal_dir,
            &self.blobs_dir,
            false,
            Some(ReplaySelection {
                start,
                end,
                max_bytes,
                kind: None,
            }),
            None,
        )?
        .events)
    }

    /// Visit one kind after a watermark in one scan. Each selected payload is
    /// bounded independently and released after the callback; unrelated blobs
    /// stay on disk. Record order/checksums and selected blob digests are checked.
    pub fn visit_kind_after(
        &self,
        after: u64,
        kind: JournalEventKind,
        max_event_bytes: usize,
        mut visit: impl FnMut(ReplayedEvent),
    ) -> Result<(), JournalError> {
        if max_event_bytes == 0 {
            return Err(JournalError::InvalidReplayRange);
        }
        if after == u64::MAX {
            return Ok(());
        }
        scan_records_selected(
            &self.journal_dir,
            &self.blobs_dir,
            false,
            Some(ReplaySelection {
                start: after + 1,
                end: u64::MAX,
                max_bytes: max_event_bytes,
                kind: Some(kind),
            }),
            Some(&mut visit),
        )?;
        Ok(())
    }
}

fn make_record(sequence: u64, event: StoredEvent) -> Result<StoredRecord, JournalError> {
    let recorded_at = Utc::now();
    let checksum = record_checksum(sequence, &recorded_at, &event)?;
    Ok(StoredRecord {
        sequence,
        recorded_at,
        event,
        checksum,
    })
}

fn encode_record(record: &StoredRecord) -> Result<Vec<u8>, JournalError> {
    let mut bytes = serde_json::to_vec(record)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn record_checksum(
    sequence: u64,
    recorded_at: &DateTime<Utc>,
    event: &StoredEvent,
) -> Result<String, JournalError> {
    Ok(digest(&serde_json::to_vec(&ChecksumInput {
        sequence,
        recorded_at,
        event,
    })?))
}

fn append_record(
    writer: &mut WriterState,
    journal_dir: &Path,
    config: JournalConfig,
    line: &[u8],
) -> Result<(), JournalError> {
    if writer.segment_len > 0
        && writer.segment_len.saturating_add(line.len() as u64) > config.segment_bytes
    {
        writer.segment_number = writer.segment_number.saturating_add(1);
        writer.segment_file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .read(true)
            .open(segment_path(journal_dir, writer.segment_number))?;
        writer.segment_len = 0;
    }
    writer.segment_file.write_all(line)?;
    writer.segment_file.flush()?;
    writer.segment_file.sync_data()?;
    writer.segment_len = writer.segment_len.saturating_add(line.len() as u64);
    Ok(())
}

fn append_storage_fault(
    writer: &mut WriterState,
    journal_dir: &Path,
    config: JournalConfig,
) -> Result<(), JournalError> {
    let record = make_record(
        writer.next_sequence,
        StoredEvent::Inline {
            event: JournalEvent::Fault {
                agent_id: None,
                code: "storage_cap".to_owned(),
                message: "run storage cap reached; journal stopped without deleting source"
                    .to_owned(),
            },
        },
    )?;
    let line = encode_record(&record)?;
    if line.len() as u64 > config.reserved_fault_bytes
        || writer.total_bytes.saturating_add(line.len() as u64) > config.max_run_bytes
    {
        return Err(JournalError::FaultReserveTooSmall);
    }
    append_record(writer, journal_dir, config, &line)?;
    writer.total_bytes = writer.total_bytes.saturating_add(line.len() as u64);
    writer.next_sequence = writer.next_sequence.saturating_add(1);
    Ok(())
}

fn write_blob(path: &Path, bytes: &[u8]) -> Result<(), JournalError> {
    if path.exists() {
        let actual = path.metadata()?.len();
        if actual != bytes.len() as u64 {
            return Err(JournalError::BlobLengthMismatch {
                expected: bytes.len() as u64,
                actual,
            });
        }
        let existing = fs::read(path)?;
        return (digest(&existing) == digest(bytes))
            .then_some(())
            .ok_or_else(|| JournalError::BlobDigestMismatch {
                expected: digest(bytes),
                actual: digest(&existing),
            });
    }
    let temp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) if path.exists() => {
            let _ = fs::remove_file(temp);
            let actual = fs::read(path)?;
            if digest(&actual) == digest(bytes) {
                Ok(())
            } else {
                Err(JournalError::Io(error))
            }
        }
        Err(error) => {
            let _ = fs::remove_file(temp);
            Err(JournalError::Io(error))
        }
    }
}

#[derive(Debug, Default)]
struct ScanResult {
    events: Vec<ReplayedEvent>,
    last_sequence: u64,
    last_segment: u64,
}

fn scan_records(
    journal_dir: &Path,
    blobs_dir: &Path,
    repair_partial_tail: bool,
) -> Result<ScanResult, JournalError> {
    scan_records_selected(journal_dir, blobs_dir, repair_partial_tail, None, None)
}

#[derive(Debug, Clone, Copy)]
struct ReplaySelection {
    start: u64,
    end: u64,
    max_bytes: usize,
    kind: Option<JournalEventKind>,
}

fn scan_records_selected(
    journal_dir: &Path,
    blobs_dir: &Path,
    repair_partial_tail: bool,
    selection: Option<ReplaySelection>,
    mut visit: Option<&mut dyn FnMut(ReplayedEvent)>,
) -> Result<ScanResult, JournalError> {
    let segments = segment_files(journal_dir)?;
    if segments.is_empty() {
        return Ok(ScanResult::default());
    }
    let mut result = ScanResult::default();
    let mut expected_sequence = 1_u64;
    let mut selected_bytes = 0_usize;
    for (segment_index, (segment_number, path)) in segments.iter().enumerate() {
        result.last_segment = *segment_number;
        let is_last_segment = segment_index + 1 == segments.len();
        let mut bytes = fs::read(path)?;
        let complete_len = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |position| position + 1);
        if complete_len < bytes.len() {
            if !(repair_partial_tail && is_last_segment) {
                return Err(JournalError::MiddleCorruption {
                    segment: path.display().to_string(),
                    line: bytes[..complete_len]
                        .iter()
                        .filter(|byte| **byte == b'\n')
                        .count()
                        + 1,
                });
            }
            let file = OpenOptions::new().write(true).open(path)?;
            file.set_len(complete_len as u64)?;
            file.sync_all()?;
            bytes.truncate(complete_len);
        }

        for (line_index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
            if line.is_empty() {
                continue;
            }
            let record: StoredRecord =
                serde_json::from_slice(line).map_err(|_| JournalError::MiddleCorruption {
                    segment: path.display().to_string(),
                    line: line_index + 1,
                })?;
            if record.sequence != expected_sequence {
                return Err(JournalError::SequenceGap {
                    expected: expected_sequence,
                    actual: record.sequence,
                });
            }
            let expected_checksum =
                record_checksum(record.sequence, &record.recorded_at, &record.event)?;
            if record.checksum != expected_checksum {
                return Err(JournalError::ChecksumMismatch {
                    sequence: record.sequence,
                });
            }
            let selected = selection.is_none_or(|range| {
                record.sequence >= range.start
                    && record.sequence <= range.end
                    && range.kind.is_none_or(|kind| match &record.event {
                        StoredEvent::Inline { event } => event.kind() == kind,
                        StoredEvent::Blob {
                            kind: stored_kind, ..
                        } => *stored_kind == kind,
                    })
            });
            if selected {
                if let Some(range) = selection {
                    let event_bytes = match &record.event {
                        StoredEvent::Inline { .. } => line.len(),
                        StoredEvent::Blob { bytes, .. } => {
                            usize::try_from(*bytes).unwrap_or(usize::MAX)
                        }
                    };
                    let retained_bytes = if visit.is_some() { 0 } else { selected_bytes };
                    selected_bytes = retained_bytes
                        .checked_add(event_bytes)
                        .filter(|bytes| *bytes <= range.max_bytes)
                        .ok_or(JournalError::ReplayLimit {
                            max_bytes: range.max_bytes,
                        })?;
                }
                let (event, storage) = resolve_event(record.event, blobs_dir)?;
                let replayed = ReplayedEvent {
                    sequence: record.sequence,
                    recorded_at: record.recorded_at,
                    event,
                    storage,
                };
                if let Some(visit) = visit.as_mut() {
                    visit(replayed);
                } else {
                    result.events.push(replayed);
                }
            }
            result.last_sequence = record.sequence;
            expected_sequence = expected_sequence.saturating_add(1);
        }
    }
    Ok(result)
}

fn resolve_event(
    stored: StoredEvent,
    blobs_dir: &Path,
) -> Result<(JournalEvent, EventStorage), JournalError> {
    match stored {
        StoredEvent::Inline { event } => Ok((event, EventStorage::Inline)),
        StoredEvent::Blob {
            kind: _,
            sha256,
            bytes,
        } => {
            let payload = fs::read(blobs_dir.join(&sha256))?;
            let actual = digest(&payload);
            if actual != sha256 {
                return Err(JournalError::BlobDigestMismatch {
                    expected: sha256,
                    actual,
                });
            }
            if payload.len() as u64 != bytes {
                return Err(JournalError::BlobLengthMismatch {
                    expected: bytes,
                    actual: payload.len() as u64,
                });
            }
            let event = serde_json::from_slice(&payload)?;
            Ok((event, EventStorage::Blob { sha256, bytes }))
        }
    }
}

fn segment_files(journal_dir: &Path) -> Result<Vec<(u64, PathBuf)>, JournalError> {
    let mut segments = Vec::new();
    for entry in fs::read_dir(journal_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| JournalError::InvalidSegment(path.display().to_string()))?;
        let number = stem
            .parse::<u64>()
            .map_err(|_| JournalError::InvalidSegment(path.display().to_string()))?;
        segments.push((number, path));
    }
    segments.sort_by_key(|(number, _)| *number);
    for pair in segments.windows(2) {
        if pair[1].0 != pair[0].0.saturating_add(1) {
            return Err(JournalError::InvalidSegmentSequence);
        }
    }
    Ok(segments)
}

fn segment_path(journal_dir: &Path, number: u64) -> PathBuf {
    journal_dir.join(format!("{number:016}.jsonl"))
}

fn directory_bytes(path: &Path) -> Result<u64, JournalError> {
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(total)
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("another writer already owns this run journal")]
    WriterActive,
    #[error("journal is not empty")]
    ExpectedEmpty,
    #[error("run storage cap reached")]
    StorageFull,
    #[error("existing run storage is {actual} bytes; configured maximum is {max}")]
    ExistingStorageTooLarge { actual: u64, max: u64 },
    #[error("journal configuration is invalid")]
    InvalidConfig,
    #[error("journal writer lock was poisoned")]
    LockPoisoned,
    #[error("reserved fault space is too small")]
    FaultReserveTooSmall,
    #[error("journal is corrupt at {segment}, line {line}")]
    MiddleCorruption { segment: String, line: usize },
    #[error("journal sequence gap: expected {expected}, found {actual}")]
    SequenceGap { expected: u64, actual: u64 },
    #[error("journal record {sequence} checksum mismatch")]
    ChecksumMismatch { sequence: u64 },
    #[error("blob digest mismatch: expected {expected}, found {actual}")]
    BlobDigestMismatch { expected: String, actual: String },
    #[error("blob length mismatch: expected {expected}, found {actual}")]
    BlobLengthMismatch { expected: u64, actual: u64 },
    #[error("invalid journal segment: {0}")]
    InvalidSegment(String),
    #[error("journal segments are not contiguous")]
    InvalidSegmentSequence,
    #[error("journal replay range is invalid")]
    InvalidReplayRange,
    #[error("journal replay exceeds the {max_bytes}-byte response limit")]
    ReplayLimit { max_bytes: usize },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
