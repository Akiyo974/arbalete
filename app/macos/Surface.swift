// A panel above the page, like the Command Bar, which the app shows, places and hides where it
// would its web counterpart's overlay (mirror in app/src/shell.rs). The page is a window of its
// own above the main one's, so what shows over it is a child window too.

import AppKit
import SwiftUI

/// A borderless child window of the main one, which moves with it.
final class ChildPanel: NSPanel, NSWindowDelegate {
    /// Called when another window takes the keyboard from it.
    var resigned: () -> Void = {}

    init(view: NSView) {
        super.init(
            contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
            defer: true)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        becomesKeyOnlyIfNeeded = true
        isReleasedWhenClosed = false
        contentView = view
        delegate = self
    }

    override var canBecomeKey: Bool { true }

    func windowDidResignKey(_ notification: Notification) {
        DispatchQueue.main.async {
            // The web overlay it stands for, out of sight, takes the keyboard as it shows.
            if NSApp.keyWindow?.alphaValue == 0 {
                self.makeKey()
            } else {
                self.resigned()
            }
        }
    }

    /// Shows the panel at `frame`, in `parent`'s points from its content's top left corner, above
    /// its other child windows; or hides it without a frame.
    func place(in parent: NSWindow, _ frame: NSRect?) {
        guard let frame, let content = parent.contentView else {
            if isVisible {
                let key = isKeyWindow
                parent.removeChildWindow(self)
                orderOut(nil)
                if key { parent.makeKey() }
            }
            return
        }
        // In the window's coordinates, from its bottom left corner.
        let height = content.frame.height
        let rect = NSRect(
            x: frame.minX, y: height - frame.maxY, width: frame.width, height: frame.height)
        setFrame(parent.convertToScreen(rect), display: true)
        // Above the other child windows, like the page's, which CEF may add or order since.
        if parent.childWindows?.last != self {
            parent.removeChildWindow(self)
            parent.addChildWindow(self, ordered: .above)
        }
    }
}

/// A rounded surface with a blur under its content and a shadow.
@MainActor
final class Surface {
    private let panel: ChildPanel
    private let parent: NSWindow
    /// Called once the surface shows, to focus what it holds.
    var shown: () -> Void = {}
    /// Whether it takes the keyboard when it shows.
    var takesKeyboard = false
    /// Called when it shows and another window takes the keyboard from it.
    var resigned: () -> Void {
        get { panel.resigned }
        set { panel.resigned = newValue }
    }

    init<Content: View>(
        in parent: NSWindow, radius: CGFloat, material: NSVisualEffectView.Material = .popover,
        @ViewBuilder view: () -> Content
    ) {
        self.parent = parent
        let effect = NSVisualEffectView()
        effect.material = material
        effect.blendingMode = .behindWindow
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = radius
        effect.layer?.masksToBounds = true
        effect.layer?.borderWidth = 0.5
        effect.layer?.borderColor = NSColor.separatorColor.cgColor
        let host = NSHostingView(rootView: view())
        host.autoresizingMask = [.width, .height]
        effect.addSubview(host)
        panel = ChildPanel(view: effect)
        panel.hasShadow = true
    }

    var isShown: Bool { panel.isVisible }

    /// Shows the surface at `frame`, from the window's top left corner, or hides it.
    func place(_ shown: Bool, _ frame: NSRect) {
        let showing = shown && !panel.isVisible
        panel.place(in: parent, shown ? frame : nil)
        // Once its rounded views are drawn, so that the shadow follows their corners.
        DispatchQueue.main.async { self.panel.invalidateShadow() }
        if showing {
            if takesKeyboard { panel.makeKey() }
            // Once its views are made, which they are on first showing.
            DispatchQueue.main.async { self.shown() }
        }
    }
}

@MainActor var surfaces: [String: Surface] = [:]

@_cdecl("arbalete_ui_surface")
public func placeSurface(
    _ name: UnsafePointer<CChar>, _ shown: Bool, _ x: Double, _ y: Double, _ width: Double,
    _ height: Double
) {
    MainActor.assumeIsolated {
        surfaces[String(cString: name)]?.place(
            shown, NSRect(x: x, y: y, width: width, height: height))
    }
}
