//! What websites ask to use, like the camera or the location: in words, for the prompt that asks
//! the user (shell::permission_prompt); and where Chromium keeps each site's decision, in the
//! profile's content settings.

use cef::{
    ContentSettingTypes as Setting, MediaAccessPermissionTypes as Media,
    PermissionRequestTypes as Request,
};

/// Something a site asks to use.
#[derive(Debug, PartialEq)]
pub struct Capability {
    /// What the prompt lists under "<site> wants to".
    pub asks: &'static str,
    /// A symbol of SF Symbols, beside it.
    pub symbol: &'static str,
    /// The content setting Chromium keeps a site's decision in, or none, for what is asked each
    /// time, like sharing the screen.
    pub setting: Option<Setting>,
}

const fn capability(asks: &'static str, symbol: &'static str, setting: Setting) -> Capability {
    Capability {
        asks,
        symbol,
        setting: Some(setting),
    }
}

const CAMERA: Capability = capability("Use your camera", "video", Setting::MEDIASTREAM_CAMERA);
const MICROPHONE: Capability = capability("Use your microphone", "mic", Setting::MEDIASTREAM_MIC);
const SCREEN: Capability = Capability {
    asks: "Share your screen",
    symbol: "rectangle.on.rectangle",
    setting: None,
};
const LOCAL_NETWORK: Capability = capability(
    "Connect to devices on your network",
    "network",
    Setting::LOCAL_NETWORK,
);

/// What each of Chromium's permission requests asks.
const REQUESTS: &[(Request, &Capability)] = &[
    (Request::CAMERA_STREAM, &CAMERA),
    (Request::MIC_STREAM, &MICROPHONE),
    (
        Request::CAMERA_PAN_TILT_ZOOM,
        &capability("Move your camera", "move.3d", Setting::CAMERA_PAN_TILT_ZOOM),
    ),
    (
        Request::GEOLOCATION,
        &capability("Know your location", "location", Setting::GEOLOCATION),
    ),
    (
        Request::NOTIFICATIONS,
        &capability("Send notifications", "bell", Setting::NOTIFICATIONS),
    ),
    (
        Request::CLIPBOARD,
        &capability(
            "See what you copy",
            "doc.on.clipboard",
            Setting::CLIPBOARD_READ_WRITE,
        ),
    ),
    (
        Request::MIDI_SYSEX,
        &capability(
            "Control your MIDI devices",
            "pianokeys",
            Setting::MIDI_SYSEX,
        ),
    ),
    (
        Request::STORAGE_ACCESS,
        &capability(
            "Use its cookies here",
            "puzzlepiece",
            Setting::STORAGE_ACCESS,
        ),
    ),
    (
        Request::TOP_LEVEL_STORAGE_ACCESS,
        &capability(
            "Let related sites use their cookies",
            "puzzlepiece",
            Setting::TOP_LEVEL_STORAGE_ACCESS,
        ),
    ),
    (
        Request::DISK_QUOTA,
        &Capability {
            asks: "Store more data on this Mac",
            symbol: "internaldrive",
            setting: None,
        },
    ),
    (
        Request::LOCAL_FONTS,
        &capability("Use your fonts", "textformat", Setting::LOCAL_FONTS),
    ),
    (
        Request::IDLE_DETECTION,
        &capability(
            "Know when you are using this Mac",
            "person.crop.circle.badge.clock",
            Setting::IDLE_DETECTION,
        ),
    ),
    (
        Request::WINDOW_MANAGEMENT,
        &capability(
            "Manage windows on your displays",
            "macwindow.on.rectangle",
            Setting::WINDOW_MANAGEMENT,
        ),
    ),
    (
        Request::MULTIPLE_DOWNLOADS,
        &capability(
            "Download multiple files",
            "arrow.down.circle",
            Setting::AUTOMATIC_DOWNLOADS,
        ),
    ),
    (
        Request::KEYBOARD_LOCK,
        &capability(
            "Take over your keyboard",
            "keyboard",
            Setting::KEYBOARD_LOCK,
        ),
    ),
    (
        Request::POINTER_LOCK,
        &capability("Lock your pointer", "cursorarrow", Setting::POINTER_LOCK),
    ),
    (
        Request::PROTECTED_MEDIA_IDENTIFIER,
        // Chromium keeps no such setting on macOS, where it never asks it either.
        &Capability {
            asks: "Play protected content",
            symbol: "play.rectangle",
            setting: None,
        },
    ),
    (
        Request::REGISTER_PROTOCOL_HANDLER,
        &capability("Open some links", "link", Setting::PROTOCOL_HANDLERS),
    ),
    (
        Request::AR_SESSION,
        &capability("Use augmented reality", "arkit", Setting::AR),
    ),
    (
        Request::VR_SESSION,
        &capability("Use virtual reality", "visionpro", Setting::VR),
    ),
    (
        Request::HAND_TRACKING,
        &capability("Track your hands", "hand.raised", Setting::HAND_TRACKING),
    ),
    (
        Request::CAPTURED_SURFACE_CONTROL,
        &capability(
            "Scroll and zoom what you share",
            "hand.point.up.left",
            Setting::CAPTURED_SURFACE_CONTROL,
        ),
    ),
    (
        Request::IDENTITY_PROVIDER,
        &capability(
            "Sign you in with an account",
            "person.badge.key",
            Setting::FEDERATED_IDENTITY_API,
        ),
    ),
    (
        Request::AMBIENT_LOGIN,
        &capability(
            "Sign you in automatically",
            "person.badge.key",
            Setting::FEDERATED_IDENTITY_AUTO_REAUTHN_PERMISSION,
        ),
    ),
    (
        Request::WEB_APP_INSTALLATION,
        &capability(
            "Install web apps",
            "app.badge",
            Setting::WEB_APP_INSTALLATION,
        ),
    ),
    (
        Request::FILE_SYSTEM_ACCESS,
        &capability(
            "Edit files on this Mac",
            "folder",
            Setting::FILE_SYSTEM_WRITE_GUARD,
        ),
    ),
    (Request::LOCAL_NETWORK, &LOCAL_NETWORK),
    (Request::LOCAL_NETWORK_ACCESS_DEPRECATED, &LOCAL_NETWORK),
    (
        Request::LOOPBACK_NETWORK,
        &capability(
            "Connect to apps on this Mac",
            "desktopcomputer",
            Setting::LOOPBACK_NETWORK,
        ),
    ),
    (
        Request::SENSORS,
        &capability("Use your motion sensors", "gyroscope", Setting::SENSORS),
    ),
];

/// What a permission prompt asks for, `requested` combining Chromium's request types.
pub fn prompted(requested: u32) -> Vec<&'static Capability> {
    REQUESTS
        .iter()
        .filter(|(request, _)| requested & request.get_raw() != 0)
        .map(|(_, capability)| *capability)
        .fold(Vec::new(), distinct)
}

/// What a request for media asks for, `requested` combining Chromium's media types: the camera
/// and the microphone, or the screen, with its sound or without.
pub fn media(requested: u32) -> Vec<&'static Capability> {
    [
        (Media::DEVICE_VIDEO_CAPTURE, &CAMERA),
        (Media::DEVICE_AUDIO_CAPTURE, &MICROPHONE),
        (Media::DESKTOP_VIDEO_CAPTURE, &SCREEN),
        (Media::DESKTOP_AUDIO_CAPTURE, &SCREEN),
    ]
    .into_iter()
    .filter(|(media, _)| requested & media.get_raw() != 0)
    .map(|(_, capability)| capability)
    .fold(Vec::new(), distinct)
}

/// What a site may have been allowed or blocked: everything Chromium keeps a decision for, once.
pub fn remembered() -> Vec<&'static Capability> {
    REQUESTS
        .iter()
        .map(|(_, capability)| *capability)
        .filter(|capability| capability.setting.is_some())
        .fold(Vec::new(), distinct)
}

/// Adds `capability` to `list`, unless already there.
fn distinct(
    mut list: Vec<&'static Capability>,
    capability: &'static Capability,
) -> Vec<&'static Capability> {
    if !list.iter().any(|known| std::ptr::eq(*known, capability)) {
        list.push(capability);
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asks(capabilities: Vec<&Capability>) -> Vec<&str> {
        capabilities.iter().map(|c| c.asks).collect()
    }

    #[test]
    fn words_a_prompt_in_the_order_people_expect() {
        let requested = Request::MIC_STREAM.get_raw() | Request::CAMERA_STREAM.get_raw();
        assert_eq!(
            asks(prompted(requested)),
            ["Use your camera", "Use your microphone"]
        );
        assert_eq!(
            asks(prompted(Request::GEOLOCATION.get_raw())),
            ["Know your location"]
        );
    }

    #[test]
    fn words_every_request_chromium_can_make() {
        // Each bit Chromium defines, up to its last request type.
        let last = Request::AMBIENT_LOGIN.get_raw();
        for bit in (0..32)
            .map(|shift| 1u32 << shift)
            .take_while(|bit| *bit <= last)
        {
            assert_eq!(prompted(bit).len(), 1, "request type {bit}");
        }
    }

    #[test]
    fn words_media_requests_once_each() {
        let screen =
            Media::DESKTOP_VIDEO_CAPTURE.get_raw() | Media::DESKTOP_AUDIO_CAPTURE.get_raw();
        assert_eq!(asks(media(screen)), ["Share your screen"]);
        let call = Media::DEVICE_AUDIO_CAPTURE.get_raw() | Media::DEVICE_VIDEO_CAPTURE.get_raw();
        assert_eq!(
            asks(media(call)),
            ["Use your camera", "Use your microphone"]
        );
    }

    #[test]
    fn remembers_each_setting_once() {
        let settings: Vec<_> = remembered().iter().filter_map(|c| c.setting).collect();
        let distinct: std::collections::HashSet<_> = settings.iter().collect();
        assert_eq!(settings.len(), distinct.len());
        assert!(settings.contains(&Setting::MEDIASTREAM_CAMERA));
        assert!(!remembered().contains(&&SCREEN));
    }
}
