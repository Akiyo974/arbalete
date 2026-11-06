//! Downloads: where they are saved, the list kept across sessions, and what the UI shows of
//! them.

use arbalete_core::sidebar::ProfileId;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path, path::PathBuf};

/// The downloads list's file name in the data directory.
pub const FILE_NAME: &str = "downloads.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    /// Unique in the list, across sessions.
    pub id: u32,
    /// CEF's id for a download of this session, which CEF reuses in the next one.
    #[serde(skip)]
    pub cef_id: Option<u32>,
    /// The file's name, as saved.
    pub name: String,
    pub path: PathBuf,
    /// The address the file came from.
    pub url: String,
    pub received: i64,
    /// 0 while the size is unknown.
    pub total: i64,
    /// Bytes per second, while it runs: the library shows the time left from it.
    #[serde(default)]
    pub speed: i64,
    pub state: State,
    /// The profile of the page it came from, which the library filters by: none for the
    /// downloads saved before it was kept.
    #[serde(default)]
    pub profile: Option<ProfileId>,
    /// When it started, in milliseconds since the Unix epoch.
    pub started_at: i64,
    /// The file is gone since it was saved: checked when the list is loaded.
    #[serde(skip_deserializing)]
    pub missing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    InProgress,
    Complete,
    Canceled,
    Failed,
}

impl Download {
    /// A file the app saved itself, like a capture kept in the library, complete already.
    pub fn saved(id: u32, path: PathBuf, url: String, started_at: i64) -> Self {
        let size = fs::metadata(&path).map_or(0, |meta| meta.len() as i64);
        Self {
            cef_id: None,
            received: size,
            total: size,
            state: State::Complete,
            ..Self::new(id, 0, path, url, started_at)
        }
    }

    /// A download of this session, `cef_id` for CEF, about to be saved at `path`.
    pub fn new(id: u32, cef_id: u32, path: PathBuf, url: String, started_at: i64) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            id,
            cef_id: Some(cef_id),
            name,
            path,
            url,
            received: 0,
            total: 0,
            speed: 0,
            state: State::InProgress,
            profile: None,
            started_at,
            missing: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    downloads: Vec<Download>,
}

const VERSION: u32 = 1;

/// Writes the list atomically: a crash while saving leaves the previous file intact.
pub fn save(downloads: &[Download], path: &Path) -> io::Result<()> {
    let saved = Saved {
        version: VERSION,
        downloads: downloads.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&saved).map_err(io::Error::other)?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, json)?;
    fs::rename(temporary, path)
}

/// Loads the list, empty when there is none or it cannot be read: a download still in progress
/// when the app quit failed, and a finished one whose file is gone is marked missing.
pub fn load(path: &Path) -> Vec<Download> {
    let saved: Option<Saved> = fs::read(path)
        .ok()
        .and_then(|json| serde_json::from_slice(&json).ok());
    let Some(saved) = saved.filter(|saved| saved.version == VERSION) else {
        return Vec::new();
    };
    let mut downloads = saved.downloads;
    for download in &mut downloads {
        if download.state == State::InProgress {
            download.state = State::Failed;
        }
        download.missing = download.state == State::Complete && !download.path.exists();
    }
    downloads
}

/// The media type of an image or a video, which the library's Media section shows, by the file's
/// extension; none for other files.
pub fn media_type(name: &str) -> Option<&'static str> {
    let extension = name.rsplit_once('.')?.1.to_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => return None,
    })
}

/// The id for a new download in the list.
pub fn next_id(downloads: &[Download]) -> u32 {
    downloads
        .iter()
        .map(|download| download.id + 1)
        .max()
        .unwrap_or(0)
}

/// A path in `dir` for a file named `name` that `taken` does not refuse: `report.pdf`, then
/// `report (1).pdf`, `report (2).pdf`… A name holding a path keeps only its last part.
pub fn unique_path(dir: &Path, name: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let name = Path::new(name)
        .file_name()
        .map_or_else(|| "download".into(), |name| name.to_string_lossy());
    let path = dir.join(name.as_ref());
    if !taken(&path) {
        return path;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        // A leading dot starts a hidden file's name, not an extension.
        Some((stem, extension)) if !stem.is_empty() => (stem, format!(".{extension}")),
        _ => (name.as_ref(), String::new()),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){extension}")))
        .find(|path| !taken(path))
        .expect("a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taken(names: &[&str]) -> impl Fn(&Path) -> bool {
        let paths: Vec<PathBuf> = names
            .iter()
            .map(|name| Path::new("/d").join(name))
            .collect();
        move |path| paths.iter().any(|taken| taken == path)
    }

    #[test]
    fn tells_images_and_videos_by_their_extension() {
        assert_eq!(media_type("photo.JPG"), Some("image/jpeg"));
        assert_eq!(media_type("clip.final.webm"), Some("video/webm"));
        assert_eq!(media_type("report.pdf"), None);
        assert_eq!(media_type("png"), None);
    }

    #[test]
    fn keeps_a_free_name() {
        let path = unique_path(Path::new("/d"), "report.pdf", taken(&[]));
        assert_eq!(path, Path::new("/d/report.pdf"));
    }

    #[test]
    fn numbers_a_taken_name_before_its_extension() {
        let path = unique_path(
            Path::new("/d"),
            "report.pdf",
            taken(&["report.pdf", "report (1).pdf"]),
        );
        assert_eq!(path, Path::new("/d/report (2).pdf"));
    }

    #[test]
    fn numbers_names_without_an_extension_and_hidden_files_at_the_end() {
        let path = unique_path(Path::new("/d"), "notes", taken(&["notes"]));
        assert_eq!(path, Path::new("/d/notes (1)"));
        let path = unique_path(Path::new("/d"), ".env", taken(&[".env"]));
        assert_eq!(path, Path::new("/d/.env (1)"));
    }

    #[test]
    fn keeps_only_the_last_part_of_a_name() {
        let path = unique_path(Path::new("/d"), "../etc/passwd", taken(&[]));
        assert_eq!(path, Path::new("/d/passwd"));
        let path = unique_path(Path::new("/d"), "", taken(&[]));
        assert_eq!(path, Path::new("/d/download"));
    }

    #[test]
    fn names_a_download_after_its_file() {
        let download = Download::new(
            0,
            7,
            PathBuf::from("/d/report (1).pdf"),
            "https://example.com/report.pdf".into(),
            1,
        );
        assert_eq!(download.name, "report (1).pdf");
        assert_eq!(download.state, State::InProgress);
    }

    /// A directory of its own under the system's temporary directory, removed on drop.
    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("arbalete-{name}-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn keeps_the_list_across_sessions() {
        let dir = TestDir::new("downloads");
        let file = dir.0.join("report.pdf");
        fs::write(&file, "pdf").unwrap();
        let mut done = Download::new(0, 5, file, "https://example.com/r".into(), 10);
        done.state = State::Complete;
        let running = Download::new(
            1,
            6,
            dir.0.join("big.iso"),
            "https://example.com/b".into(),
            20,
        );
        let mut gone = Download::new(
            2,
            7,
            dir.0.join("gone.zip"),
            "https://example.com/g".into(),
            30,
        );
        gone.state = State::Complete;
        let list = dir.0.join(FILE_NAME);
        save(&[done.clone(), running, gone], &list).unwrap();

        let loaded = load(&list);
        assert_eq!(loaded[0].name, "report.pdf");
        assert_eq!(loaded[0].cef_id, None);
        assert!(!loaded[0].missing);
        assert_eq!(loaded[1].state, State::Failed);
        assert!(loaded[2].missing);
        assert_eq!(next_id(&loaded), 3);
    }

    #[test]
    fn loads_nothing_from_a_missing_or_invalid_file() {
        let dir = TestDir::new("downloads-invalid");
        assert!(load(&dir.0.join(FILE_NAME)).is_empty());
        fs::write(dir.0.join(FILE_NAME), "not json").unwrap();
        assert!(load(&dir.0.join(FILE_NAME)).is_empty());
        assert_eq!(next_id(&[]), 0);
    }
}
