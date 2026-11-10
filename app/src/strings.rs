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
    let locale = crate::system::chromium_locale();
    let english = read_pack(&english_pack()?)?;
    let local = read_pack(&locale_pack(&locale)?)?;
    Some(translations(&english, &local))
}

/// The English locale file in CEF's framework in the bundle.
#[cfg(target_os = "macos")]
fn english_pack() -> Option<PathBuf> {
    Some(resources_dir()?.join("en.lproj/locale.pak"))
}

#[cfg(target_os = "macos")]
fn locale_pack(locale: &str) -> Option<PathBuf> {
    let name = locale.replace('-', "_");
    Some(resources_dir()?.join(format!("{name}.lproj/locale.pak")))
}

/// The resources folder of CEF's framework in the bundle.
#[cfg(target_os = "macos")]
fn resources_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe
        .parent()?
        .parent()?
        .join("Frameworks/Chromium Embedded Framework.framework/Resources");
    dir.exists().then_some(dir)
}

/// The English locale file of CEF's, in its `locales` folder beside the app.
#[cfg(target_os = "linux")]
fn english_pack() -> Option<PathBuf> {
    locale_pack("en-US")
}

#[cfg(target_os = "linux")]
fn locale_pack(locale: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.with_file_name("locales").join(format!("{locale}.pak")))
}

/// Chromium's locale for a BCP 47 language tag, like "fr-FR" or "zh-Hans-CN".
pub fn chromium_locale_of(tag: &str) -> String {
    let parts: Vec<&str> = tag.split('-').collect();
    let language = parts.first().copied().unwrap_or("en");
    let region = parts
        .iter()
        .skip(1)
        .find(|part| part.len() == 2 || part.len() == 3);
    match (language, region.copied()) {
        ("en", Some("GB" | "AU" | "NZ" | "IE" | "IN" | "ZA")) => "en-GB".to_string(),
        ("en", _) => "en-US".to_string(),
        ("pt", Some("PT")) => "pt-PT".to_string(),
        ("pt", _) => "pt-BR".to_string(),
        ("es", Some("ES") | None) => "es".to_string(),
        ("es", _) => "es-419".to_string(),
        ("zh", _)
            if tag.contains("Hant") || matches!(region.copied(), Some("TW" | "HK" | "MO")) =>
        {
            "zh-TW".to_string()
        }
        ("zh", _) => "zh-CN".to_string(),
        (language, _) => language.to_string(),
    }
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

#[cfg(test)]
mod locale_tests {
    use super::chromium_locale_of;

    #[test]
    fn names_chromium_locales() {
        assert_eq!(chromium_locale_of("fr-FR"), "fr");
        assert_eq!(chromium_locale_of("en-FR"), "en-US");
        assert_eq!(chromium_locale_of("en-GB"), "en-GB");
        assert_eq!(chromium_locale_of("pt-PT"), "pt-PT");
        assert_eq!(chromium_locale_of("es-MX"), "es-419");
        assert_eq!(chromium_locale_of("zh-Hant-TW"), "zh-TW");
        assert_eq!(chromium_locale_of("de"), "de");
    }
}
