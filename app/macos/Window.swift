// What the interface puts in the main window: the blur behind its views, and the sidebar above
// them, docked at its left edge or floating above the page.

import AppKit
import Combine
import SwiftUI

/// Under the window's views, which leave it showing where they are transparent: the desktop
/// blurred through the window, tinted by the displayed Space's theme.
@MainActor
final class Backdrop {
    private let effect: NSVisualEffectView
    private var watch: AnyCancellable?

    init(content: NSView, model: AppModel) {
        effect = NSVisualEffectView(frame: content.frame)
        effect.material = .underWindowBackground
        effect.blendingMode = .behindWindow
        effect.state = .followsWindowActiveState
        effect.autoresizingMask = [.width, .height]
        let tint = NSHostingView(rootView: BackdropTint(model: model))
        tint.frame = effect.bounds
        tint.autoresizingMask = [.width, .height]
        effect.addSubview(tint)
        content.superview?.addSubview(effect, positioned: .below, relativeTo: content)
        watch = model.$state.sink { [effect] _ in
            effect.appearance = NSAppearance(named: isDark(model.space?.theme) ? .darkAqua : .aqua)
        }
    }
}

struct BackdropTint: View {
    @ObservedObject var model: AppModel

    var body: some View {
        Tint(theme: model.space?.theme).ignoresSafeArea()
    }
}

/// The sidebar, in a child window of the main one, whose own views Chromium keeps every click
/// from: docked, it shows the backdrop through, and its transparent corner the window's buttons,
/// which clicks there reach; floating above the page, it blurs the page under it.
@MainActor
final class SidebarHost {
    private let window: NSWindow
    private let panel = ChildPanel(view: NSView())
    private let container = NSView()
    private let effect = NSVisualEffectView()
    /// The toolbar's gap between its buttons, which moves the window.
    private let drag = WindowDrag()
    /// Barely there, so that clicks on the docked sidebar's transparent parts reach it, rather
    /// than the main window under it: below the toolbar, and the toolbar past the window's
    /// buttons, which keep theirs.
    private let catchers = [NSView(), NSView()]
    /// While the sidebar hides, its edge of the window shows it again on hover.
    private let edge = Tracking()
    private var watch: AnyCancellable?
    private var revealed = false
    private var rightClicks: Any?

    init(window: NSWindow, model: AppModel) {
        self.window = window
        container.wantsLayer = true
        effect.material = .sidebar
        effect.blendingMode = .behindWindow
        effect.state = .active
        effect.autoresizingMask = [.width, .height]
        let host = NSHostingView(rootView: SidebarView(model: model))
        host.autoresizingMask = [.width, .height]
        let leave = Tracking()
        leave.autoresizingMask = [.width, .height]
        container.addSubview(leave)
        for catcher in catchers {
            catcher.wantsLayer = true
            catcher.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.004).cgColor
            container.addSubview(catcher)
        }
        container.addSubview(effect)
        container.addSubview(host)
        container.addSubview(drag)
        leave.exited = { [weak self] in
            if self?.revealed == true { model.run("sidebar/conceal") }
        }
        edge.entered = { model.run("sidebar/reveal") }
        edge.isHidden = true
        edge.autoresizingMask = [.height]
        window.contentView?.addSubview(edge)
        // Right clicks never reach the views under the pointer that want them, the rows and
        // Spaces whose menus they open, only their release does: this hands it over.
        rightClicks = NSEvent.addLocalMonitorForEvents(matching: [.rightMouseUp]) { [weak self] event in
            guard let self, event.window === self.panel,
                let root = self.panel.contentView?.superview ?? self.panel.contentView,
                let catcher = root.hitTest(event.locationInWindow) as? RightClick.Catcher
            else { return event }
            catcher.action()
            return nil
        }
        watch = model.$state.sink { [container] _ in
            container.appearance = NSAppearance(
                named: isDark(model.space?.theme) ? .darkAqua : .aqua)
        }
    }

    /// Shows the sidebar at `frame`, from the window's top left corner, or hides it.
    func place(_ frame: NSRect?, floating: Bool, revealed: Bool) {
        self.revealed = revealed
        if let content = window.contentView {
            edge.frame = NSRect(x: 0, y: 0, width: 8, height: content.bounds.height)
        }
        edge.isHidden = frame != nil
        effect.isHidden = !floating
        container.layer?.cornerRadius = floating ? 10 : 0
        container.layer?.masksToBounds = floating
        panel.hasShadow = floating
        panel.place(in: window, frame)
        if container.superview != panel.contentView {
            panel.contentView?.addSubview(container)
        }
        container.frame = panel.contentView?.bounds ?? .zero
        container.autoresizingMask = [.width, .height]
        container.subviews.forEach { $0.frame = container.bounds }
        let bounds = container.bounds
        catchers[0].frame = NSRect(x: 0, y: 0, width: bounds.width, height: max(0, bounds.height - 40))
        catchers[1].frame = NSRect(x: 76, y: bounds.height - 40, width: max(0, bounds.width - 76), height: 40)
        // Past the window's buttons and the sidebar's toggle, before the navigation buttons.
        drag.frame = NSRect(x: 108, y: bounds.height - 40, width: max(0, bounds.width - 208), height: 40)
        panel.invalidateShadow()
    }
}

/// A view that says when the pointer enters or leaves it.
final class Tracking: NSView {
    var entered: () -> Void = {}
    var exited: () -> Void = {}

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: bounds, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                owner: self))
    }

    override func mouseEntered(with event: NSEvent) { entered() }
    override func mouseExited(with event: NSEvent) { exited() }
}

/// Moves the main window when dragged, like a title bar: the sidebar is a window of its own,
/// and AppKit drags a window only by its own events.
final class WindowDrag: NSView {
    /// Where the pointer and the window were as the drag began, on the screen.
    private var start: (pointer: NSPoint, origin: NSPoint)?

    override func mouseDown(with event: NSEvent) {
        guard let main = window?.parent else { return }
        if event.clickCount == 2 {
            main.performZoom(nil)
        } else {
            start = (NSEvent.mouseLocation, main.frame.origin)
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start, let main = window?.parent else { return }
        let pointer = NSEvent.mouseLocation
        main.setFrameOrigin(
            NSPoint(
                x: start.origin.x + pointer.x - start.pointer.x,
                y: start.origin.y + pointer.y - start.pointer.y))
    }

    override func mouseUp(with event: NSEvent) {
        start = nil
    }
}
