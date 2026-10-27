use std::path::{Component, Path, PathBuf};

use path_clean::PathClean;
use relative_path::RelativePath;
use serde::{Deserialize, Serialize};

pub mod local;
mod upload;
mod watcher;

use crate::utils::{errors::ServiceError, paths::resolve_existing_ancestor};
use local::LocalStorage;
pub(crate) use upload::MAX_UPLOAD_REQUEST_SIZE;
pub use upload::{UploadStatus, UploadStatusQuery};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PathObject {
    pub source: String,
    parent: Option<String>,
    parent_folders: Option<Vec<String>>,
    folders: Option<Vec<String>>,
    /// Subfolders with a program sheet (`.nfo`) of their own.
    #[serde(default)]
    sheet_folders: Option<Vec<String>>,
    /// The ones among `sheet_folders` whose sheet is a film's.
    #[serde(default)]
    movie_folders: Option<Vec<String>>,
    /// Subfolders holding a live's address file (`.live`).
    #[serde(default)]
    live_folders: Option<Vec<String>>,
    /// Subfolders marked as commercials folders.
    #[serde(default)]
    commercial_folders: Option<Vec<String>>,
    /// The commercials folder the browsed folder lies in (itself or a parent),
    /// relative to the storage.
    #[serde(default)]
    commercial: Option<String>,
    files: Option<Vec<VideoFile>>,
    #[serde(default)]
    pub folders_only: bool,
    /// List every file, not only the media the channel can play.
    #[serde(default)]
    pub all_files: bool,
    #[serde(default)]
    pub recursive: bool,
}

impl PathObject {
    fn new(source: String, parent: Option<String>) -> Self {
        Self {
            source,
            parent,
            parent_folders: Some(vec![]),
            folders: Some(vec![]),
            sheet_folders: Some(vec![]),
            movie_folders: Some(vec![]),
            live_folders: Some(vec![]),
            commercial_folders: Some(vec![]),
            commercial: None,
            files: Some(vec![]),
            folders_only: false,
            all_files: false,
            recursive: false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MoveObject {
    source: String,
    target: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VideoFile {
    name: String,
    duration: f64,
    /// Size in bytes.
    #[serde(default)]
    size: u64,
    /// Last change, seconds since the Unix epoch.
    #[serde(default)]
    modified: Option<i64>,
    /// Whether the channel can play it (its extension is a media one).
    #[serde(default = "default_true")]
    media: bool,
    /// Whether it has a sheet of its own (`<name>.nfo` next to it).
    #[serde(default)]
    sheet: bool,
    /// Whether its marks (`<name>.logo.json`) keep the logo off somewhere or
    /// mark SCTE-35 breaks; a trim alone is `trim`.
    #[serde(default)]
    logo_marks: bool,
    /// The part of the video that airs by default, saved in its marks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trim: Option<crate::utils::logo_marks::LogoSpan>,
}

fn default_true() -> bool {
    true
}

pub async fn init_storage(
    root: PathBuf,
    extensions: Vec<String>,
) -> Result<LocalStorage, ServiceError> {
    LocalStorage::new(root, extensions).await
}

/// Normalize absolut path
///
/// This function takes care, that it is not possible to break out from root_path.
pub fn norm_abs_path(
    root_path: &Path,
    input_path: &str,
) -> Result<(PathBuf, String, String), ServiceError> {
    if Path::new(input_path)
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(ServiceError::Forbidden("Access denied".to_string()));
    }

    let path_relative = RelativePath::new(&root_path.to_string_lossy())
        .normalize()
        .to_string();
    let path_suffix = root_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut source_relative = RelativePath::new(input_path).normalize().to_string();

    if Path::new(&source_relative).starts_with(&path_relative) {
        source_relative = source_relative
            .strip_prefix(&path_relative)
            .and_then(|suffix| suffix.strip_prefix('/'))
            .unwrap_or_default()
            .to_string();
    } else {
        source_relative = source_relative
            .strip_prefix(&path_suffix)
            .and_then(|s| s.strip_prefix('/'))
            .unwrap_or(&source_relative)
            .to_string();
    }

    let path = root_path.join(&source_relative);

    // Defensive containment check: the cleaned absolute path must never leave
    // the storage root, regardless of the normalization above.
    let cleaned = path.clean();
    let cleaned_root = root_path.clean();
    ensure_path_within_root(&cleaned_root, &cleaned)?;

    Ok((path, path_suffix, source_relative))
}

pub(crate) fn ensure_path_within_root(root: &Path, path: &Path) -> Result<(), ServiceError> {
    let root = root.clean();
    let path = path.clean();

    if !path.starts_with(&root) {
        return Err(ServiceError::Forbidden("Access denied".to_string()));
    }

    let resolved_root = resolve_existing_ancestor(&root)?;
    let resolved_path = resolve_existing_ancestor(&path)?;

    if !resolved_path.starts_with(resolved_root) {
        return Err(ServiceError::Forbidden("Access denied".to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_paths_preserve_virtual_roots_and_use_component_boundaries() {
        let root = std::env::temp_dir().join(format!("noar-paths-{}", uuid::Uuid::new_v4()));

        for input in ["", "/", "video.mp4", "/video.mp4", "folder/video.mp4"] {
            let (path, _, _) = norm_abs_path(&root, input).unwrap();

            assert!(path.starts_with(&root), "{input}");
        }

        let file = root.join("video.mp4");
        assert_eq!(
            norm_abs_path(&root, &file.to_string_lossy()).unwrap().0,
            file
        );
        let similar_root = format!("{}-other/video.mp4", root.display());
        let (path, _, _) = norm_abs_path(&root, &similar_root).unwrap();

        assert_ne!(path, root);
        assert!(path.starts_with(&root));
    }

    #[test]
    fn normalized_paths_reject_parent_traversal() {
        let root = std::env::temp_dir().join("noar-storage");

        for input in [
            "..",
            "../secret",
            "folder/../../secret",
            "folder/../video.mp4",
            "/../secret",
        ] {
            assert!(
                matches!(norm_abs_path(&root, input), Err(ServiceError::Forbidden(_))),
                "{input}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn containment_checks_symlinks_for_existing_files_and_new_targets() {
        let directory =
            std::env::temp_dir().join(format!("noar-symlinks-{}", uuid::Uuid::new_v4()));
        let root = directory.join("storage");
        let outside = directory.join("outside");
        std::fs::create_dir_all(root.join("inside")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret"), "outside").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        std::os::unix::fs::symlink(root.join("inside"), root.join("safe")).unwrap();
        std::os::unix::fs::symlink(outside.join("missing"), root.join("dangling")).unwrap();

        assert!(norm_abs_path(&root, "escape/secret").is_err());
        assert!(norm_abs_path(&root, "escape/new-file").is_err());
        assert!(norm_abs_path(&root, "dangling").is_err());
        assert!(norm_abs_path(&root, "safe/new-file").is_ok());
        assert!(ensure_path_within_root(&root, &outside.join("secret")).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
