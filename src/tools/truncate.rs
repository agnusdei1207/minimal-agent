use serde_json::Value;
use std::io::{self, Write};
use tokio::io::{AsyncRead, AsyncReadExt};

use super::constants::{MAX_TOOL_ARGUMENT_BYTES, MAX_TOOL_CONTEXT_BYTES, READ_BUFFER_SIZE};
use super::error::ToolError;

pub struct BoundedJsonCounter {
    pub bytes: usize,
    pub exceeded: bool,
}

impl Write for BoundedJsonCounter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(total) = self.bytes.checked_add(buffer.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("tool arguments exceeded their byte limit"));
        };
        if total > MAX_TOOL_ARGUMENT_BYTES {
            self.exceeded = true;
            return Err(io::Error::other("tool arguments exceeded their byte limit"));
        }
        self.bytes = total;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn enforce_argument_limit(arguments: &Value) -> Result<(), ToolError> {
    let mut counter = BoundedJsonCounter {
        bytes: 0,
        exceeded: false,
    };
    let encoded = serde_json::to_writer(&mut counter, arguments);
    if counter.exceeded {
        return Err(ToolError::ArgumentLimit(MAX_TOOL_ARGUMENT_BYTES));
    }
    encoded?;
    Ok(())
}

/// Truncate a large tool result to a head + tail with an elision marker so a
/// single big output cannot bloat the model context (INTENT-0002 §3.15). The head
/// keeps the start (errors, setup) and the tail keeps the end (results, flags).
pub fn truncate_tool_content(content: String) -> String {
    if content.len() <= MAX_TOOL_CONTEXT_BYTES {
        return content;
    }
    let head_budget = MAX_TOOL_CONTEXT_BYTES * 3 / 5;
    let tail_budget = MAX_TOOL_CONTEXT_BYTES - head_budget;
    let head_end = floor_char_boundary(&content, head_budget);
    let tail_start = ceil_char_boundary(&content, content.len().saturating_sub(tail_budget));
    if tail_start <= head_end {
        return content;
    }
    let omitted = tail_start - head_end;
    format!(
        "{}\n\n[... {omitted} bytes omitted; re-run with head/tail/grep or redirect to a file for the full output ...]\n\n{}",
        &content[..head_end],
        &content[tail_start..],
    )
}

pub fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

pub fn ceil_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

pub async fn read_bounded<R>(mut reader: R, max: usize) -> Result<Vec<u8>, ToolError>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut buffer = [0_u8; READ_BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > max {
            return Err(ToolError::OutputLimit(max));
        }
        output.extend_from_slice(&buffer[..read]);
    }
}
