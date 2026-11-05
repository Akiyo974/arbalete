// A panel above the page, like the Command Bar, which the app shows, places and hides (Spot and
// mirror in app/src/shell.rs). One that takes the keyboard is a child window of the main one,
// whose own keyboard CEF keeps for its views; the others are views of the main window (Overlay).

import AppKit
import SwiftUI

/// A borderless child window of the main one, which moves with it.
final class ChildPanel: NSPanel, NSWindowDelegate {
    /// Called when another window takes the keyboard from it.
    var resigned: () -> Void = {}
    /// Where it shows, in its parent's points from its content's top left corner.
    private var placed: NSRect?
    /// Places it again as its parent changes size, like into full screen: a child window keeps its
    /// place from the parent's bottom left corner, and the app may lay the window out before it
    /// takes its last size.
    private var resized: NSObjectProtocol?

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

    /// A click in it brings the app forward with its window, as a click in the window would: the
    /// panel itself does not activate the app.
    override func sendEvent(_ event: NSEvent) {
        if [.leftMouseDown, .rightMouseDown, .otherMouseDown].contains(event.type), !NSApp.isActive {
            NSApp.activate()
            parent?.makeKeyAndOrderFront(nil)
        }
        super.sendEvent(event)
    }

    func windowDidResignKey(_ notification: Notification) {
        DispatchQueue.main.async { self.resigned() }
    }

    /// Shows the panel at `frame`, in `parent`'s points from its content's top left corner, above
    /// its other child windows unless `raise` is off; or hides it without a frame. Raising it
    /// takes it out of the window a moment, which ends a drag of it.
    func place(in parent: NSWindow, _ frame: NSRect?, raise: Bool = true) {
        guard let frame, let content = parent.contentView else {
            if isVisible {
                let key = isKeyWindow
                parent.removeChildWindow(self)
                orderOut(nil)
                if key { parent.makeKey() }
            }
            return
        }
        placed = frame
        if resized == nil || self.parent !== parent {
            resized.map(NotificationCenter.default.removeObserver)
            resized = NotificationCenter.default.addObserver(
                forName: NSWindow.didResizeNotification, object: parent, queue: .main
            ) { [weak self, weak parent] _ in
                MainActor.assumeIsolated {
                    guard let self, let parent, self.isVisible, self.parent === parent else { return }
                    self.follow(parent)
                }
            }
        }
        follow(parent)
        // Above the other child windows, like the page's, which CEF may add or order since.
        if self.parent !== parent || (raise && parent.childWindows?.last != self) {
            parent.removeChildWindow(self)
            parent.addChildWindow(self, ordered: .above)
        }
    }

    /// Takes its place in `parent` as it stands now.
    private func follow(_ parent: NSWindow) {
        guard let frame = placed, let content = parent.contentView else { return }
        // In the window's coordinates, from its bottom left corner.
        let height = content.frame.height
        let rect = NSRect(
            x: frame.minX, y: height - frame.maxY, width: frame.width, height: frame.height)
        setFrame(parent.convertToScreen(rect), display: true)
    }
}

/// The view over each window's card and sidebar, by window, which the views standing in for
/// its child windows show in (PagePlacer.overlay).
@MainActor var overlays: [ObjectIdentifier: () -> NSView?] = [:]

/// A view over the window's card and sidebar, where a child window above them would be, placed
/// from the window's top left corner, as the app counts.
@MainActor
final class Overlay {
    let view: NSView

    init(view: NSView) {
        self.view = view
        view.isHidden = true
    }

    var isVisible: Bool { view.superview != nil && !view.isHidden }

    /// Shows it at `frame`, in `parent`'s points from its top left corner, above the others there
    /// unless `raise` is off; or hides it without a frame. Raising it takes it out of the window a
    /// moment, which ends a drag in it.
    func place(in parent: NSWindow, _ frame: NSRect?, raise: Bool = true) {
        guard let frame, let over = overlays[ObjectIdentifier(parent)]?() else {
            view.isHidden = true
            return
        }
        if view.superview !== over || (raise && over.subviews.last !== view) {
            over.addSubview(view, positioned: .above, relativeTo: nil)
        }
        view.frame = frame
        view.isHidden = false
    }

    /// Gives it the keyboard.
    func makeKey() { view.window?.makeFirstResponder(view) }

    func close() { view.removeFromSuperview() }
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

    /// Its window, which clicks in it land in.
    var window: NSWindow { panel }

    /// Closes it, its window closing.
    func close() { panel.close() }

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

@_cdecl("arbalete_ui_surface")
public func placeSurface(
    _ id: UInt32, _ name: UnsafePointer<CChar>, _ shown: Bool, _ x: Double, _ y: Double,
    _ width: Double, _ height: Double
) {
    MainActor.assumeIsolated {
        nativeWindows[id]?.surfaces[String(cString: name)]?.place(
            shown, NSRect(x: x, y: y, width: width, height: height))
    }
}
