use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::error::JournalError;
use super::events::{JournalEvent, JournalEventKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "storage", rename_all = "snake_case")]
pub enum StoredEvent {
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
pub struct StoredRecord {
    pub sequence: u64,
    pub recorded_at: DateTime<Utc>,
    pub event: StoredEvent,
    pub checksum: String,
}

#[derive(Serialize)]
pub struct ChecksumInput<'a> {
    pub sequence: u64,
    pub recorded_at: &'a DateTime<Utc>,
    pub event: &'a StoredEvent,
}

pub fn record_checksum(
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

pub fn make_record(sequence: u64, event: StoredEvent) -> Result<StoredRecord, JournalError> {
    let recorded_at = Utc::now();
    let checksum = record_checksum(sequence, &recorded_at, &event)?;
    Ok(StoredRecord {
        sequence,
        recorded_at,
        event,
        checksum,
    })
}

pub fn encode_record(record: &StoredRecord) -> Result<Vec<u8>, JournalError> {
    let mut bytes = serde_json::to_vec(record)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
