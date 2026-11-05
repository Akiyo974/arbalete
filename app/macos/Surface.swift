// A panel above the page, like the Command Bar, which the app shows, places and hides (Spot and
// mirror in app/src/shell.rs): a view of the main window (Overlay).

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

/// A rounded surface with a blur under its content and a shadow, a view over the window's card
/// and sidebar.
@MainActor
final class Surface {
    private let overlay: Overlay
    /// Casts its shadow, which its rounded content would clip.
    private let holder = NSView()
    private let host: NSView
    private let parent: NSWindow
    private let radius: CGFloat
    /// Called once the surface shows, to focus what it holds.
    var shown: () -> Void = {}
    /// Whether it takes the keyboard when it shows.
    var takesKeyboard = false
    /// Called when it shows with the keyboard and loses it: to another window, or to a click
    /// elsewhere in its own.
    var resigned: () -> Void = {}
    /// What had the keyboard before it took it, which gets it back as it hides.
    private weak var before: NSResponder?
    private var resignedKey: NSObjectProtocol?
    private var clicks: Any?

    init<Content: View>(
        in parent: NSWindow, radius: CGFloat, material: NSVisualEffectView.Material = .popover,
        @ViewBuilder view: () -> Content
    ) {
        self.parent = parent
        self.radius = radius
        let effect = NSVisualEffectView()
        effect.material = material
        // Over the page and the sidebar, in the same window: it blurs them.
        effect.blendingMode = .withinWindow
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = radius
        effect.layer?.masksToBounds = true
        effect.layer?.borderWidth = 0.5
        effect.layer?.borderColor = NSColor.separatorColor.cgColor
        effect.autoresizingMask = [.width, .height]
        let host = NSHostingView(rootView: view())
        host.autoresizingMask = [.width, .height]
        host.safeAreaRegions = []
        self.host = host
        effect.addSubview(host)
        holder.wantsLayer = true
        holder.layer?.shadowColor = NSColor.black.cgColor
        holder.layer?.shadowOpacity = 0.3
        holder.layer?.shadowRadius = 16
        holder.layer?.shadowOffset = .zero
        holder.addSubview(effect)
        overlay = Overlay(view: holder)
        resignedKey = NotificationCenter.default.addObserver(
            forName: NSWindow.didResignKeyNotification, object: parent, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, self.takesKeyboard, self.isShown else { return }
                self.resigned()
            }
        }
        clicks = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) {
            [weak self] event in
            if let self, self.takesKeyboard, self.isShown, !self.contains(event) {
                DispatchQueue.main.async { self.resigned() }
            }
            return event
        }
    }

    var isShown: Bool { overlay.isVisible }

    /// Whether `event` is a click in it.
    func contains(_ event: NSEvent) -> Bool {
        isShown && event.window === parent
            && holder.bounds.contains(holder.convert(event.locationInWindow, from: nil))
    }

    /// Takes it away.
    func close() {
        resignedKey.map(NotificationCenter.default.removeObserver)
        if let clicks { NSEvent.removeMonitor(clicks) }
        overlay.close()
    }

    /// Shows the surface at `frame`, from the window's top left corner, or hides it.
    func place(_ shown: Bool, _ frame: NSRect) {
        let showing = shown && !isShown
        if showing { before = parent.firstResponder }
        let had = (parent.firstResponder as? NSView)?.isDescendant(of: holder) == true
        overlay.place(in: parent, shown ? frame : nil)
        holder.layer?.shadowPath = CGPath(
            roundedRect: holder.bounds, cornerWidth: radius, cornerHeight: radius, transform: nil)
        // Hiding, it gives the keyboard back to what had it.
        if !shown, had {
            parent.makeFirstResponder((before as? NSView).flatMap { $0.window === parent ? $0 : nil })
        }
        guard showing else { return }
        if takesKeyboard { parent.makeKey() }
        // Once its views are made, which they are on first showing.
        DispatchQueue.main.async { self.focus() }
    }

    /// Focuses what it holds, or itself, if it takes the keyboard.
    private func focus() {
        shown()
        guard takesKeyboard, (parent.firstResponder as? NSView)?.isDescendant(of: holder) != true
        else { return }
        parent.makeFirstResponder(host)
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
