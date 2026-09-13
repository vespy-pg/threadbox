//! Content-addressed media storage.
//!
//! Every stored file lives under the media root as `blobs/<first two hex digits>/<sha256><ext>`,
//! and the database records only that relative path. Absolute paths never enter a row, so the data
//! directory can be moved, restored on another machine or later synchronised without rewriting the
//! database. Identical bytes stored twice occupy one file, which matters as soon as the same
//! screenshot is attached to a task and a project document.

use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Directory holding every content-addressed file, relative to the media root.
pub(crate) const BLOB_DIRECTORY: &str = "blobs";

/// True when the value carries its own bytes instead of pointing at a stored file.
pub(crate) fn is_data_url(value: &str) -> bool {
    value.starts_with("data:")
}

/// Decodes the base64 payload of a `data:` URL.
pub(crate) fn decode_data_url(data_url: &str) -> AppResult<Vec<u8>> {
    let encoded = data_url
        .split_once(',')
        .map(|(_, value)| value)
        .ok_or_else(|| AppError::InvalidInput("Invalid attachment data".into()))?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| AppError::InvalidInput(format!("Invalid attachment data: {error}")))
}

/// Writes the bytes under their content hash and returns the reference to record in the database.
///
/// Writing is atomic through a temporary file, so an interrupted write never leaves a blob whose
/// name promises content it does not hold.
pub(crate) fn store_bytes(root: &Path, bytes: &[u8], extension: &str) -> AppResult<String> {
    let digest = Sha256::digest(bytes);
    let hash = digest.iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
        text
    });
    let extension = sanitised_extension(extension);
    let shard = &hash[..2];
    let reference = format!("{BLOB_DIRECTORY}/{shard}/{hash}{extension}");
    let destination = root
        .join(BLOB_DIRECTORY)
        .join(shard)
        .join(format!("{hash}{extension}"));
    if destination.exists() {
        return Ok(reference);
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_file_name(format!(".tmp-{}", Uuid::new_v4()));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, &destination)?;
    Ok(reference)
}

/// Stores a file already on disk, moving it into the blob store when possible.
pub(crate) fn store_file(root: &Path, source: &Path, extension: &str) -> AppResult<String> {
    let bytes = fs::read(source)?;
    let reference = store_bytes(root, &bytes, extension)?;
    let stored = root.join(reference_to_relative(&reference));
    if source != stored {
        let _ = fs::remove_file(source);
    }
    Ok(reference)
}

/// Resolves a stored reference to an absolute path inside the media root.
///
/// Rejects absolute paths and any traversal, so a reference that arrived from the interface cannot
/// reach a file outside Threadbox storage.
pub(crate) fn resolve(root: &Path, reference: &str) -> AppResult<PathBuf> {
    let relative = Path::new(reference);
    if relative.is_absolute() || reference.is_empty() {
        return Err(outside_storage());
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(part) => path.push(part),
            _ => return Err(outside_storage()),
        }
    }
    if let (Ok(canonical), Ok(canonical_root)) = (path.canonicalize(), root.canonicalize()) {
        if !canonical.starts_with(&canonical_root) {
            return Err(outside_storage());
        }
        return Ok(canonical);
    }
    Ok(path)
}

/// Deletes every blob that no reference points at, together with the shard directories it empties.
///
/// Soft-deleted rows still count as references: only a permanent delete frees the bytes.
pub(crate) fn collect_garbage(root: &Path, referenced: &HashSet<String>) -> AppResult<usize> {
    let blobs = root.join(BLOB_DIRECTORY);
    if !blobs.exists() {
        return Ok(0);
    }
    let kept = referenced
        .iter()
        .map(|reference| root.join(reference_to_relative(reference)))
        .collect::<HashSet<_>>();
    let mut removed = 0;
    for shard in fs::read_dir(&blobs)? {
        let shard = shard?.path();
        if !shard.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&shard)? {
            let path = entry?.path();
            if path.is_file() && !kept.contains(&path) {
                fs::remove_file(&path)?;
                removed += 1;
            }
        }
        if fs::read_dir(&shard)?.next().is_none() {
            fs::remove_dir(&shard)?;
        }
    }
    Ok(removed)
}

/// The file extension to store a name under, including the leading dot.
pub(crate) fn extension_of(name: &str, fallback: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_else(|| fallback.to_string())
}

/// References are recorded with forward slashes regardless of the platform that wrote them.
fn reference_to_relative(reference: &str) -> PathBuf {
    reference
        .split('/')
        .fold(PathBuf::new(), |path, part| path.join(part))
}

/// Keeps an extension that is safe as a file name, and drops anything else.
fn sanitised_extension(extension: &str) -> String {
    let trimmed = extension.trim_start_matches('.');
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() <= 12 && trimmed.chars().all(|value| value.is_ascii_alphanumeric()) {
        format!(".{}", trimmed.to_ascii_lowercase())
    } else {
        ".bin".into()
    }
}

fn outside_storage() -> AppError {
    AppError::InvalidInput("Attachment path is outside Threadbox storage".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("threadbox-media-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn stores_identical_bytes_once() {
        let root = temporary_root();
        let first = store_bytes(&root, b"screenshot", ".png").unwrap();
        let second = store_bytes(&root, b"screenshot", ".png").unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with("blobs/"));
        assert!(resolve(&root, &first).unwrap().is_file());
        let shard = root.join("blobs").join(&first[6..8]);
        assert_eq!(fs::read_dir(shard).unwrap().count(), 1);
    }

    #[test]
    fn refuses_references_that_leave_the_media_root() {
        let root = temporary_root();
        assert!(resolve(&root, "../secrets.txt").is_err());
        assert!(resolve(&root, "/etc/passwd").is_err());
        assert!(resolve(&root, "").is_err());
    }

    #[test]
    fn garbage_collection_keeps_referenced_blobs_only() {
        let root = temporary_root();
        let kept = store_bytes(&root, b"kept", ".png").unwrap();
        let dropped = store_bytes(&root, b"dropped", ".png").unwrap();
        let referenced = HashSet::from([kept.clone()]);
        assert_eq!(collect_garbage(&root, &referenced).unwrap(), 1);
        assert!(resolve(&root, &kept).unwrap().is_file());
        assert!(!resolve(&root, &dropped).unwrap().is_file());
    }

    #[test]
    fn unsafe_extensions_fall_back_to_a_plain_one() {
        assert_eq!(sanitised_extension(".png"), ".png");
        assert_eq!(sanitised_extension("PNG"), ".png");
        assert_eq!(sanitised_extension(""), "");
        assert_eq!(sanitised_extension("../../etc"), ".bin");
    }
}
