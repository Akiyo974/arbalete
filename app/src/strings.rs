//! Chromium's own strings in the language its pages speak, for the items Arbalete adds to its
//! menus, like Arc's: read from CEF's locale files, found by their English text.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The resources of a pack file, by id.
type Pack = HashMap<u16, Vec<u8>>;

/// Chromium's string with English text `english`, in the system's language, without its
/// mnemonic marks ("&"); `english` itself when CEF's files do not have it.
pub fn chromium(english: &str) -> String {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    let table = TABLE.get_or_init(|| load().unwrap_or_default());
    let text = table.get(english).map_or(english, String::as_str);
    without_mnemonics(text)
}

/// Chromium's string as `chromium` gives it, with "$1", "$2"… replaced by `values`.
pub fn chromium_with(english: &str, values: &[&str]) -> String {
    values
        .iter()
        .enumerate()
        .fold(chromium(english), |text, (index, value)| {
            text.replace(&format!("${}", index + 1), value)
        })
}

/// The system language's strings by their English text, from CEF's locale files.
fn load() -> Option<HashMap<String, String>> {
    let resources = resources_dir()?;
    #[cfg(target_os = "macos")]
    let locale = crate::mac::chromium_locale().replace('-', "_");
    #[cfg(not(target_os = "macos"))]
    let locale = "en".to_string();
    let english = read_pack(&resources.join("en.lproj/locale.pak"))?;
    let local = read_pack(&resources.join(format!("{locale}.lproj/locale.pak")))?;
    Some(translations(&english, &local))
}

/// The resources folder of CEF's framework in the bundle.
fn resources_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe
        .parent()?
        .parent()?
        .join("Frameworks/Chromium Embedded Framework.framework/Resources");
    dir.exists().then_some(dir)
}

/// Each English string's translation, the first of its ids when it has several.
fn translations(english: &Pack, local: &Pack) -> HashMap<String, String> {
    let mut ids: Vec<_> = english.keys().copied().collect();
    ids.sort_unstable();
    let mut table = HashMap::new();
    for id in ids {
        let (Ok(source), Some(Ok(text))) = (
            std::str::from_utf8(&english[&id]),
            local.get(&id).map(|text| std::str::from_utf8(text)),
        ) else {
            continue;
        };
        table
            .entry(source.to_string())
            .or_insert_with(|| text.to_string());
    }
    table
}

fn read_pack(path: &Path) -> Option<Pack> {
    parse_pack(&std::fs::read(path).ok()?)
}

/// A pack file of version 5: its header, the resources' ids and offsets, then its aliases.
fn parse_pack(bytes: &[u8]) -> Option<Pack> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    if u32_at(0)? != 5 {
        return None;
    }
    let count = usize::from(u16_at(8)?);
    let aliases = usize::from(u16_at(10)?);
    let entry = |index: usize| Some((u16_at(12 + 6 * index)?, u32_at(14 + 6 * index)? as usize));
    let body = |index: usize| {
        let (_, start) = entry(index)?;
        let (_, end) = entry(index + 1)?;
        Some(bytes.get(start..end)?.to_vec())
    };
    let mut pack = Pack::new();
    for index in 0..count {
        pack.insert(entry(index)?.0, body(index)?);
    }
    let base = 12 + 6 * (count + 1);
    for alias in 0..aliases {
        let id = u16_at(base + 4 * alias)?;
        let index = usize::from(u16_at(base + 4 * alias + 2)?);
        pack.insert(id, body(index)?);
    }
    Some(pack)
}

/// A menu item's text without its mnemonic marks: "&" goes, "&&" stays one "&".
fn without_mnemonics(text: &str) -> String {
    text.replace("&&", "\u{0}")
        .replace('&', "")
        .replace('\u{0}', "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(resources: &[(u16, &str)], aliases: &[(u16, u16)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(5u32.to_le_bytes());
        bytes.extend([1, 0, 0, 0]);
        bytes.extend((resources.len() as u16).to_le_bytes());
        bytes.extend((aliases.len() as u16).to_le_bytes());
        let mut offset = 12 + 6 * (resources.len() + 1) + 4 * aliases.len();
        for (id, text) in resources {
            bytes.extend(id.to_le_bytes());
            bytes.extend((offset as u32).to_le_bytes());
            offset += text.len();
        }
        bytes.extend(0u16.to_le_bytes());
        bytes.extend((offset as u32).to_le_bytes());
        for (id, index) in aliases {
            bytes.extend(id.to_le_bytes());
            bytes.extend(index.to_le_bytes());
        }
        for (_, text) in resources {
            bytes.extend(text.as_bytes());
        }
        bytes
    }

    #[test]
    fn translates_by_the_english_text() {
        let english = parse_pack(&pack(&[(7, "Cop&y Image"), (9, "Inspect")], &[(8, 1)])).unwrap();
        let french = parse_pack(&pack(&[(7, "Cop&ier l'image"), (9, "Inspecter")], &[])).unwrap();
        assert_eq!(english[&8], b"Inspect");
        let table = translations(&english, &french);
        assert_eq!(table["Cop&y Image"], "Cop&ier l'image");
        assert_eq!(table["Inspect"], "Inspecter");
    }

    #[test]
    fn drops_mnemonics() {
        assert_eq!(
            without_mnemonics("Open Link in New &Tab"),
            "Open Link in New Tab"
        );
        assert_eq!(without_mnemonics("Copy && Paste"), "Copy & Paste");
    }
}
