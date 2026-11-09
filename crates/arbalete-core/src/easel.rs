//! Easels: boards where the user lays out captures of pages, pictures, texts, shapes, arrows and
//! lines drawn with the pen (decision 0009).
//!
//! Each Easel is a folder of its own: `easel.json`, its objects, and `images/`, the pictures they
//! show. The board is `WIDTH` points wide and as tall as its objects reach; the interface scales it
//! to its width.

use crate::store::write_atomically;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const VERSION: u32 = 1;

/// The board's width, in points.
pub const WIDTH: f64 = 1000.0;

pub const FILE_NAME: &str = "easel.json";
const IMAGES: &str = "images";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Easel {
    /// Back to front.
    pub objects: Vec<Object>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    /// Unique in its Easel: a live capture's page is known by it.
    pub id: u64,
    /// Its box on the board, in points from its top left corner.
    pub frame: Rect,
    pub content: Content,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A point within an object's frame, as fractions of its width and height: resizing the frame
/// stretches its arrow or its line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Content {
    /// A part of a page, as it looked when captured, with the page it came from and, to show it
    /// live, where it was on the page.
    Capture {
        image: String,
        url: String,
        title: String,
        /// The page's icon, a picture of the Easel's too.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        favicon: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<PageView>,
        /// Shown live: the page itself, loaded again, rather than its picture.
        #[serde(default)]
        live: bool,
    },
    Image {
        image: String,
    },
    Text {
        text: String,
        font: Font,
        style: TextStyle,
    },
    Shape {
        shape: Shape,
        color: Color,
    },
    Arrow {
        tail: Point,
        head: Point,
        color: Color,
    },
    Pen {
        points: Vec<Point>,
        color: Color,
    },
}

/// Where a capture was on its page, in the page's CSS pixels: the size of the page's view, where
/// it was scrolled, and the part picked in the view.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageView {
    pub width: f64,
    pub height: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub part: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shape {
    Rectangle,
    Ellipse,
}

/// Arc's faces: the system's, Inter, Space Mono and EB Garamond.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Font {
    Clean,
    Hip,
    Mono,
    Fancy,
}

/// Arc's text sizes, from a title's largest to a body's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextStyle {
    Ultra,
    H0,
    H1,
    H2,
    Body,
}

/// Arc's palette, by name: the interface gives each its shade, black and white swapping in dark
/// mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Color {
    Black,
    White,
    Grey,
    Pink,
    Red,
    Yellow,
    LightGreen,
    DarkGreen,
    LightBlue,
    DarkBlue,
    Purple,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u32,
    #[serde(flatten)]
    easel: Easel,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid(serde_json::Error),
    UnsupportedVersion(u32),
    /// An object shows a picture that is not in the Easel's `images` folder.
    MissingImage(String),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl Content {
    /// The pictures it shows, by their file names in the Easel's `images` folder.
    pub fn images(&self) -> Vec<&str> {
        match self {
            Self::Capture { image, favicon, .. } => [Some(image.as_str()), favicon.as_deref()]
                .into_iter()
                .flatten()
                .collect(),
            Self::Image { image } => vec![image],
            _ => Vec::new(),
        }
    }
}

/// The folder of an Easel's pictures.
pub fn images(dir: &Path) -> PathBuf {
    dir.join(IMAGES)
}

/// Loads the Easel in `dir`; a folder without one holds an empty Easel.
pub fn load(dir: &Path) -> Result<Easel, Error> {
    let json = match fs::read(dir.join(FILE_NAME)) {
        Ok(json) => json,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Easel::default()),
        Err(error) => return Err(error.into()),
    };
    let saved: Saved = serde_json::from_slice(&json).map_err(Error::Invalid)?;
    if saved.version != VERSION {
        return Err(Error::UnsupportedVersion(saved.version));
    }
    Ok(saved.easel)
}

/// Saves `easel` in `dir`, atomically, after checking that the pictures it shows are there, then
/// deletes those no object shows any more.
pub fn save(easel: &Easel, dir: &Path) -> Result<(), Error> {
    let images = images(dir);
    let shown: Vec<&str> = easel
        .objects
        .iter()
        .flat_map(|o| o.content.images())
        .collect();
    if let Some(missing) = shown.iter().find(|name| !is_image(&images, name)) {
        return Err(Error::MissingImage(missing.to_string()));
    }
    fs::create_dir_all(dir)?;
    let saved = Saved {
        version: VERSION,
        easel: easel.clone(),
    };
    let json = serde_json::to_vec_pretty(&saved).map_err(io::Error::other)?;
    write_atomically(&dir.join(FILE_NAME), &json)?;
    let Ok(entries) = fs::read_dir(&images) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !shown.iter().any(|shown| name == **shown) {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// A plain file name in `images`, never a path out of it.
fn is_image(images: &Path, name: &str) -> bool {
    let plain = Path::new(name).file_name().is_some_and(|file| file == name);
    plain && images.join(name).is_file()
}

/// Moves the picture at `file` into the Easel in `dir`, as `name`, a plain file name.
pub fn add_image(dir: &Path, file: &Path, name: &str) -> io::Result<()> {
    if Path::new(name)
        .file_name()
        .is_none_or(|plain| plain != name)
    {
        return Err(io::Error::other("not a plain file name"));
    }
    let images = images(dir);
    fs::create_dir_all(&images)?;
    let path = images.join(name);
    if fs::rename(file, &path).is_err() {
        fs::copy(file, &path)?;
        let _ = fs::remove_file(file);
    }
    Ok(())
}

/// Where a capture lands, like Arc's, measured in its code: in the first of a grid's slots that
/// no object touches, row after row, three a row; 272 by 194.3 points, 42 from the board's sides,
/// 50 apart across and down, from 160 down.
const SLOT: (f64, f64) = (272.0, 194.3);
const SLOT_ORIGIN: (f64, f64) = (42.0, 160.0);
const SLOT_STEP: (f64, f64) = (322.0, 244.3);
const SLOTS_ACROSS: usize = 3;

/// The frame a capture `size` points wide and tall takes on a board whose objects take `frames`:
/// the first free slot, which it fills as much as it can, centered, keeping its shape.
pub fn capture_frame(frames: &[Rect], size: (f64, f64)) -> Rect {
    let overlaps = |a: &Rect, b: &Rect| {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    };
    let slot = (0..)
        .map(|n| Rect {
            x: SLOT_ORIGIN.0 + (n % SLOTS_ACROSS) as f64 * SLOT_STEP.0,
            y: SLOT_ORIGIN.1 + (n / SLOTS_ACROSS) as f64 * SLOT_STEP.1,
            width: SLOT.0,
            height: SLOT.1,
        })
        .find(|slot| !frames.iter().any(|frame| overlaps(slot, frame)))
        .expect("a free slot");
    let (width, height) = (size.0.max(1.0), size.1.max(1.0));
    let scale = (slot.width / width).min(slot.height / height);
    let (width, height) = (width * scale, height * scale);
    Rect {
        x: slot.x + (slot.width - width) / 2.0,
        y: slot.y + (slot.height - height) / 2.0,
        width,
        height,
    }
}

/// A capture of a page to add to an Easel: its picture's file, so many points wide and tall, the
/// page's address, title and icon's file, and where it was on the page.
pub struct NewCapture<'a> {
    pub file: &'a Path,
    pub size: (f64, f64),
    pub url: &'a str,
    pub title: &'a str,
    pub favicon: Option<&'a Path>,
    pub page: Option<PageView>,
}

/// Adds `capture` to the Easel in `dir`, in its first free slot (capture_frame): its id. Its
/// icon is copied, the page's keeping it.
pub fn add_capture(dir: &Path, capture: NewCapture) -> Result<u64, Error> {
    let mut easel = load(dir)?;
    let free = |stem: &str| {
        (1..)
            .map(|n| format!("{stem}-{n}.png"))
            .find(|name| !images(dir).join(name).exists())
            .expect("a free name")
    };
    let image = free("capture");
    add_image(dir, capture.file, &image)?;
    let favicon = match capture.favicon {
        Some(icon) => {
            let name = free("icon");
            fs::copy(icon, images(dir).join(&name))?;
            Some(name)
        }
        None => None,
    };
    let frames: Vec<Rect> = easel.objects.iter().map(|o| o.frame).collect();
    let id = easel.objects.iter().map(|o| o.id).max().unwrap_or(0) + 1;
    easel.objects.push(Object {
        id,
        frame: capture_frame(&frames, capture.size),
        content: Content::Capture {
            image,
            url: capture.url.into(),
            title: capture.title.into(),
            favicon,
            page: capture.page,
            live: false,
        },
    });
    save(&easel, dir)?;
    Ok(id)
}

const SCHEME: &str = "arbalete://easel/";

/// The address of the tab showing the Easel `number`.
pub fn url(number: u64) -> String {
    format!("{SCHEME}{number}")
}

/// The Easel a tab's address shows, if it is an Easel's.
pub fn number(url: &str) -> Option<u64> {
    url.strip_prefix(SCHEME)?.parse().ok()
}

/// The folder of the Easel `number`, among the Easels' folder `easels`.
pub fn dir(easels: &Path, number: u64) -> PathBuf {
    easels.join(number.to_string())
}

/// A number no Easel in `easels` has, nor any of `used`: an Easel closed is still the tab's
/// to reopen.
pub fn new_number(easels: &Path, used: impl IntoIterator<Item = u64>) -> u64 {
    let used: Vec<u64> = used.into_iter().chain(numbers(easels)).collect();
    used.into_iter().max().map_or(1, |max| max + 1)
}

fn numbers(easels: &Path) -> Vec<u64> {
    fs::read_dir(easels)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .collect()
}

/// Deletes the Easels in `easels` that none of the tab addresses `urls` shows any more.
/// Errors are left for the next launch.
pub fn sweep<'a>(easels: &Path, urls: impl IntoIterator<Item = &'a str>) {
    let kept: Vec<u64> = urls.into_iter().filter_map(number).collect();
    for number in numbers(easels) {
        if !kept.contains(&number) {
            let _ = fs::remove_dir_all(dir(easels, number));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::TestDir;

    fn object(content: Content) -> Object {
        Object {
            id: 1,
            frame: Rect {
                x: 10.0,
                y: 20.0,
                width: 300.0,
                height: 200.0,
            },
            content,
        }
    }

    fn picture(dir: &TestDir, name: &str) -> PathBuf {
        let path = dir.0.join(name);
        fs::write(&path, b"png").unwrap();
        path
    }

    #[test]
    fn saves_and_loads_every_kind_of_object() {
        let dir = TestDir::new("easel-round-trip");
        let easel_dir = dir.0.join("easel");
        let image = "capture.png".to_string();
        add_image(&easel_dir, &picture(&dir, "a.png"), &image).unwrap();
        let color = Color::LightBlue;
        let easel = Easel {
            objects: vec![
                object(Content::Capture {
                    image: image.clone(),
                    url: "https://example.com/".into(),
                    title: "Example".into(),
                    favicon: None,
                    page: Some(PageView {
                        width: 1280.0,
                        height: 800.0,
                        scroll_x: 0.0,
                        scroll_y: 640.0,
                        part: Rect {
                            x: 20.0,
                            y: 30.0,
                            width: 400.0,
                            height: 200.0,
                        },
                    }),
                    live: true,
                }),
                object(Content::Image { image }),
                object(Content::Text {
                    text: "Hello".into(),
                    font: Font::Hip,
                    style: TextStyle::H1,
                }),
                object(Content::Shape {
                    shape: Shape::Ellipse,
                    color,
                }),
                object(Content::Arrow {
                    tail: Point { x: 0.0, y: 1.0 },
                    head: Point { x: 1.0, y: 0.0 },
                    color,
                }),
                object(Content::Pen {
                    points: vec![Point { x: 0.1, y: 0.2 }, Point { x: 0.3, y: 0.4 }],
                    color,
                }),
            ],
        };
        save(&easel, &easel_dir).unwrap();
        assert_eq!(load(&easel_dir).unwrap(), easel);
    }

    #[test]
    fn a_new_folder_holds_an_empty_easel() {
        let dir = TestDir::new("easel-empty");
        assert_eq!(load(&dir.0.join("none")).unwrap(), Easel::default());
    }

    #[test]
    fn moves_pictures_in_under_plain_names() {
        let dir = TestDir::new("easel-add-image");
        let easel_dir = dir.0.join("easel");
        add_image(&easel_dir, &picture(&dir, "a.png"), "b.png").unwrap();
        assert!(images(&easel_dir).join("b.png").is_file());
        assert!(!dir.0.join("a.png").exists());
        assert!(add_image(&easel_dir, &picture(&dir, "c.png"), "../c.png").is_err());
    }

    #[test]
    fn deletes_the_pictures_no_object_shows() {
        let dir = TestDir::new("easel-unused-images");
        let easel_dir = dir.0.join("easel");
        add_image(&easel_dir, &picture(&dir, "a.png"), "kept.png").unwrap();
        add_image(&easel_dir, &picture(&dir, "b.png"), "dropped.png").unwrap();
        let easel = Easel {
            objects: vec![object(Content::Image {
                image: "kept.png".into(),
            })],
        };
        save(&easel, &easel_dir).unwrap();
        assert!(images(&easel_dir).join("kept.png").is_file());
        assert!(!images(&easel_dir).join("dropped.png").exists());
    }

    #[test]
    fn adds_a_capture_in_the_first_free_slot() {
        let dir = TestDir::new("easel-add-capture");
        let easel_dir = dir.0.join("easel");
        let url = "https://example.com/";
        let capture = |file, size, favicon| NewCapture {
            file,
            size,
            url,
            title: "A",
            favicon,
            page: None,
        };
        let icon = picture(&dir, "icon.png");
        let (a, b) = (picture(&dir, "a.png"), picture(&dir, "b.png"));
        add_capture(&easel_dir, capture(&a, (1200.0, 300.0), None)).unwrap();
        add_capture(&easel_dir, capture(&b, (200.0, 100.0), Some(&icon))).unwrap();
        let objects = load(&easel_dir).unwrap().objects;
        let frames: Vec<Rect> = objects.iter().map(|o| o.frame).collect();
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        // Like Arc's, a wide capture shrinks to its slot's width; a small one grows to it.
        let wide = 272.0 / 1200.0 * 300.0;
        assert_eq!(
            frames,
            [
                rect(42.0, 160.0 + (194.3 - wide) / 2.0, 272.0, wide),
                rect(364.0, 160.0 + (194.3 - 136.0) / 2.0, 272.0, 136.0)
            ]
        );
        assert_eq!(objects.iter().map(|o| o.id).collect::<Vec<_>>(), [1, 2]);
        for image in objects[1].content.images() {
            assert!(images(&easel_dir).join(image).is_file());
        }
        assert_eq!(objects[1].content.images().len(), 2);
        assert!(icon.is_file());
    }

    #[test]
    fn skips_the_slots_an_object_touches() {
        let rect = |x, y, width, height| Rect {
            x,
            y,
            width,
            height,
        };
        // The first slot's corner, and all of the second row's.
        let frames = [
            rect(300.0, 340.0, 10.0, 10.0),
            rect(0.0, 420.0, 1000.0, 100.0),
        ];
        let frame = capture_frame(&frames, (272.0, 194.3));
        assert_eq!((frame.x, frame.y), (364.0, 160.0));
        let full: Vec<Rect> = (0..3)
            .map(|n| rect(42.0 + n as f64 * 322.0, 160.0, 272.0, 194.3))
            .collect();
        let frame = capture_frame(&[full, frames.to_vec()].concat(), (272.0, 194.3));
        assert_eq!((frame.x, frame.y), (42.0, 160.0 + 2.0 * 244.3));
    }

    #[test]
    fn reads_an_easel_from_its_address() {
        assert_eq!(number(&url(12)), Some(12));
        assert_eq!(number("arbalete://easel/x"), None);
        assert_eq!(number("https://example.com/"), None);
    }

    #[test]
    fn numbers_a_new_easel_after_every_other() {
        let dir = TestDir::new("easel-new-number");
        assert_eq!(new_number(&dir.0, []), 1);
        dir.dir("4");
        assert_eq!(new_number(&dir.0, [2]), 5);
        assert_eq!(new_number(&dir.0, [9]), 10);
    }

    #[test]
    fn deletes_the_easels_no_tab_shows() {
        let dir = TestDir::new("easel-sweep");
        dir.write("1/easel.json", "{}");
        dir.write("2/easel.json", "{}");
        dir.dir("notes");
        sweep(&dir.0, [url(1).as_str(), "https://example.com/"]);
        assert!(dir.0.join("1").is_dir());
        assert!(!dir.0.join("2").exists());
        assert!(dir.0.join("notes").is_dir());
    }

    #[test]
    fn refuses_a_picture_out_of_its_folder() {
        let dir = TestDir::new("easel-missing-image");
        let easel_dir = dir.0.join("easel");
        picture(&dir, "outside.png");
        for image in ["../outside.png", "nothing.png"] {
            let easel = Easel {
                objects: vec![object(Content::Image {
                    image: image.into(),
                })],
            };
            assert!(matches!(
                save(&easel, &easel_dir),
                Err(Error::MissingImage(name)) if name == image
            ));
        }
        assert!(!easel_dir.join(FILE_NAME).exists());
    }

    #[test]
    fn rejects_an_unknown_version() {
        let dir = TestDir::new("easel-version");
        fs::write(dir.0.join(FILE_NAME), r#"{"version": 99, "objects": []}"#).unwrap();
        assert!(matches!(load(&dir.0), Err(Error::UnsupportedVersion(99))));
    }
}
