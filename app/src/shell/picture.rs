//! Picture in Picture, like Arc's: a tab left while its video plays with sound goes on playing in
//! Chromium's floating video window, which closes as the tab shows again.

use super::*;

/// Puts a tab's page just hidden in Picture in Picture, once the app is done laying out: a page
/// only hidden while a split is laid out again shows again before.
pub(super) fn page_hidden(tab: NodeId) {
    post_task(ThreadId::UI, Some(&mut EnterPicture::new(tab)));
}

/// Opens the window on the page's video playing with sound, unless the page shows one already,
/// and closes it as the page shows again. Chromium only opens it for a user's gesture: the request
/// stands for the user's, who left the tab.
const ENTER_PICTURE: &str = r#"(async () => {
    if (document.pictureInPictureElement || !document.pictureInPictureEnabled) return;
    const video = [...document.querySelectorAll("video")].find((video) =>
        !video.paused && !video.ended && !video.muted && video.volume > 0
            && video.readyState > 2 && !video.disablePictureInPicture);
    if (!video) return;
    await video.requestPictureInPicture();
    const shown = () => {
        if (document.hidden) return;
        document.removeEventListener("visibilitychange", shown);
        if (document.pictureInPictureElement === video) document.exitPictureInPicture();
    };
    document.addEventListener("visibilitychange", shown);
})().catch(() => {})"#;

/// Lets a site's videos go on in Picture in Picture as its tabs are left, or no longer, from its
/// site controls.
pub(super) fn allow_picture(site: String, on: bool) {
    SETTINGS.with_borrow_mut(|settings| {
        settings.no_picture_sites.retain(|blocked| *blocked != site);
        if !on {
            settings.no_picture_sites.push(site);
            settings.no_picture_sites.sort();
        }
    });
    save_settings();
    if let Some(url) = selected_url() {
        send_site_controls(&url);
    }
}

wrap_task! {
    struct EnterPicture {
        tab: NodeId,
    }

    impl Task {
        fn execute(&self) {
            let settings = app_settings();
            let blocked = tab_url(self.tab).is_some_and(|url| {
                settings.no_picture_sites.iter().any(|blocked| *blocked == site(&url))
            });
            if !settings.picture_in_picture || blocked {
                return;
            }
            let host = PAGES.with_borrow(|pages| {
                let live = pages.get(&self.tab).filter(|live| live.hidden_since.is_some())?;
                live.browser.host()
            });
            let Some(host) = host else { return };
            let params = json!({ "expression": ENTER_PICTURE, "userGesture": true });
            let message = json!({ "id": 0, "method": "Runtime.evaluate", "params": params });
            host.send_dev_tools_message(Some(message.to_string().as_bytes()));
        }
    }
}
