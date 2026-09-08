use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::domain::{InsightId, SequenceRange, estimate_tokens};

use super::error::CompactionError;
use super::types::{ContextEntry, Partition, SourceUnit};

pub fn make_partitions(
    entries: &[ContextEntry],
    partition_tokens: u64,
    overlap_tokens: u64,
) -> Result<Vec<Partition>, String> {
    if partition_tokens == 0 || overlap_tokens >= partition_tokens {
        return Err("invalid partition bounds".to_owned());
    }
    let units = make_units(entries);
    let mut pieces = Vec::new();
    for unit in units {
        pieces.extend(split_unit(unit, partition_tokens, overlap_tokens)?);
    }
    if pieces.is_empty() {
        return Err("no source partition was produced".to_owned());
    }

    let mut partitions: Vec<Partition> = Vec::new();
    for piece in pieces {
        if let Some(last) = partitions.last_mut()
            && estimate_tokens(&last.source)
                .saturating_add(estimate_tokens(&piece.source))
                .saturating_add(1)
                <= partition_tokens
        {
            last.source.push_str("\n\n");
            last.source.push_str(&piece.source);
            last.ranges = unique_ranges(last.ranges.iter().chain(piece.ranges.iter()).copied());
            last.insight_ids = unique_ids(
                last.insight_ids
                    .iter()
                    .chain(piece.insight_ids.iter())
                    .cloned(),
            );
            continue;
        }
        partitions.push(piece);
    }
    Ok(partitions)
}

pub fn make_units(entries: &[ContextEntry]) -> Vec<SourceUnit> {
    let mut units = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        let group = entries[index].atomic_group.clone();
        let atomic = group.is_some();
        let mut grouped = vec![entries[index].clone()];
        index += 1;
        if let Some(group_id) = &group {
            while index < entries.len() && entries[index].atomic_group.as_ref() == Some(group_id) {
                grouped.push(entries[index].clone());
                index += 1;
            }
        }
        let member_labels = grouped
            .iter()
            .map(|entry| {
                entry
                    .content
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(80)
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let prefix = match group {
            Some(group) => format!(
                "ATOMIC GROUP {group}\nMEMBERS: {}\n",
                member_labels.join(" | ")
            ),
            None => format!(
                "SOURCE {}..={}\n",
                grouped[0].range.start, grouped[0].range.end
            ),
        };
        let body = grouped
            .iter()
            .map(|entry| {
                format!(
                    "[sequence {}..={}]\n{}",
                    entry.range.start, entry.range.end, entry.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n---\n");
        units.push(SourceUnit {
            prefix,
            body,
            ranges: unique_ranges(grouped.iter().map(|entry| entry.range)),
            insight_ids: unique_insights(&grouped),
            atomic,
        });
    }
    units
}

pub fn split_unit(
    unit: SourceUnit,
    max_tokens: u64,
    overlap_tokens: u64,
) -> Result<Vec<Partition>, String> {
    let whole = format!("{}{}", unit.prefix, unit.body);
    let body: Vec<char> = unit.body.chars().collect();
    if unit.atomic || estimate_tokens(&whole) <= max_tokens {
        return Ok(vec![Partition {
            source: whole,
            ranges: unit.ranges,
            insight_ids: unit.insight_ids,
        }]);
    }
    let mut partitions = Vec::new();
    let mut start = 0;
    while start < body.len() {
        let end = largest_fitting_end(&unit.prefix, &body, start, max_tokens).ok_or_else(|| {
            format!("partition token bound {max_tokens} is too small for source metadata")
        })?;
        partitions.push(Partition {
            source: render_part(&unit.prefix, &body, start, end),
            ranges: unit.ranges.clone(),
            insight_ids: unit.insight_ids.clone(),
        });
        if end == body.len() {
            break;
        }
        start = overlap_start(&body, start, end, overlap_tokens)
            .max(start.saturating_add(1))
            .min(end);
    }
    Ok(partitions)
}

pub fn largest_fitting_end(
    prefix: &str,
    body: &[char],
    start: usize,
    max_tokens: u64,
) -> Option<usize> {
    let mut low = start.saturating_add(1);
    let mut high = body.len();
    let mut best = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        if estimate_tokens(&render_part(prefix, body, start, middle)) <= max_tokens {
            best = Some(middle);
            low = middle.saturating_add(1);
        } else {
            high = middle.saturating_sub(1);
        }
    }
    best
}

pub fn overlap_start(body: &[char], start: usize, end: usize, overlap_tokens: u64) -> usize {
    if overlap_tokens == 0 {
        return end;
    }
    let mut low = start;
    let mut high = end;
    let mut best = end;
    while low <= high {
        let middle = low + (high - low) / 2;
        let suffix = body[middle..end].iter().collect::<String>();
        if estimate_tokens(&suffix) <= overlap_tokens {
            best = middle;
            if middle == 0 {
                break;
            }
            high = middle - 1;
        } else {
            low = middle.saturating_add(1);
        }
    }
    best
}

pub fn render_part(prefix: &str, body: &[char], start: usize, end: usize) -> String {
    let slice = body[start..end].iter().collect::<String>();
    format!("{prefix}PART {start}..{end}\n{slice}")
}

pub fn source_digest(entries: &[ContextEntry]) -> Result<String, CompactionError> {
    let bytes = serde_json::to_vec(entries).map_err(|error| CompactionError::Serialization {
        message: error.to_string(),
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn unique_insights(entries: &[ContextEntry]) -> Vec<InsightId> {
    unique_ids(
        entries
            .iter()
            .flat_map(|entry| entry.insight_ids.iter())
            .cloned(),
    )
}

pub fn unique_ids(ids: impl IntoIterator<Item = InsightId>) -> Vec<InsightId> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

pub fn unique_ranges(ranges: impl IntoIterator<Item = SequenceRange>) -> Vec<SequenceRange> {
    let mut seen = HashSet::new();
    ranges
        .into_iter()
        .filter(|range| seen.insert((range.start, range.end)))
        .collect()
}

pub fn entry_tokens(entry: &ContextEntry) -> u64 {
    estimate_tokens(&entry.content)
}
