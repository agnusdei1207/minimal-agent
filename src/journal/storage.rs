use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fs2::FileExt;

use super::blob::write_blob;
use super::config::JournalConfig;
use super::constants::*;
use super::error::JournalError;
use super::events::{EventStorage, JournalAck, JournalEvent, JournalEventKind, ReplayedEvent};
use super::record::{
    StoredEvent, StoredRecord, digest, encode_record, make_record, record_checksum,
};

#[derive(Debug)]
pub struct WriterState {
    pub next_sequence: u64,
    pub segment_number: u64,
    pub segment_file: File,
    pub segment_len: u64,
    pub total_bytes: u64,
    pub stopped: bool,
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
        let journal_dir = root.join(JOURNAL_DIR_NAME);
        let blobs_dir = root.join(BLOBS_DIR_NAME);
        fs::create_dir_all(&journal_dir)?;
        fs::create_dir_all(&blobs_dir)?;

        let lock_path = root.join(WRITER_LOCK_NAME);
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
                    JournalEvent::Fault { code, .. } if code == FAULT_CODE_STORAGE_CAP
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
                code: FAULT_CODE_STORAGE_CAP.to_owned(),
                message: FAULT_MSG_STORAGE_CAP.to_owned(),
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
        if path.extension().and_then(|value| value.to_str()) != Some(SEGMENT_EXTENSION) {
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
    journal_dir.join(format!("{number:016}.{SEGMENT_EXTENSION}"))
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
