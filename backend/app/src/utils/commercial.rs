//! Folders of commercials.
//!
//! A folder holding the hidden file `.comercial` is a commercials folder: every
//! clip in it or in its subfolders plays as a commercial wherever it is used
//! (playlist, scheduler, generated days). The logo is off while it plays, and
//! the guide adds its time to the programme around it instead of listing it.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use path_clean::PathClean;
use tokio::fs;

use crate::{file::norm_abs_path, utils::errors::ServiceError};

/// Name of the marker file of a commercials folder.
pub const COMMERCIAL_MARKER: &str = ".comercial";

const MARKER_TEXT: &str =
    "NoAr: everything in this folder and its subfolders plays as a commercial.\n";

/// Answers whether clips are commercials, remembering the folders it checked.
pub struct CommercialFolders {
    storage: PathBuf,
    folders: HashMap<PathBuf, Option<PathBuf>>,
}

impl CommercialFolders {
    pub fn new(storage: &Path) -> Self {
        Self {
            storage: storage.clean(),
            folders: HashMap::new(),
        }
    }

    /// Whether `source` (absolute, or relative to the storage) lies in a
    /// commercials folder. Remote sources never do.
    pub fn contains(&mut self, source: &str) -> bool {
        self.folder_of(source).is_some()
    }

    /// The commercials folder `source` lies in, if any.
    pub fn folder_of(&mut self, source: &str) -> Option<PathBuf> {
        if source.is_empty() || source.contains("://") {
            return None;
        }

        let path = Path::new(source);
        let path = if path.is_absolute() {
            path.clean()
        } else {
            self.storage.join(path).clean()
        };

        self.marked(path.parent()?)
    }

    /// The marked folder covering `dir`: `dir` itself or one of its parents,
    /// inside the storage (the storage root itself cannot be marked).
    fn marked(&mut self, dir: &Path) -> Option<PathBuf> {
        if !dir.starts_with(&self.storage) || dir == self.storage {
            return None;
        }

        if let Some(known) = self.folders.get(dir) {
            return known.clone();
        }

        let found = if dir.join(COMMERCIAL_MARKER).is_file() {
            Some(dir.to_path_buf())
        } else {
            dir.parent().and_then(|parent| self.marked(parent))
        };

        self.folders.insert(dir.to_path_buf(), found.clone());

        found
    }
}

/// Whether one source lies in a commercials folder.
pub fn is_commercial(storage: &Path, source: &str) -> bool {
    CommercialFolders::new(storage).contains(source)
}

/// Mark a storage folder (relative to the storage) as a commercials folder, or
/// remove the mark.
pub async fn set_commercial(
    storage: &Path,
    folder: &str,
    commercial: bool,
) -> Result<(), ServiceError> {
    let (dir, _, _) = norm_abs_path(storage, folder)?;

    if dir.clean() == storage.clean() {
        return Err(ServiceError::BadRequest(
            "the storage root cannot be a commercials folder".to_string(),
        ));
    }

    if !fs::metadata(&dir).await.is_ok_and(|meta| meta.is_dir()) {
        return Err(ServiceError::NotFound(format!("{folder} is not a folder")));
    }

    let marker = dir.join(COMMERCIAL_MARKER);

    if commercial {
        fs::write(&marker, MARKER_TEXT).await?;
    } else if fs::try_exists(&marker).await? {
        fs::remove_file(&marker).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempStorage(PathBuf);

    impl TempStorage {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "noar-commercial-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(path.join("Breaks/Sponsors")).unwrap();
            std::fs::create_dir_all(path.join("Show")).unwrap();

            Self(path)
        }
    }

    impl Drop for TempStorage {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn clips_in_a_marked_folder_and_its_subfolders_are_commercials() {
        let storage = TempStorage::new();
        let absolute = storage.0.join("Breaks/Sponsors/a.mp4");

        assert!(!is_commercial(&storage.0, "Breaks/ad.mp4"));

        set_commercial(&storage.0, "Breaks", true).await.unwrap();

        let mut folders = CommercialFolders::new(&storage.0);

        assert!(folders.contains("Breaks/ad.mp4"));
        assert!(folders.contains(&absolute.to_string_lossy()));
        assert_eq!(
            folders.folder_of("Breaks/Sponsors/a.mp4"),
            Some(storage.0.join("Breaks"))
        );
        assert!(!folders.contains("Show/ep1.mp4"));
        assert!(!folders.contains("https://cdn.example.org/Breaks/ad.mp4"));

        set_commercial(&storage.0, "Breaks", false).await.unwrap();

        assert!(!is_commercial(&storage.0, "Breaks/ad.mp4"));
        assert!(set_commercial(&storage.0, "", true).await.is_err());
        assert!(set_commercial(&storage.0, "Missing", true).await.is_err());
        assert!(
            set_commercial(&storage.0, "../outside", true)
                .await
                .is_err()
        );
    }
}
