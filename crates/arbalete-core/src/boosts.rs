//! Boosts: the user's own changes to how a site looks or behaves, like Arc's (decision 0010).
//!
//! A profile's Boosts are kept in a file of their own, and the pages get them as scripts made
//! here: one that applies the stylesheet of each site's active Boosts as soon as a document
//! starts, and one per Boost with JavaScript, run once the document is ready.

use crate::store::write_atomically;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path};

/// The Boosts' file name in a profile's directory.
pub const FILE_NAME: &str = "boosts.json";

pub type BoostId = u64;

/// A Boost's text case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextCase {
    Capitalize,
    Uppercase,
    Lowercase,
}

/// A Boost's text size, around the site's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextSize {
    Small,
    Medium,
    Large,
    Larger,
}

impl TextSize {
    fn zoom(self) -> f64 {
        match self {
            Self::Small => 0.9,
            Self::Medium => 1.1,
            Self::Large => 1.25,
            Self::Larger => 1.5,
        }
    }
}

/// A Boost's colors, from its color square, like Arc's: the page is toned from a dark color to a
/// light one, the accent's `hue` with as much color as `saturation` (0 to 1) gives it, keeping
/// `original` (0 to 1) of its own colors; `invert` turns its lightness over, light pages dark.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Colors {
    pub hue: f64,
    pub saturation: f64,
    #[serde(default)]
    pub invert: bool,
    #[serde(default = "one")]
    pub contrast: f64,
    #[serde(default = "one")]
    pub brightness: f64,
    #[serde(default = "original")]
    pub original: f64,
}

fn one() -> f64 {
    1.0
}

fn original() -> f64 {
    0.3
}

/// How a page is toned: its darkest and lightest colors, as red, green and blue from 0 to 1, and
/// how much of its own colors it keeps.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Tone {
    pub dark: [f64; 3],
    pub light: [f64; 3],
    pub original: f64,
}

impl Colors {
    /// The tone: the text takes the accent's hue, darkly; the background its complement, softly.
    pub fn tone(self) -> Option<Tone> {
        let amount = self.saturation.clamp(0.0, 1.0);
        if amount == 0.0 {
            return None;
        }
        let (text, background) = (self.hue, self.hue + 180.0);
        let (dark, light) = if self.invert {
            (
                hsl(background, 0.35 * amount, 0.13),
                hsl(text, 0.4 * amount, 0.9),
            )
        } else {
            (
                hsl(text, 0.7 * amount, 0.14),
                hsl(background, 0.25 + 0.45 * amount, 0.86 - 0.06 * amount),
            )
        };
        Some(Tone {
            dark,
            light,
            original: self.original.clamp(0.0, 1.0),
        })
    }
}

/// A color from its hue in degrees, and its saturation and lightness from 0 to 1.
fn hsl(hue: f64, saturation: f64, lightness: f64) -> [f64; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let hue = hue.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    [r + m, g + m, b + m].map(|c| (c * 1000.0).round() / 1000.0)
}

/// A Boost: what changes on its site while it is active.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Boost {
    pub id: BoostId,
    /// The host it applies to, without a leading `www.` (`site`).
    pub site: String,
    pub title: String,
    pub active: bool,
    /// The selectors of the elements it hides.
    #[serde(default)]
    pub zaps: Vec<String>,
    #[serde(default)]
    pub colors: Option<Colors>,
    /// A font family for the page's text.
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub text_case: Option<TextCase>,
    #[serde(default)]
    pub text_size: Option<TextSize>,
    #[serde(default)]
    pub css: String,
    #[serde(default)]
    pub js: String,
}

/// The attribute the zap picker gives the elements of a zap it shows again (app/src/shell/zap.js).
const PREVIEW: &str = "data-arbalete-unzap";

/// The elements a Boost's font and text case leave alone: those drawing icons with a font.
const TEXT: &str = ":root :not(i, [class*=\"icon\" i], [class*=\"material-symbols\"])";

impl Boost {
    /// The stylesheet that makes its changes, its own CSS last.
    pub fn stylesheet(&self) -> String {
        let mut rules: Vec<String> = self
            .zaps
            .iter()
            // The zap picker shows a zapped element again, to restore it, by marking it.
            .map(|zap| format!(":is({zap}):not([{PREVIEW}]) {{ display: none !important; }}"))
            .collect();
        if let Some(colors) = self.colors {
            rules.extend(color_rules(colors));
        }
        if let Some(font) = &self.font {
            let font = font.replace(['"', '\\', '\n'], "");
            rules.push(format!("{TEXT} {{ font-family: \"{font}\" !important; }}"));
        }
        if let Some(case) = self.text_case {
            let case = match case {
                TextCase::Capitalize => "capitalize",
                TextCase::Uppercase => "uppercase",
                TextCase::Lowercase => "lowercase",
            };
            rules.push(format!("{TEXT} {{ text-transform: {case} !important; }}"));
        }
        if let Some(size) = self.text_size {
            rules.push(format!(":root {{ zoom: {} !important; }}", size.zoom()));
        }
        if !self.css.trim().is_empty() {
            rules.push(self.css.clone());
        }
        rules.join("\n")
    }

    /// Changes nothing.
    pub fn is_empty(&self) -> bool {
        self.stylesheet().is_empty() && self.js.trim().is_empty()
    }
}

/// The colors' rules: the page through the tone's filter (`RESTYLE` adds it to the document), or,
/// only inverted, turned over with its pictures turned back; then its contrast and brightness.
fn color_rules(colors: Colors) -> Vec<String> {
    let mut filter = Vec::new();
    let mut rules = Vec::new();
    if colors.tone().is_some() {
        filter.push("url(#arbalete-boost-colors)".to_string());
    } else if colors.invert {
        filter.push("invert(1) hue-rotate(180deg)".to_string());
        rules.push(
            "img, picture, video, canvas, iframe, embed, object { filter: invert(1) hue-rotate(180deg); }"
                .to_string(),
        );
    }
    if colors.contrast != 1.0 {
        filter.push(format!("contrast({})", colors.contrast.max(0.0)));
    }
    if colors.brightness != 1.0 {
        filter.push(format!("brightness({})", colors.brightness.max(0.0)));
    }
    if filter.is_empty() {
        return Vec::new();
    }
    rules.insert(
        0,
        format!(":root {{ filter: {} !important; }}", filter.join(" ")),
    );
    // The canvas is not filtered: a page without a background of its own gets one, which is.
    rules.push(":where(:root) { background-color: white; }".to_string());
    rules
}

/// The site of a page at `url`, which Boosts apply to: its host without a leading `www.`, for
/// `http` and `https` pages only.
pub fn site(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if !["http", "https"].contains(&scheme.to_lowercase().as_str()) {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = authority.split(':').next()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    (!host.is_empty()).then(|| host.to_string())
}

/// A profile's Boosts, in the order they were made.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Boosts {
    boosts: Vec<Boost>,
}

impl Boosts {
    pub fn list(&self) -> &[Boost] {
        &self.boosts
    }

    pub fn get(&self, id: BoostId) -> Option<&Boost> {
        self.boosts.iter().find(|boost| boost.id == id)
    }

    /// A site's Boosts.
    pub fn for_site<'a>(&'a self, site: &'a str) -> impl Iterator<Item = &'a Boost> {
        self.boosts.iter().filter(move |boost| boost.site == site)
    }

    /// Makes a Boost for `site`, which changes nothing yet, named after it: the site's active one.
    pub fn create(&mut self, site: &str) -> BoostId {
        let id = self.boosts.iter().map(|boost| boost.id).max().unwrap_or(0) + 1;
        let count = self.for_site(site).count();
        let title = match count {
            0 => site.to_string(),
            _ => format!("{site} {}", count + 1),
        };
        self.boosts.push(Boost {
            id,
            site: site.to_string(),
            title,
            active: true,
            zaps: Vec::new(),
            colors: None,
            font: None,
            text_case: None,
            text_size: None,
            css: String::new(),
            js: String::new(),
        });
        self.set_active(id, true);
        id
    }

    /// Replaces the Boost of the same id, if it is still there.
    pub fn update(&mut self, boost: Boost) {
        if let Some(kept) = self.boosts.iter_mut().find(|kept| kept.id == boost.id) {
            *kept = boost;
        }
    }

    /// Turns a Boost on or off; on, like Arc's, it turns off the other Boosts of its site.
    pub fn set_active(&mut self, id: BoostId, active: bool) {
        let Some(site) = self.get(id).map(|boost| boost.site.clone()) else {
            return;
        };
        for boost in self.boosts.iter_mut().filter(|boost| boost.site == site) {
            if boost.id == id {
                boost.active = active;
            } else if active {
                boost.active = false;
            }
        }
    }

    pub fn delete(&mut self, id: BoostId) {
        self.boosts.retain(|boost| boost.id != id);
    }

    /// What a site's active Boosts give its pages: their stylesheet, and the tone of the last
    /// one with colors.
    fn styling(&self, site: &str) -> (String, Option<Tone>) {
        let active: Vec<&Boost> = self.for_site(site).filter(|boost| boost.active).collect();
        let stylesheet: Vec<String> = active
            .iter()
            .map(|boost| boost.stylesheet())
            .filter(|stylesheet| !stylesheet.is_empty())
            .collect();
        let tone = active.iter().rev().find_map(|boost| boost.colors?.tone());
        (stylesheet.join("\n"), tone)
    }

    /// The script each new document runs first: it applies its site's active Boosts' stylesheet at
    /// once, as a constructed stylesheet, which needs no element and which the page's Content
    /// Security Policy does not block, and their tone's filter. `Boosts::restyle` replaces them.
    pub fn stylesheet_script(&self) -> String {
        let mut sites = BTreeMap::new();
        for boost in self.boosts.iter().filter(|boost| boost.active) {
            let (css, tone) = self.styling(&boost.site);
            if !css.is_empty() {
                sites.insert(boost.site.as_str(), serde_json::json!([css, tone]));
            }
        }
        let sites = serde_json::to_string(&sites).expect("values");
        format!("(() => {{ const boost = {sites}[{SITE}]; if (boost) ({RESTYLE})(...boost); }})();")
    }

    /// The script that replaces, in an open page of `site`, its Boosts' stylesheet and tone with
    /// their current ones.
    pub fn restyle(&self, site: &str) -> String {
        let (css, tone) = self.styling(site);
        let arguments = serde_json::to_string(&(css, tone)).expect("values");
        format!("({RESTYLE})(...{arguments});")
    }

    /// The scripts of the active Boosts with JavaScript, one each so that a Boost's mistake stops
    /// only its own: each runs its code in its site's documents once they are ready.
    pub fn scripts(&self) -> Vec<String> {
        self.boosts
            .iter()
            .filter(|boost| boost.active && !boost.js.trim().is_empty())
            .map(|boost| {
                let site = serde_json::to_string(&boost.site).expect("a string");
                format!(
                    "if ({SITE} === {site}) {{ const boost = () => {{\n{}\n}}; \
                     if (document.readyState === 'loading') \
                     document.addEventListener('DOMContentLoaded', boost, {{ once: true }}); \
                     else boost(); }}",
                    boost.js
                )
            })
            .collect()
    }
}

/// The document's site, in its scripts, as `site` makes it.
const SITE: &str =
    "(/^https?:$/.test(location.protocol) ? location.hostname.replace(/^www\\./, '') : '')";

/// Puts a stylesheet's text in the document's Boosts' constructed stylesheet, and a tone in their
/// filter, made the first time. The filter is an SVG element of the document, as CSS reaches no
/// other, built without markup, which a page's Trusted Types would refuse.
const RESTYLE: &str = "(css, tone) => { const key = Symbol.for('arbalete.boosts'); \
     let boost = window[key]; \
     if (!boost) { const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); \
     svg.setAttribute('aria-hidden', 'true'); \
     svg.setAttribute('style', 'position: absolute; width: 0; height: 0; overflow: hidden'); \
     boost = { sheet: new CSSStyleSheet(), svg }; \
     Object.defineProperty(window, key, { value: boost }); \
     document.adoptedStyleSheets = [...document.adoptedStyleSheets, boost.sheet]; } \
     boost.sheet.replaceSync(css); \
     boost.svg.replaceChildren(); \
     if (!tone) return boost.svg.remove(); \
     const make = (name, attributes, children = []) => { \
     const element = document.createElementNS('http://www.w3.org/2000/svg', name); \
     for (const [key, value] of Object.entries(attributes)) element.setAttribute(key, value); \
     element.append(...children); return element; }; \
     const table = (name, channel) => make(name, { type: 'table', tableValues: `${tone.dark[channel]} ${tone.light[channel]}` }); \
     boost.svg.append(make('filter', { id: 'arbalete-boost-colors', 'color-interpolation-filters': 'sRGB' }, [ \
     make('feColorMatrix', { type: 'saturate', values: '0' }), \
     make('feComponentTransfer', { result: 'tone' }, [table('feFuncR', 0), table('feFuncG', 1), table('feFuncB', 2)]), \
     make('feComposite', { in: 'tone', in2: 'SourceGraphic', operator: 'arithmetic', k1: 0, k2: 1 - tone.original, k3: tone.original, k4: 0 }), \
     ])); \
     const place = () => boost.svg.isConnected || document.documentElement.append(boost.svg); \
     if (document.documentElement) place(); \
     else new MutationObserver((_, observer) => { if (document.documentElement) { observer.disconnect(); place(); } }).observe(document, { childList: true }); }";

/// Reads a profile's Boosts, none when it has no file yet.
pub fn load(path: &Path) -> io::Result<Boosts> {
    match fs::read(path) {
        Ok(json) => serde_json::from_slice(&json).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Boosts::default()),
        Err(error) => Err(error),
    }
}

/// Writes a profile's Boosts atomically.
pub fn save(boosts: &Boosts, path: &Path) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(boosts).map_err(io::Error::other)?;
    write_atomically(path, &json)
}

/// The Boost the editor shows, with its states before and after the user's changes, for undo
/// and redo.
#[derive(Clone, Debug, PartialEq)]
pub struct Editing {
    pub boost: Boost,
    undo: Vec<Boost>,
    redo: Vec<Boost>,
    /// The last change was part of a gesture, like a drag on the color wheel, which undoes as one.
    gesture: bool,
}

impl Editing {
    pub fn new(boost: Boost) -> Self {
        Self {
            boost,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: false,
        }
    }

    /// Takes the user's change; one of a gesture's changes after the first replaces the last.
    pub fn change(&mut self, boost: Boost, gesture: bool) {
        if boost == self.boost {
            return;
        }
        if !(gesture && self.gesture) {
            self.undo.push(self.boost.clone());
            self.redo.clear();
        }
        self.gesture = gesture;
        self.boost = boost;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(before) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.boost, before));
            self.gesture = false;
        }
    }

    pub fn redo(&mut self) {
        if let Some(after) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.boost, after));
            self.gesture = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boost(boosts: &mut Boosts, site: &str) -> Boost {
        let id = boosts.create(site);
        boosts.get(id).unwrap().clone()
    }

    #[test]
    fn a_site_is_the_host_without_www_for_web_pages_only() {
        assert_eq!(
            site("https://www.Example.com:8443/a?b#c").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            site("http://user@news.example.com/").as_deref(),
            Some("news.example.com")
        );
        assert_eq!(site("arbalete://settings/"), None);
        assert_eq!(site("about:blank"), None);
        assert_eq!(site("file:///tmp/a.html"), None);
    }

    #[test]
    fn new_boosts_are_active_empty_and_named_after_their_site() {
        let mut boosts = Boosts::default();
        let first = boost(&mut boosts, "example.com");
        let second = boost(&mut boosts, "example.com");
        assert!(first.active && first.is_empty());
        assert_eq!(first.title, "example.com");
        assert_eq!(second.title, "example.com 2");
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn a_site_has_one_active_boost() {
        let mut boosts = Boosts::default();
        let first = boost(&mut boosts, "example.com");
        let other = boost(&mut boosts, "other.com");
        let second = boost(&mut boosts, "example.com");
        let active = |boosts: &Boosts, id| boosts.get(id).unwrap().active;
        assert!(
            !active(&boosts, first.id) && active(&boosts, second.id) && active(&boosts, other.id)
        );
        boosts.set_active(first.id, true);
        assert!(
            active(&boosts, first.id) && !active(&boosts, second.id) && active(&boosts, other.id)
        );
        boosts.set_active(first.id, false);
        assert!(!active(&boosts, first.id) && !active(&boosts, second.id));
    }

    #[test]
    fn zaps_hide_each_selector_on_its_own() {
        let mut boosts = Boosts::default();
        let mut boost = boost(&mut boosts, "example.com");
        boost.zaps = vec![".ad".to_string(), "#banner > p".to_string()];
        assert_eq!(
            boost.stylesheet(),
            ":is(.ad):not([data-arbalete-unzap]) { display: none !important; }\n\
             :is(#banner > p):not([data-arbalete-unzap]) { display: none !important; }"
        );
    }

    #[test]
    fn the_font_and_case_leave_icon_fonts_alone() {
        let mut boosts = Boosts::default();
        let mut boost = boost(&mut boosts, "example.com");
        boost.font = Some("Avenir \"Next\"".to_string());
        boost.text_case = Some(TextCase::Uppercase);
        let stylesheet = boost.stylesheet();
        assert!(stylesheet.contains(&format!(
            "{TEXT} {{ font-family: \"Avenir Next\" !important; }}"
        )));
        assert!(stylesheet.contains("text-transform: uppercase !important"));
        assert!(TEXT.contains(":not(i,"));
    }

    fn colors(hue: f64, saturation: f64, invert: bool) -> Colors {
        Colors {
            hue,
            saturation,
            invert,
            contrast: 1.0,
            brightness: 1.0,
            original: 0.3,
        }
    }

    #[test]
    fn colors_tone_the_page_dark_text_on_a_soft_complement() {
        let tone = colors(0.0, 1.0, false).tone().unwrap();
        // Red text, dark; a cyan background, light.
        assert!(tone.dark[0] > tone.dark[1] && tone.dark.iter().all(|c| *c < 0.3));
        assert!(tone.light[1] > tone.light[0] && tone.light.iter().all(|c| *c > 0.6));
        let inverted = colors(0.0, 1.0, true).tone().unwrap();
        assert!(inverted.dark[2] > inverted.dark[0] && inverted.light.iter().all(|c| *c > 0.8));
        assert_eq!(colors(120.0, 0.0, false).tone(), None);
        assert_eq!(hsl(240.0, 1.0, 0.5), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn colors_filter_the_page_and_invert_alone_turns_pictures_back() {
        let mut boosts = Boosts::default();
        let mut boost = boost(&mut boosts, "example.com");
        boost.colors = Some(Colors {
            contrast: 1.2,
            ..colors(60.0, 0.5, false)
        });
        let stylesheet = boost.stylesheet();
        assert!(stylesheet.starts_with(
            ":root { filter: url(#arbalete-boost-colors) contrast(1.2) !important; }"
        ));
        assert!(stylesheet.contains(":where(:root) { background-color: white; }"));
        boost.colors = Some(colors(0.0, 0.0, true));
        let stylesheet = boost.stylesheet();
        assert!(
            stylesheet.starts_with(":root { filter: invert(1) hue-rotate(180deg) !important; }")
        );
        assert!(stylesheet.contains("img, picture, video"));
        boost.colors = Some(colors(0.0, 0.0, false));
        assert!(boost.is_empty());
    }

    #[test]
    fn a_site_takes_the_tone_of_its_last_boost_with_colors() {
        let mut boosts = Boosts::default();
        let mut first = boost(&mut boosts, "example.com");
        first.colors = Some(colors(0.0, 1.0, false));
        boosts.update(first);
        let mut second = boost(&mut boosts, "example.com");
        second.colors = Some(colors(200.0, 1.0, false));
        boosts.update(second.clone());
        let tone = colors(200.0, 1.0, false).tone().unwrap();
        let expected = serde_json::to_string(&tone).unwrap();
        assert!(boosts.restyle("example.com").contains(&expected));
        assert!(boosts.stylesheet_script().contains(&expected));
        boosts.set_active(second.id, false);
        assert!(!boosts.restyle("example.com").contains(&expected));
    }

    #[test]
    fn the_own_css_comes_last() {
        let mut boosts = Boosts::default();
        let mut boost = boost(&mut boosts, "example.com");
        boost.css = "body { margin: 0 }".to_string();
        boost.text_size = Some(TextSize::Large);
        assert_eq!(
            boost.stylesheet(),
            ":root { zoom: 1.25 !important; }\nbody { margin: 0 }"
        );
    }

    #[test]
    fn only_active_boosts_reach_the_pages() {
        let mut boosts = Boosts::default();
        let mut on = boost(&mut boosts, "example.com");
        on.css = "a { color: red }".to_string();
        on.js = "console.log('on')".to_string();
        boosts.update(on.clone());
        let mut off = boost(&mut boosts, "example.com");
        off.css = "a { color: blue }".to_string();
        off.js = "console.log('off')".to_string();
        boosts.update(off.clone());
        boosts.set_active(on.id, true);
        let script = boosts.stylesheet_script();
        assert!(script.contains(r#"{"example.com":["a { color: red }",null]}"#));
        assert!(!script.contains("blue"));
        assert_eq!(boosts.scripts().len(), 1);
        assert!(boosts.scripts()[0].contains("console.log('on')"));
        assert!(
            boosts
                .restyle("example.com")
                .contains(r#"(...["a { color: red }",null])"#)
        );
        boosts.delete(on.id);
        assert!(boosts.scripts().is_empty());
        assert!(
            boosts
                .restyle("example.com")
                .ends_with(r#"(...["",null]);"#)
        );
    }

    #[test]
    fn scripts_run_on_their_site_once_the_document_is_ready() {
        let mut boosts = Boosts::default();
        let mut boost = boost(&mut boosts, "example.com");
        boost.js = "document.title = 'Boosted'".to_string();
        boosts.update(boost);
        let script = &boosts.scripts()[0];
        assert!(script.starts_with(&format!("if ({SITE} === \"example.com\")")));
        assert!(script.contains("DOMContentLoaded"));
    }

    #[test]
    fn saves_and_loads_the_boosts() {
        let dir = std::env::temp_dir().join(format!("arbalete-boosts-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        assert_eq!(load(&path).unwrap(), Boosts::default());
        let mut boosts = Boosts::default();
        let mut kept = boost(&mut boosts, "example.com");
        kept.colors = Some(colors(90.0, 1.0, true));
        kept.text_size = Some(TextSize::Small);
        boosts.update(kept);
        save(&boosts, &path).unwrap();
        assert_eq!(load(&path).unwrap(), boosts);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn undoes_and_redoes_changes_a_gesture_as_one() {
        let mut boosts = Boosts::default();
        let start = boost(&mut boosts, "example.com");
        let mut editing = Editing::new(start.clone());
        let with_hue = |hue| Boost {
            colors: Some(colors(hue, 1.0, false)),
            ..start.clone()
        };
        editing.change(with_hue(10.0), true);
        editing.change(with_hue(20.0), true);
        editing.change(with_hue(30.0), false);
        assert!(editing.can_undo() && !editing.can_redo());
        editing.undo();
        assert_eq!(editing.boost, with_hue(20.0));
        editing.undo();
        assert_eq!(editing.boost, start);
        assert!(!editing.can_undo());
        editing.redo();
        assert_eq!(editing.boost, with_hue(20.0));
        editing.change(with_hue(40.0), false);
        assert!(!editing.can_redo());
    }
}
