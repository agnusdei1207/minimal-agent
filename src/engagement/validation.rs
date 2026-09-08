use super::constants::MAX_ENGAGEMENT_TEXT_BYTES;
use super::error::EngagementError;

pub fn check_optional(field: &'static str, value: Option<&str>) -> Result<(), EngagementError> {
    match value {
        Some(value) => check_required(field, value),
        None => Ok(()),
    }
}

pub fn check_required(field: &'static str, value: &str) -> Result<(), EngagementError> {
    if value.trim().is_empty() {
        return Err(EngagementError::EmptyField { field });
    }
    if value.len() > MAX_ENGAGEMENT_TEXT_BYTES {
        return Err(EngagementError::TextTooLong {
            field,
            max: MAX_ENGAGEMENT_TEXT_BYTES,
        });
    }
    Ok(())
}
