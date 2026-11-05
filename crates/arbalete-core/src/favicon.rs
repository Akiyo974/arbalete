//! The icons the sidebar shows, kept as PNG files in each profile's directory, so that a tab
//! shows its icon before its page loads: unloaded, after a restart or an import.
//!
//! A tab's `favicon` names where its icon comes from: the icon's url, or the imported item it was
//! found for. Its file is named after the md5 of that name: tabs showing the same icon share one
//! file, and a file never changes, as another icon has another name.

use crate::sidebar::{ProfileId, Sidebar};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{fs, io};

/// The directory of the icons in a profile's directory. Not `Favicons`: Chromium's own database
/// has that name there.
pub const DIR: &str = "Sidebar Favicons";

/// The file of the icon `source`, in `DIR`.
pub fn file_name(source: &str) -> String {
    format!("{:x}.png", md5::compute(source))
}

/// Whether `name` is the name of an icon's file, so that a request names no other file.
pub fn is_file_name(name: &str) -> bool {
    name.strip_suffix(".png")
        .is_some_and(|key| key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn path(profile_dir: &Path, source: &str) -> PathBuf {
    profile_dir.join(DIR).join(file_name(source))
}

pub fn write(profile_dir: &Path, source: &str, png: &[u8]) -> io::Result<()> {
    fs::create_dir_all(profile_dir.join(DIR))?;
    fs::write(path(profile_dir, source), png)
}

/// Chromium's own database of the icons of the pages a profile visited, in its directory, read
/// only. Chromium keeps it locked while it runs: it is opened as immutable, which takes no lock.
pub fn chromium(profile_dir: &Path) -> Option<Connection> {
    let path = profile_dir.join("Favicons");
    if !path.exists() {
        return None;
    }
    let path = path.to_str()?;
    let path: String = path
        .chars()
        .map(|c| match c {
            '%' | ' ' | '?' | '#' => format!("%{:02X}", c as u32),
            c => c.to_string(),
        })
        .collect();
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    Connection::open_with_flags(format!("file:{path}?immutable=1"), flags).ok()
}

/// The icon Chromium found for a page, else for another page of its site, cached in `DIR` if it
/// is not yet: its source, its url, as a tab names it. Its largest size is kept.
pub fn of_page(profile_dir: &Path, chromium: &Connection, page_url: &str) -> Option<String> {
    let rest = page_url.split_once("://")?.1;
    let origin = &page_url[..page_url.len() - rest.len() + rest.find('/').unwrap_or(rest.len())];
    let (source, png): (String, Vec<u8>) = chromium
        .prepare_cached(
            "SELECT f.url, b.image_data FROM icon_mapping m
             JOIN favicons f ON f.id = m.icon_id JOIN favicon_bitmaps b ON b.icon_id = m.icon_id
             WHERE m.page_url = ?1 OR substr(m.page_url, 1, length(?2)) = ?2
             ORDER BY m.page_url = ?1 DESC, b.width DESC LIMIT 1",
        )
        .ok()?
        .query_row([page_url, &format!("{origin}/")], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()
        .ok()??;
    if !path(profile_dir, &source).exists() {
        write(profile_dir, &source, &png).ok()?;
    }
    Some(source)
}

/// Forgets the icons of tabs whose file is missing, as an older session's icon urls, and deletes
/// the files no tab of their profile shows any more. Errors are left for the next launch.
pub fn sweep(sidebar: &mut Sidebar, profile_dir: impl Fn(ProfileId) -> PathBuf) {
    let profiles: Vec<_> = sidebar.profiles().iter().map(|p| p.id).collect();
    for profile in profiles {
        let dir = profile_dir(profile).join(DIR);
        let files: HashSet<String> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        let tabs: Vec<_> = sidebar
            .tabs()
            .filter(|&(id, _)| sidebar.profile_of(id) == Some(profile))
            .filter_map(|(id, tab)| Some((id, file_name(tab.favicon.as_deref()?))))
            .collect();
        let mut shown = HashSet::new();
        for (id, name) in tabs {
            if files.contains(&name) {
                shown.insert(name);
            } else if let Some(tab) = sidebar.tab_mut(id) {
                tab.favicon = None;
            }
        }
        for name in files.difference(&shown) {
            let _ = fs::remove_file(dir.join(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::TestDir;

    #[test]
    fn names_a_file_after_the_md5_of_its_source() {
        assert_eq!(file_name(""), "d41d8cd98f00b204e9800998ecf8427e.png");
        assert!(is_file_name(&file_name("https://example.com/favicon.ico")));
        for name in [
            "../session.json",
            "d41d8cd98f00b204e9800998ecf8427e",
            "a.png",
        ] {
            assert!(!is_file_name(name), "{name}");
        }
    }

    #[test]
    fn keeps_the_files_tabs_show_and_forgets_the_icons_without_one() {
        let dir = TestDir::new("favicon-sweep");
        let mut sidebar = Sidebar::default();
        let profile = sidebar.profiles()[0].id;
        let today = sidebar.spaces()[0].today;
        let [shown, missing] = ["https://a.example/", "https://b.example/"].map(|url| {
            let id = sidebar.add_tab(today, url, url).unwrap();
            sidebar.tab_mut(id).unwrap().favicon = Some(format!("{url}favicon.ico"));
            id
        });
        let profile_dir = dir.0.join("profile");
        write(&profile_dir, "https://a.example/favicon.ico", b"png").unwrap();
        write(&profile_dir, "https://gone.example/favicon.ico", b"png").unwrap();
        sweep(&mut sidebar, |p| {
            assert_eq!(p, profile);
            profile_dir.clone()
        });
        let favicon = |sidebar: &mut Sidebar, id| sidebar.tab_mut(id).unwrap().favicon.clone();
        assert_eq!(
            favicon(&mut sidebar, shown).as_deref(),
            Some("https://a.example/favicon.ico")
        );
        assert_eq!(favicon(&mut sidebar, missing), None);
        assert!(path(&profile_dir, "https://a.example/favicon.ico").exists());
        assert!(!path(&profile_dir, "https://gone.example/favicon.ico").exists());
    }

    #[test]
    fn caches_the_icon_chromium_found_for_a_page_or_its_site() {
        let dir = TestDir::new("favicon-of-page");
        let profile_dir = dir.0.join("Profile 1");
        fs::create_dir_all(&profile_dir).unwrap();
        let db = Connection::open(profile_dir.join("Favicons")).unwrap();
        db.execute_batch(
            "CREATE TABLE icon_mapping (id INTEGER PRIMARY KEY, page_url TEXT, icon_id INTEGER);
             CREATE TABLE favicons (id INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE favicon_bitmaps (id INTEGER PRIMARY KEY, icon_id INTEGER, width INTEGER,
                image_data BLOB);
             INSERT INTO favicons VALUES (1, 'https://a.example/a.ico'), (2, 'https://a.example/b.ico');
             INSERT INTO icon_mapping (page_url, icon_id) VALUES
                ('https://a.example/one', 1), ('https://a.example/two', 2);
             INSERT INTO favicon_bitmaps (icon_id, width, image_data) VALUES
                (1, 16, x'613136'), (1, 32, x'613332'), (2, 16, x'623136');",
        )
        .unwrap();
        drop(db);
        let chromium = chromium(&profile_dir).unwrap();
        let of_page = |url| of_page(&profile_dir, &chromium, url);
        assert_eq!(
            of_page("https://a.example/two").as_deref(),
            Some("https://a.example/b.ico")
        );
        assert_eq!(
            of_page("https://a.example/").as_deref(),
            Some("https://a.example/a.ico")
        );
        assert_eq!(of_page("https://a.example.org/"), None);
        assert_eq!(of_page("https://b.example/one"), None);
        let cached = fs::read(path(&profile_dir, "https://a.example/a.ico")).unwrap();
        assert_eq!(cached, b"a32");
    }
}
