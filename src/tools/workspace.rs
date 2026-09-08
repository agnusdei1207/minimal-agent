use std::path::{Component, Path, PathBuf};

use super::error::ToolError;

pub fn normalized_relative(path: &str) -> Result<PathBuf, ToolError> {
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ToolError::OutsideWorkspace(path.display().to_string()));
    }
    Ok(path.to_path_buf())
}

pub fn resolve_existing(root: &Path, input: &str) -> Result<PathBuf, ToolError> {
    let candidate = root.join(normalized_relative(input)?);
    let canonical = candidate.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(ToolError::OutsideWorkspace(input.to_owned()));
    }
    Ok(canonical)
}

pub fn resolve_for_write(root: &Path, input: &str) -> Result<PathBuf, ToolError> {
    let relative = normalized_relative(input)?;
    let file_name = relative
        .file_name()
        .ok_or_else(|| ToolError::InvalidArguments("write path needs a file name".to_owned()))?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let parent_candidate = root.join(parent);
    std::fs::create_dir_all(&parent_candidate)?;
    let canonical_parent = parent_candidate.canonicalize()?;
    if !canonical_parent.starts_with(root) {
        return Err(ToolError::OutsideWorkspace(input.to_owned()));
    }
    let candidate = canonical_parent.join(file_name);
    if candidate.exists() && !candidate.canonicalize()?.starts_with(root) {
        return Err(ToolError::OutsideWorkspace(input.to_owned()));
    }
    Ok(candidate)
}
