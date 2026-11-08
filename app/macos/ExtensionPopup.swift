// An extension's popup, like Arc's: a small window of its own with a title bar to move or close
// it, under the site controls' button, or over the main window's middle for an options page
// (open_extension_popup in app/src/shell/site_controls.rs). CEF draws the extension's page in it,
// at the size the page asks for.

import AppKit

/// How far the popup stands under the site controls' button (PANEL_GAP in
/// app/src/shell/site_controls.rs).
private let buttonGap: CGFloat = 6
/// How far the popup stays from the main window's edges, as the page card does (CARD_MARGIN in
/// app/src/shell.rs).
private let windowMargin: CGFloat = 10

/// The window an extension's popup shows in: closing it closes the popup.
private final class ExtensionPopupWindow: NSWindow, NSWindowDelegate {
    let page = PageHost()
    private let closed: () -> Void

    init(closed: @escaping () -> Void) {
        self.closed = closed
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 25, height: 25),
            styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        isReleasedWhenClosed = false
        delegate = self
        contentView = page
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        closed()
        return false
    }
}

/// The extension popups open, the last shown the one in use: a closing one stays until its page's
/// view goes.
@MainActor private var extensionPopups: [ExtensionPopupWindow] = []

/// A new extension popup's view, for the window `id`, hidden until its page has a size: CEF makes
/// the page's browser in it.
@_cdecl("arbalete_ui_extension_popup_view")
public func extensionPopupView(_ id: UInt32) -> UnsafeMutableRawPointer? {
    // As an address: a pointer may not leave the main actor's closure.
    let address: UInt = MainActor.assumeIsolated {
        let popup = ExtensionPopupWindow { nativeWindows[id]?.model.run("close-extension-popup") }
        extensionPopups.append(popup)
        return UInt(bitPattern: Unmanaged.passUnretained(popup.page).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// Shows the extension popup at its page's size, `width` by `height`: under the site controls'
/// button, in the window `id`'s points from its top left corner, if it has one, else over the
/// window's middle; with the keyboard. Like Arc's, it stays inside the window: under a button
/// near the window's right edge, it ends at that edge rather than centered on the button; and
/// on the screen.
@_cdecl("arbalete_ui_extension_popup_show")
public func showExtensionPopup(
    _ id: UInt32, _ button: Bool, _ x: Double, _ y: Double, _ buttonWidth: Double,
    _ buttonHeight: Double, _ width: Double, _ height: Double
) {
    MainActor.assumeIsolated {
        guard let popup = extensionPopups.last, let parent = nativeWindows[id]?.window else {
            return
        }
        popup.setContentSize(NSSize(width: width, height: height))
        let size = popup.frame.size
        let origin: NSPoint
        if button {
            let top = parent.frame.maxY - (y + buttonHeight + buttonGap)
            let centered = parent.frame.minX + x + buttonWidth / 2 - size.width / 2
            let right = parent.frame.maxX - windowMargin - size.width
            origin = NSPoint(
                x: max(min(centered, right), parent.frame.minX + windowMargin),
                y: top - size.height)
        } else {
            origin = NSPoint(
                x: parent.frame.midX - size.width / 2,
                y: parent.frame.maxY - (parent.frame.height - size.height) / 3 - size.height)
        }
        popup.setFrameOrigin(onScreen(origin, size, parent.screen?.visibleFrame))
        popup.makeKeyAndOrderFront(nil)
    }
}

/// `origin` moved so that a window `size` large there stays within `screen`, its top first.
private func onScreen(_ origin: NSPoint, _ size: NSSize, _ screen: NSRect?) -> NSPoint {
    guard let screen else { return origin }
    return NSPoint(
        x: max(min(origin.x, screen.maxX - size.width), screen.minX),
        y: min(max(origin.y, screen.minY), screen.maxY - size.height))
}

/// Takes away the extension popup whose page's view is at `view`, its page's view with it, which
/// destroys its browser: only its address, as CEF may have destroyed it since.
@_cdecl("arbalete_ui_extension_popup_closed")
public func extensionPopupClosed(_ view: UnsafeMutableRawPointer?) {
    MainActor.assumeIsolated {
        guard let view,
            let index = extensionPopups.firstIndex(where: { popup in
                popup.page.subviews.contains { Unmanaged.passUnretained($0).toOpaque() == view }
            })
        else { return }
        let popup = extensionPopups.remove(at: index)
        popup.page.subviews.forEach { $0.removeFromSuperview() }
        popup.close()
    }
}
