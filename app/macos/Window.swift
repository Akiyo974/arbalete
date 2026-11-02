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
    private var activity: [NSObjectProtocol] = []

    init(content: NSView, model: AppModel) {
        effect = NSVisualEffectView(frame: content.frame)
        // Like Arc's: a dark or light blur as the theme's appearance is, behind the window.
        effect.blendingMode = .behindWindow
        // Like Arc's, the blur falls flat while another app is in front: by the app's own state,
        // which its panels taking the keyboard do not muddle.
        effect.state = NSApp.isActive ? .active : .inactive
        effect.isEmphasized = true
        effect.autoresizingMask = [.width, .height]
        let tint = NSHostingView(rootView: BackdropTint(model: model))
        tint.frame = effect.bounds
        tint.autoresizingMask = [.width, .height]
        effect.addSubview(tint)
        content.superview?.addSubview(effect, positioned: .below, relativeTo: content)
        activity = [NSApplication.didBecomeActiveNotification, NSApplication.didResignActiveNotification]
            .map {
                NotificationCenter.default.addObserver(forName: $0, object: nil, queue: .main) {
                    [effect] _ in
                    MainActor.assumeIsolated {
                        effect.state = NSApp.isActive ? .active : .inactive
                        model.active = NSApp.isActive
                    }
                }
            }
        // Like Arc's, a theme's colors lie over a light blur, whatever the appearance; without
        // colors, the blur is the system's appearance's.
        watch = model.$state.sink { [effect] _ in
            let colored = !(model.space?.theme.colors.isEmpty ?? true)
            effect.material = colored || !systemDark() ? .light : .dark
        }
    }
}

/// The card's corners (CARD_RADIUS in app/src/mac.rs).
private let cardRadius: CGFloat = 6

struct BackdropTint: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let theme = model.space?.theme
        let colored = !(theme?.colors.isEmpty ?? true)
        ZStack(alignment: .topLeading) {
            Tint(theme: theme, active: model.active)
                // Like Arc's, a theme does not paint the card's room: an empty card shows the
                // blur alone.
                .mask {
                    ZStack(alignment: .topLeading) {
                        Rectangle()
                        if colored, let card = model.emptyCard {
                            RoundedRectangle(cornerRadius: cardRadius)
                                .frame(width: card.width, height: card.height)
                                .offset(x: card.minX, y: card.minY)
                                .blendMode(.destinationOut)
                        }
                    }
                    .compositingGroup()
                }
            // Without colors, an empty card is a little lighter than the window, measured.
            if !colored, let card = model.emptyCard {
                RoundedRectangle(cornerRadius: cardRadius)
                    .fill(Color.white.opacity(isDark(theme) ? 0.11 : 0.5))
                    .frame(width: card.width, height: card.height)
                    .offset(x: card.minX, y: card.minY)
            }
        }
        .ignoresSafeArea()
    }
}

/// The sidebar, in a child window of the main one, whose own views Chromium keeps every click
/// from: docked, it shows the backdrop through, and its transparent corner the window's buttons,
/// which clicks there reach; floating above the page, it blurs the page under it.
@MainActor
final class SidebarHost {
    private let window: NSWindow
    private let model: AppModel
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
    /// While it is docked, the room between it and the page resizes it, like Arc's: a window of
    /// its own, as the main one keeps the clicks on its views.
    private let resize = SidebarResize()
    private lazy var resizer: ChildPanel = {
        let panel = ChildPanel(view: resize)
        panel.ignoresMouseEvents = false
        return panel
    }()
    private var watch: AnyCancellable?
    private var revealed = false
    private var rightClicks: Any?

    init(window: NSWindow, model: AppModel) {
        self.window = window
        self.model = model
        container.wantsLayer = true
        effect.material = .sidebar
        effect.blendingMode = .behindWindow
        effect.state = .active
        effect.autoresizingMask = [.width, .height]
        let host = NSHostingView(rootView: SidebarView(model: model))
        // Its frame follows the sidebar's width, which its edge sets: no size of its own.
        host.sizingOptions = []
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
        resize.send = { model.run($0) }
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

    /// Closes it, its window closing.
    func close() {
        if let rightClicks { NSEvent.removeMonitor(rightClicks) }
        rightClicks = nil
        panel.close()
        resizer.close()
    }

    /// Shows the sidebar at `frame`, from the window's top left corner, or hides it.
    func place(_ frame: NSRect?, floating: Bool, revealed: Bool) {
        self.revealed = revealed
        if let content = window.contentView {
            edge.frame = NSRect(x: 0, y: 0, width: 8, height: content.bounds.height)
        }
        edge.isHidden = frame != nil
        // From the sidebar's edge across the margin before the page.
        resizer.place(
            in: window,
            frame.flatMap { frame in
                floating ? nil : NSRect(x: frame.maxX - 3, y: 0, width: 8, height: frame.height)
            }, raise: false)
        effect.isHidden = !floating
        // Floating, like Arc's: rounded 12 points, with a 1-point border.
        container.layer?.cornerRadius = floating ? 12 : 0
        container.layer?.masksToBounds = floating
        container.layer?.borderWidth = floating ? 1 : 0
        container.layer?.borderColor =
            (isDark(model.space?.theme)
            ? NSColor.white.withAlphaComponent(0.2) : NSColor.black.withAlphaComponent(0.1)).cgColor
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
        // Past the window's buttons and the sidebar's toggle, before the navigation buttons, in
        // the toolbar's room, centered with the window's buttons.
        let band = windowButtonsCenter * 2
        drag.frame = NSRect(
            x: 108, y: bounds.height - band, width: max(0, bounds.width - 208), height: band)
        panel.invalidateShadow()
    }
}

/// Resizes the docked sidebar when dragged, to the pointer.
final class SidebarResize: NSView {
    var send: (String) -> Void = { _ in }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: .zero,
                options: [.cursorUpdate, .mouseMoved, .activeAlways, .inVisibleRect],
                owner: self))
    }

    override func cursorUpdate(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
    override func mouseMoved(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
    override func resetCursorRects() { addCursorRect(bounds, cursor: .resizeLeftRight) }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override func mouseDown(with event: NSEvent) {}
    override func mouseUp(with event: NSEvent) { send("resized") }

    override func mouseDragged(with event: NSEvent) {
        guard let parent = window?.parent else { return }
        // In the main window's points from its left edge, like the app's.
        send("sidebar/width/\(parent.convertPoint(fromScreen: NSEvent.mouseLocation).x)")
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

/// Moves the main window when dragged, like a title bar and Arc's WindowDraggableView: the
/// sidebar is a window of its own, which asks the main one to move.
final class WindowDrag: NSView {
    /// Whether a double-click minimizes or zooms the window, as the system's setting asks.
    var doubleClicks = true

    /// Like Arc's, it moves an inactive window from the first click.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        guard let main = window?.parent else { return }
        // The system moves the window, as by its titlebar: with its tiling, and its child
        // windows, like the page's, along.
        guard event.clickCount == 2 else {
            main.performDrag(with: event)
            return
        }
        guard doubleClicks else { return }
        switch UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") {
        case "Minimize": main.performMiniaturize(nil)
        case "None": break
        default: main.performZoom(nil)
        }
    }
}
