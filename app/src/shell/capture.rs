//! Captures of the shown page, like Arc's: framed, a part of it, or all of it.

use super::*;

/// The tallest picture of a whole page, in its points, which Chromium can draw at once.
const FULL_PAGE_MAX: f64 = 15_000.0;

/// What a capture is for: the page framed, like Arc's Portrait Mode; a part of it to pick; or all
/// of it, scrolled down to its end. The interface knows them by these numbers.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum CaptureMode {
    Portrait = 0,
    Area = 1,
    Full = 2,
}

/// Lists the page's blocks that show, which a capture of a part picks with a click, like Arc's:
/// the elements laid out as boxes, or pictures and controls, from 12 points across, but the whole
/// page. Where the page's view is scrolled and how tall it is come along, which an Easel's live
/// capture loads the page at.
const BLOCKS_SCRIPT: &str = r#"(() => {
  const width = innerWidth, height = innerHeight, rects = [];
  const replaced = new Set(['IMG', 'SVG', 'svg', 'VIDEO', 'CANVAS', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA', 'PICTURE']);
  for (const element of document.querySelectorAll('body *')) {
    const r = element.getBoundingClientRect();
    if (r.width < 12 || r.height < 12 || r.bottom <= 0 || r.right <= 0 || r.top >= height || r.left >= width) continue;
    if (r.width >= width * 0.98 && r.height >= height * 0.98) continue;
    const style = getComputedStyle(element);
    if (style.visibility === 'hidden' || style.display === 'contents' || style.opacity === '0') continue;
    if (style.display === 'inline' && !replaced.has(element.tagName)) continue;
    rects.push([r.left, r.top, r.width, r.height]);
    if (rects.length >= 4000) break;
  }
  return { width, height, scrollX, scrollY, rects };
})()"#;

/// Where the shown page lies in the window, in points from its top left corner.
pub(super) fn page_frame() -> Option<Rect> {
    let shown = win().shown.get()?;
    let tabs = win().split.borrow().0.clone();
    if tabs.is_empty() {
        let card = card_rect()?;
        let bar = shown_toolbar_height();
        return Some(Rect {
            y: card.y + bar,
            height: card.height - bar,
            ..card
        });
    }
    // A split's page, under its bar.
    let rect = split_rects()
        .get(tabs.iter().position(|&tab| tab == shown)?)?
        .clone();
    Some(Rect {
        y: rect.y + PANE_BAR_HEIGHT,
        height: rect.height - PANE_BAR_HEIGHT,
        ..rect
    })
}

/// The capture under way: whether its part is to pick, the picture and the page's blocks once
/// they come, and what brings the page's protocol messages.
pub(super) struct Capturing {
    mode: CaptureMode,
    picture: Option<String>,
    blocks: Option<String>,
    _observer: Option<Registration>,
}

thread_local! {
    /// The scrolls of the page under a capture's veil: the last one takes the picture again.
    static CAPTURE_SCROLLS: Cell<u32> = const { Cell::new(0) };
}

/// Captures the shown page, like Arc's: the interface frames it on a color, or shows it in a veil
/// where a drag picks the part to copy, save or edit.
pub(super) fn capture(mode: CaptureMode) {
    let Some(host) = content().and_then(|browser| browser.host()) else {
        return;
    };
    let observer = host.add_dev_tools_message_observer(Some(&mut CaptureObserver::new()));
    let area = mode == CaptureMode::Area;
    CAPTURING.set(Some(Capturing {
        mode,
        picture: None,
        // Only a part is picked among them.
        blocks: (!area).then(|| "null".to_string()),
        _observer: observer,
    }));
    // The whole page: how tall it is first.
    let ask = if mode == CaptureMode::Full {
        json!({ "id": LAYOUT_ID, "method": "Page.getLayoutMetrics" })
    } else {
        json!({ "id": CAPTURE_ID, "method": "Page.captureScreenshot", "params": { "format": "png" } })
    };
    host.send_dev_tools_message(Some(ask.to_string().as_bytes()));
    if area {
        let params = json!({ "expression": BLOCKS_SCRIPT, "returnByValue": true });
        let ask = json!({ "id": BLOCKS_ID, "method": "Runtime.evaluate", "params": params });
        host.send_dev_tools_message(Some(ask.to_string().as_bytes()));
    }
}

/// Scrolls the page under a capture's veil, like Arc's, at `x` and `y` in its points, then takes
/// its picture again once the scroll stops.
pub(super) fn scroll_capture(dx: f64, dy: f64, x: f64, y: f64) {
    let Some(host) = content().and_then(|browser| browser.host()) else {
        return;
    };
    let params = json!({ "type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy });
    let ask = json!({ "id": SCROLL_ID, "method": "Input.dispatchMouseEvent", "params": params });
    host.send_dev_tools_message(Some(ask.to_string().as_bytes()));
    let scroll = CAPTURE_SCROLLS.get() + 1;
    CAPTURE_SCROLLS.set(scroll);
    post_delayed_task(ThreadId::UI, Some(&mut RetakeCapture::new(scroll)), 180);
}

wrap_task! {
    struct RetakeCapture {
        scroll: u32,
    }

    impl Task {
        fn execute(&self) {
            if CAPTURE_SCROLLS.get() == self.scroll && CAPTURING.with_borrow(Option::is_none) {
                capture(CaptureMode::Area);
            }
        }
    }
}

wrap_task! {
    struct CaptureAnswer {
        message: String,
    }

    impl Task {
        fn execute(&self) {
            capture_answered(&self.message);
        }
    }
}

/// Hands the page's picture to the interface, over the page.
fn capture_answered(message: &str) {
    let answer: serde_json::Value = serde_json::from_str(message).unwrap_or_default();
    if answer["id"] == LAYOUT_ID {
        // All of the page, as tall as Chromium draws at once.
        let size = &answer["result"]["cssContentSize"];
        let width = size["width"].as_f64().unwrap_or(0.0);
        let height = size["height"].as_f64().unwrap_or(0.0).min(FULL_PAGE_MAX);
        let clip = json!({ "x": 0, "y": 0, "width": width, "height": height, "scale": 1 });
        let params = json!({ "format": "png", "captureBeyondViewport": true, "clip": clip });
        let ask = json!({ "id": CAPTURE_ID, "method": "Page.captureScreenshot", "params": params });
        if let Some(host) = content().and_then(|browser| browser.host()) {
            host.send_dev_tools_message(Some(ask.to_string().as_bytes()));
        }
        return;
    }
    let ready = CAPTURING.with_borrow_mut(|capturing| {
        let capturing = capturing.as_mut()?;
        if answer["id"] == CAPTURE_ID {
            capturing.picture = Some(answer["result"]["data"].as_str()?.to_string());
        } else if answer["id"] == BLOCKS_ID {
            capturing.blocks = Some(answer["result"]["result"]["value"].to_string());
        }
        Some(capturing.picture.is_some() && capturing.blocks.is_some())
    });
    if ready != Some(true) {
        return;
    }
    let Some(Capturing {
        mode,
        picture: Some(picture),
        blocks: Some(blocks),
        ..
    }) = CAPTURING.take()
    else {
        return;
    };
    #[cfg(target_os = "macos")]
    if let Some(frame) = page_frame() {
        crate::native::captured(win().id, &picture, mode as u32, &frame, &blocks);
    }
}

// Brings a page's answers to a capture under way.
wrap_dev_tools_message_observer! {
    struct CaptureObserver {}

    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> i32 {
            let Some(message) = message.and_then(|message| std::str::from_utf8(message).ok()) else {
                return 0;
            };
            if !is_app_message(message) {
                return 0;
            }
            post_task(ThreadId::UI, Some(&mut CaptureAnswer::new(message.to_string())));
            1
        }
    }
}

/// Keeps a capture in the library's Media, like Arc's Save to Library: moved into the data
/// directory's Captures folder, named after the page, and listed with the downloads.
pub(super) fn save_capture_to_library(file: &Path) {
    let dir = DATA_DIR.get().expect("set").join("Captures");
    if let Err(error) = std::fs::create_dir_all(&dir) {
        return eprintln!("cannot create the Captures folder: {error}");
    }
    let (title, url) = STATE.with_borrow_mut(|state| {
        active_tab(&mut state.sidebar)
            .map(|tab| (tab.label().replace('/', "-"), tab.url.clone()))
            .unwrap_or_else(|| ("Capture".into(), String::new()))
    });
    let path = downloads::unique_path(&dir, &format!("{title}.png"), Path::exists);
    if std::fs::rename(file, &path).is_err() {
        if let Err(error) = std::fs::copy(file, &path) {
            return eprintln!("cannot keep the capture: {error}");
        }
        let _ = std::fs::remove_file(file);
    }
    DOWNLOADS.with_borrow_mut(|list| {
        let id = downloads::next_id(list);
        let mut capture = Download::saved(id, path, url, now());
        capture.profile = Some(displayed_profile());
        list.push(capture);
    });
    save_downloads();
    send_downloads();
}
