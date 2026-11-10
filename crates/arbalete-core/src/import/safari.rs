//! Reads Safari's data, in `~/Library/Safari`.

use super::Copy;
use super::bookmarks::{Bookmark, Bookmarks, Folder};
use crate::history::{self, Entry};
use plist::{Dictionary, Value};
use rusqlite::Connection;
use std::{fs, io, io::Cursor, path::Path};

#[derive(Debug)]
pub enum Error {
    /// macOS lets an app read Safari's data only once the user grants it Full Disk Access.
    NeedsFullDiskAccess,
    Io(io::Error),
    Invalid(plist::Error),
    Sqlite(rusqlite::Error),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::PermissionDenied => Self::NeedsFullDiskAccess,
            _ => Self::Io(error),
        }
    }
}

/// Reads `History.db` in Safari's data directory. Safari never opened has none.
pub fn read_history(dir: &Path) -> Result<Vec<Entry>, Error> {
    let Some(copy) = Copy::of(&dir.join("History.db"))? else {
        return Ok(Vec::new());
    };
    let db = Connection::open(copy.file()).map_err(Error::Sqlite)?;
    history(&db).map_err(Error::Sqlite)
}

/// The visited urls of a `History.db`, with the title of their latest visit, whose times are
/// seconds since 2001.
fn history(db: &Connection) -> rusqlite::Result<Vec<Entry>> {
    history::select(
        db,
        "SELECT i.url, IFNULL(v.title, ''), i.visit_count,
            CAST((MAX(v.visit_time) + 978307200) * 1000 AS INTEGER)
         FROM history_items i JOIN history_visits v ON v.history_item = i.id
         GROUP BY i.id",
    )
}

/// Reads `Bookmarks.plist` in Safari's data directory. Safari without bookmarks has none.
pub fn read_bookmarks(dir: &Path) -> Result<Bookmarks, Error> {
    match fs::read(dir.join("Bookmarks.plist")) {
        Ok(plist) => bookmarks(&plist),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Bookmarks::default()),
        Err(error) => Err(error.into()),
    }
}

/// Whether macOS keeps Arbalete from reading Safari's data until the user grants it Full Disk
/// Access.
pub fn needs_full_disk_access(dir: &Path) -> bool {
    matches!(read_bookmarks(dir), Err(Error::NeedsFullDiskAccess))
}

const BAR: &str = "BookmarksBar";

/// Parses a `Bookmarks.plist`, binary or XML. Its root lists the bar, the bookmarks menu and the
/// reading list, which keep internal names as their titles, and a link to the history, skipped.
pub fn bookmarks(plist: &[u8]) -> Result<Bookmarks, Error> {
    let root = Value::from_reader(Cursor::new(plist)).map_err(Error::Invalid)?;
    let mut bookmarks = Bookmarks {
        bar: Folder {
            title: "Favorites Bar".to_string(),
            children: Vec::new(),
        },
        others: Vec::new(),
    };
    for list in children(root.as_dictionary()).filter(|d| kind(d) == "WebBookmarkTypeList") {
        let title = list
            .get("Title")
            .and_then(Value::as_string)
            .unwrap_or_default();
        let content = content(list);
        let title = match title {
            BAR => {
                bookmarks.bar.children = content;
                continue;
            }
            "BookmarksMenu" => "Bookmarks Menu",
            "com.apple.ReadingList" => "Reading List",
            title => title,
        };
        bookmarks.others.push(Folder {
            title: title.to_string(),
            children: content,
        });
    }
    Ok(bookmarks)
}

fn children(list: Option<&Dictionary>) -> impl Iterator<Item = &Dictionary> {
    let children = list.and_then(|l| l.get("Children")?.as_array());
    children
        .into_iter()
        .flatten()
        .filter_map(Value::as_dictionary)
}

fn kind(item: &Dictionary) -> &str {
    item.get("WebBookmarkType")
        .and_then(Value::as_string)
        .unwrap_or_default()
}

fn content(list: &Dictionary) -> Vec<Bookmark> {
    let text = |item: Option<&Value>| item.and_then(Value::as_string).map(str::to_string);
    children(Some(list))
        .filter_map(|item| match kind(item) {
            "WebBookmarkTypeLeaf" => Some(Bookmark::Link {
                title: text(
                    item.get("URIDictionary")
                        .and_then(|d| d.as_dictionary()?.get("title")),
                )
                .unwrap_or_default(),
                url: text(item.get("URLString"))?,
            }),
            "WebBookmarkTypeList" => Some(Bookmark::Folder(Folder {
                title: text(item.get("Title")).unwrap_or_default(),
                children: content(item),
            })),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::entry;
    use crate::import::TestDir;
    use crate::import::bookmarks::{folder, link};

    fn list(title: &str, children: Vec<Value>) -> Value {
        let mut list = Dictionary::new();
        list.insert("Title".into(), title.into());
        list.insert("WebBookmarkType".into(), "WebBookmarkTypeList".into());
        list.insert("Children".into(), children.into());
        list.into()
    }

    fn leaf(title: &str, url: &str) -> Value {
        let mut uri = Dictionary::new();
        uri.insert("title".into(), title.into());
        let mut leaf = Dictionary::new();
        leaf.insert("URIDictionary".into(), uri.into());
        leaf.insert("URLString".into(), url.into());
        leaf.insert("WebBookmarkType".into(), "WebBookmarkTypeLeaf".into());
        leaf.into()
    }

    /// A binary `Bookmarks.plist` with fictitious bookmarks, as Safari writes it.
    fn plist() -> Vec<u8> {
        let mut history = Dictionary::new();
        history.insert("Title".into(), "History".into());
        history.insert("WebBookmarkType".into(), "WebBookmarkTypeProxy".into());
        let root = list(
            "",
            vec![
                history.into(),
                list(
                    "BookmarksBar",
                    vec![
                        leaf("Mail", "https://mail.example.com/"),
                        list(
                            "Work",
                            vec![leaf("Tracker", "https://tracker.example.com/")],
                        ),
                    ],
                ),
                list(
                    "BookmarksMenu",
                    vec![leaf("News", "https://news.example.net/")],
                ),
                list(
                    "com.apple.ReadingList",
                    vec![leaf("Post", "https://blog.example.org/post")],
                ),
            ],
        );
        let mut plist = Vec::new();
        root.to_writer_binary(&mut plist).unwrap();
        plist
    }

    #[test]
    fn parses_the_bar_the_menu_and_the_reading_list() {
        assert_eq!(
            bookmarks(&plist()).unwrap(),
            Bookmarks {
                bar: folder(
                    "Favorites Bar",
                    vec![
                        link("Mail", "https://mail.example.com/"),
                        Bookmark::Folder(folder(
                            "Work",
                            vec![link("Tracker", "https://tracker.example.com/")]
                        )),
                    ]
                ),
                others: vec![
                    folder(
                        "Bookmarks Menu",
                        vec![link("News", "https://news.example.net/")]
                    ),
                    folder(
                        "Reading List",
                        vec![link("Post", "https://blog.example.org/post")]
                    ),
                ],
            }
        );
        assert!(matches!(bookmarks(b"not a plist"), Err(Error::Invalid(_))));
    }

    #[test]
    fn reads_the_bookmarks_file_in_safari_s_directory() {
        let dir = TestDir::new("safari");
        assert_eq!(read_bookmarks(&dir.0).unwrap(), Bookmarks::default());
        dir.write("Bookmarks.plist", plist());
        assert_eq!(
            read_bookmarks(&dir.0).unwrap(),
            bookmarks(&plist()).unwrap()
        );
    }

    /// The part of Safari's `History.db` schema the import reads, with fictitious visits.
    const HISTORY: &str = "
        CREATE TABLE history_items (id INTEGER PRIMARY KEY, url TEXT NOT NULL UNIQUE,
            visit_count INTEGER NOT NULL);
        CREATE TABLE history_visits (id INTEGER PRIMARY KEY, history_item INTEGER NOT NULL,
            visit_time REAL NOT NULL, title TEXT);
        INSERT INTO history_items (id, url, visit_count) VALUES
            (1, 'https://mail.example.com/', 2), (2, 'https://news.example.net/', 1);
        INSERT INTO history_visits (history_item, visit_time, title) VALUES
            (1, 727219200.5, 'Old mail'), (1, 727219300.25, 'Mail'), (2, 727219250.0, NULL);
    ";

    #[test]
    fn reads_the_visited_urls_with_their_latest_title_and_unix_times() {
        let dir = TestDir::new("safari-history");
        assert_eq!(read_history(&dir.0).unwrap(), vec![]);
        let db = Connection::open(dir.0.join("History.db")).unwrap();
        db.execute_batch(HISTORY).unwrap();
        assert_eq!(
            read_history(&dir.0).unwrap(),
            vec![
                entry("https://mail.example.com/", "Mail", 2, 1_705_526_500_250),
                entry("https://news.example.net/", "", 1, 1_705_526_450_000),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn asks_for_full_disk_access_when_a_file_cannot_be_read() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("safari-denied");
        dir.write("Bookmarks.plist", plist());
        dir.write("History.db", "");
        for file in ["Bookmarks.plist", "History.db"] {
            let file = dir.0.join(file);
            fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        }
        // Root reads a file whatever its permissions, as in a container: nothing is denied then.
        if fs::read(dir.0.join("Bookmarks.plist")).is_ok() {
            return;
        }
        assert!(matches!(
            read_bookmarks(&dir.0),
            Err(Error::NeedsFullDiskAccess)
        ));
        assert!(matches!(
            read_history(&dir.0),
            Err(Error::NeedsFullDiskAccess)
        ));
        assert!(needs_full_disk_access(&dir.0));
        let readable = TestDir::new("safari-readable");
        assert!(!needs_full_disk_access(&readable.0));
    }
}
