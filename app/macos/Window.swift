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
        // Once the state changed: its publisher tells before.
        watch = model.$state.receive(on: DispatchQueue.main).sink { [effect] _ in
            let colored = !(model.space?.theme.colors.isEmpty ?? true)
            effect.material = colored || !systemDark() ? .light : .dark
        }
    }
}

/// The room between a split's pages (SPLIT_GAP in app/src/shell.rs).
private let splitGap: CGFloat = 10

/// The card's corners (CARD_RADIUS in app/src/mac.rs).
private let cardRadius: CGFloat = 6

/// How much white lightens an empty card, measured on Arc's: without colors, 11% in the dark
/// and half in the light; over a theme's colors, 8% while the app is in front, 2% behind.
private func emptyWash(colored: Bool, dark: Bool, active: Bool) -> Double {
    guard colored else { return dark ? 0.11 : 0.5 }
    return active ? 0.08 : 0.02
}

/// A page's loading, like Arc's scrim loader, measured in its code: for 3.2 seconds a 48 by 4
/// track pulses in opacity and scale; then, at the pulse's trough, it widens to 128 and a bar runs
/// along it, growing for 0.375 seconds, shrinking for 0.375, then gone for 0.375. It goes once the
/// page loads, or after 30 seconds. Between a split's stacked pages, it rests as a 64 by 2 grabber.
private struct LoadingLine: View {
    let loading: Bool
    let dark: Bool
    var grabber = false
    private let _start = State<Date?>(initialValue: nil)

    var body: some View {
        let start = _start.wrappedValue
        TimelineView(.animation(paused: start == nil)) { context in
            let elapsed = start.map { context.date.timeIntervalSince($0) } ?? 0
            let shown = start != nil && elapsed < 30
            // From the grabber, the pulse starts at its peak, and its trough comes 1.6 later.
            let carouselAt = grabber ? 4.8 : 3.2
            let carousel = shown && elapsed >= carouselAt
            let wave = cos(2 * .pi * elapsed / 3.2)
            let progress: Double =
                carousel ? 1 : shown ? (grabber ? 0.5 + 0.5 * wave : 0.5 - 0.5 * wave) : grabber ? 1 : 0
            let ink = dark ? Color.white : Color.black
            let fill: Double =
                carousel ? (dark ? 0.46 : 0.276) : shown ? 0.92 * progress : grabber ? 0.5 : 0
            let size: CGSize =
                carousel ? CGSize(width: 128, height: 4)
                : shown ? CGSize(width: 48, height: 4)
                : grabber ? CGSize(width: 64, height: 2) : CGSize(width: 48, height: 4)
            ZStack(alignment: .leading) {
                Rectangle().fill(ink.opacity(fill))
                if carousel {
                    let (from, to) = segment(elapsed - carouselAt)
                    Rectangle().fill(ink.opacity(0.92))
                        .frame(width: 128 * (to - from))
                        .offset(x: 128 * from)
                }
            }
            .frame(width: size.width, height: size.height, alignment: .leading)
            .clipShape(Capsule())
            .scaleEffect(0.88 + 0.12 * progress)
            .animation(.spring(response: 0.8, dampingFraction: 0.92), value: carousel)
            .animation(.spring(response: 0.8, dampingFraction: 0.92), value: shown)
        }
        .frame(width: 128, height: 4)
        .onAppear { if loading { _start.wrappedValue = Date() } }
        .onChange(of: loading) { _, now in _start.wrappedValue = now ? Date() : nil }
    }

    /// The bar's start and end along the track, from 0 to 1, `time` into the carousel.
    private func segment(_ time: Double) -> (Double, Double) {
        let t = time.truncatingRemainder(dividingBy: 1.125)
        if t <= 0.375 { return (0, t / 0.375) }
        if t <= 0.75 { return ((t - 0.375) / 0.375, 1) }
        return (0, 0)
    }
}

struct BackdropTint: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let theme = model.space?.theme
        let colored = !(theme?.colors.isEmpty ?? true)
        ZStack(alignment: .topLeading) {
            // Like Arc's, a swipe between Spaces fades the window into the coming one's colors.
            let (coming, share) = model.swipeNeighbor()
            // Like Arc's, an incognito window is near black, measured, and opaque.
            if model.state?.incognito == true {
                color("#0e0f13")
            } else {
                Tint(theme: theme, active: model.active).opacity(1 - share)
                if share > 0 {
                    Tint(theme: coming, active: model.active).opacity(share)
                }
            }
            // Like Arc's, each of a split's pages loading shows a line above it, in the room over
            // the card or between the pages, where a grabber rests between stacked ones.
            if model.panes.count > 1, let pane = model.panes.first(where: \.focused) {
                // Like Arc's, a glow around the focused page, outside it, measured.
                RoundedRectangle(cornerRadius: 8)
                    .fill(
                        isDark(theme) ? Color.white.opacity(0.27) : model.focusColor.opacity(0.3))
                    .frame(width: pane.width, height: pane.height)
                    .blur(radius: 8)
                    .position(
                        x: pane.cardX + pane.x + pane.width / 2,
                        y: pane.cardY + pane.y + pane.height / 2)
            }
            if model.panes.count > 1 {
                ForEach(model.panes, id: \.tab) { pane in
                    LoadingLine(loading: pane.loading, dark: isDark(theme), grabber: pane.y > 0)
                        .position(
                            x: pane.cardX + pane.x + pane.width / 2,
                            y: pane.y <= 0
                                ? pane.cardY / 2 : pane.cardY + pane.y - splitGap / 2)
                }
            }
            if let card = model.card {
                // Like Arc's, a faint shade around the card, outside it only, and an empty card a
                // little lighter than the window, measured on Arc's.
                let wash = model.cardEmpty
                    ? emptyWash(colored: colored, dark: isDark(theme), active: model.active) : 0
                Canvas { context, _ in
                    let shape = Path(roundedRect: card, cornerRadius: cardRadius)
                    var shade = context
                    shade.addFilter(.blur(radius: 2))
                    shade.fill(
                        Path(roundedRect: card.insetBy(dx: -1, dy: -1), cornerRadius: cardRadius),
                        with: .color(.black.opacity(0.12)))
                    context.blendMode = .clear
                    context.fill(shape, with: .color(.black))
                    context.blendMode = .normal
                    context.fill(shape, with: .color(.white.opacity(wash)))
                }
                // Like Arc's, a page loading shows a line in the margin above it.
                LoadingLine(loading: model.state?.page.loading == true, dark: isDark(theme))
                    .position(x: card.midX, y: card.minY / 2)
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
    /// Barely there, so that clicks on the docked sidebar's transparent parts reach it, rather
    /// than the main window under it: below the toolbar, and the toolbar past the window's
    /// buttons, which keep theirs.
    private let catchers = [NSView(), NSView()]
    /// While the sidebar hides, its edge of the window shows it again on hover.
    private let edge = Tracking()
    /// While it is docked, the margin between its content and the page resizes it, like Arc's:
    /// above its other views.
    private let resize = SidebarResize()
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
        leave.exited = { [weak self] in
            if self?.revealed == true { model.run("sidebar/conceal") }
        }
        edge.entered = { model.run("sidebar/reveal") }
        edge.isHidden = true
        edge.autoresizingMask = [.height]
        window.contentView?.addSubview(edge)
        resize.send = { model.run($0) }
        resize.ended = { [weak self] in
            // Hidden by the drag, it goes once the drag ends.
            guard let self, self.panel.alphaValue == 0 else { return }
            self.place(nil, floating: false, revealed: false)
        }
        // Like Arc's, read in its code: the theme's color, half seen.
        resize.tint = { NSColor(model.themeColor) }
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
        // Once the state changed, the displayed Space's: its publisher tells before.
        watch = model.$state.receive(on: DispatchQueue.main).sink { [container] _ in
            container.appearance = NSAppearance(
                named: isDark(model.space?.theme) ? .darkAqua : .aqua)
        }
    }

    /// Closes it, its window closing.
    func close() {
        if let rightClicks { NSEvent.removeMonitor(rightClicks) }
        rightClicks = nil
        panel.close()
    }

    /// Shows the sidebar at `frame`, from the window's top left corner, or hides it.
    func place(_ frame: NSRect?, floating: Bool, revealed: Bool) {
        // Like Arc's, a drag of its edge hides it past its least width and shows it again: while
        // the drag goes on, it only fades, as a window out of sight would lose the drag.
        if frame == nil, resize.dragging, panel.isVisible {
            panel.alphaValue = 0
            return
        }
        panel.alphaValue = 1
        self.revealed = revealed
        if let content = window.contentView {
            edge.frame = NSRect(x: 0, y: 0, width: 8, height: content.bounds.height)
        }
        edge.isHidden = frame != nil
        effect.isHidden = !floating
        // Floating, like Arc's: rounded 12 points, with a 1-point border.
        container.layer?.cornerRadius = floating ? 12 : 0
        container.layer?.masksToBounds = floating
        container.layer?.borderWidth = floating ? 1 : 0
        container.layer?.borderColor =
            (isDark(model.space?.theme)
            ? NSColor.white.withAlphaComponent(0.2) : NSColor.black.withAlphaComponent(0.1)).cgColor
        panel.hasShadow = floating
        // Only once it changes: publishing it draws the sidebar again, which ends a drag in it.
        let floats = floating && frame != nil
        let floated = model.sidebarFloating
        if floated != floats { model.sidebarFloating = floats }
        let docked = frame != nil && !floating
        if model.sidebarDocked != docked { model.sidebarDocked = docked }
        // Raised above the window's others only as it shows: raising takes it out of the window
        // a moment, which would end a drag in it.
        panel.place(in: window, frame, raise: !panel.isVisible)
        // As it starts floating, above the card's window too, which CEF orders as it lays the
        // card out: like Arc's, the sidebar passes over the page and its bar. Ordering it again
        // while it floats would end a drag in it.
        if floats, !floated { panel.order(.above, relativeTo: 0) }
        if container.superview != panel.contentView {
            panel.contentView?.addSubview(container)
        }
        container.frame = panel.contentView?.bounds ?? .zero
        container.autoresizingMask = [.width, .height]
        container.subviews.forEach { $0.frame = container.bounds }
        let bounds = container.bounds
        // Across the margin between the sidebar's content and the page.
        if resize.superview != container { container.addSubview(resize) }
        resize.frame = NSRect(x: bounds.width - 8, y: 0, width: 8, height: bounds.height)
        resize.isHidden = floating
        catchers[0].frame = NSRect(x: 0, y: 0, width: bounds.width, height: max(0, bounds.height - 40))
        let past = windowButtonsEnd - 6
        catchers[1].frame = NSRect(
            x: past, y: bounds.height - 40, width: max(0, bounds.width - past), height: 40)
        panel.invalidateShadow()
    }
}

/// Resizes the docked sidebar when dragged, to the pointer. Like Arc's, a line shows along the
/// page once the pointer rests over it a moment, and while it drags.
/// Like Arc's: the pointer rests a quarter of a second before the line fades in or out, which
/// takes a fifth of one, to half the tint.
private let hoverDelay = 0.25
private let fadeDuration = 0.2
private let litOpacity: Float = 0.5

final class SidebarResize: NSView {
    var send: (String) -> Void = { _ in }
    var tint: () -> NSColor = { .controlAccentColor }
    /// Called once a drag ends.
    var ended: () -> Void = {}
    private(set) var dragging = false { didSet { light() } }
    private var hovered = false { didSet { light() } }
    private var hover: DispatchWorkItem?
    private let line = CAShapeLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        line.isHidden = true
        line.opacity = 0
        layer?.addSublayer(line)
    }

    required init?(coder: NSCoder) { nil }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        line.frame = bounds
        // Like Arc's: 3 points wide with round ends, 2.5 points short of the page, 4 points
        // short of its top and bottom (CARD_MARGIN, 10).
        let rect = NSRect(x: bounds.width - 5.5, y: 14, width: 3, height: max(0, bounds.height - 28))
        line.path = CGPath(roundedRect: rect, cornerWidth: 1.5, cornerHeight: 1.5, transform: nil)
        CATransaction.commit()
    }

    /// Fades the line in while hovered or dragged, out otherwise, then hides it.
    private func light() {
        let lit = dragging || hovered
        guard lit != (line.opacity == litOpacity) else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if lit {
            line.fillColor = tint().cgColor
            line.isHidden = false
        }
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = line.presentation()?.opacity ?? line.opacity
        fade.toValue = lit ? litOpacity : 0
        fade.duration = fadeDuration
        line.opacity = lit ? litOpacity : 0
        CATransaction.setCompletionBlock { [weak self] in
            guard let self, !(dragging || hovered) else { return }
            line.isHidden = true
        }
        line.add(fade, forKey: "opacity")
        CATransaction.commit()
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: .zero,
                options: [
                    .cursorUpdate, .mouseMoved, .mouseEnteredAndExited, .activeAlways,
                    .inVisibleRect,
                ],
                owner: self))
    }

    override func cursorUpdate(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
    override func mouseMoved(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
    override func mouseEntered(with event: NSEvent) { rest(true) }

    override func mouseExited(with event: NSEvent) {
        rest(false)
        // Only over it: the cursor goes back to the arrow past it, but while dragging it.
        if !dragging { NSCursor.arrow.set() }
    }
    /// Like Arc's, the line follows the pointer in or out of it only once it rests there.
    private func rest(_ over: Bool) {
        hover?.cancel()
        let change = DispatchWorkItem { [weak self] in self?.hovered = over }
        hover = change
        DispatchQueue.main.asyncAfter(deadline: .now() + hoverDelay, execute: change)
    }

    override func resetCursorRects() { addCursorRect(bounds, cursor: .resizeLeftRight) }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override func mouseDown(with event: NSEvent) {
        // Like Arc's, a double-click gives it back its usual width (SNAP_SIDEBAR).
        if event.clickCount == 2 {
            send("sidebar/width/228")
            return
        }
        dragging = true
    }
    override func mouseUp(with event: NSEvent) {
        dragging = false
        send("resized")
        ended()
    }

    override func mouseDragged(with event: NSEvent) {
        // Like Arc's, the cursor stays until the drag ends, past the edge, the sidebar hidden.
        NSCursor.resizeLeftRight.set()
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
/// The window's buttons, like Arc's in its floating sidebar, which covers the window's own: the
/// same buttons, acting on the main window.
struct WindowLights: NSViewRepresentable {
    func makeNSView(context: Context) -> LightsView { LightsView() }
    func updateNSView(_ view: LightsView, context: Context) {}
}

final class LightsView: NSView {
    private let buttons: [NSButton]

    init() {
        let kinds: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]
        buttons = kinds.compactMap {
            NSWindow.standardWindowButton($0, for: [.titled, .closable, .miniaturizable, .resizable])
        }
        super.init(frame: NSRect(origin: .zero, size: windowLightsSize))
        for (index, button) in buttons.enumerated() {
            let natural = button.frame.size
            button.frame = NSRect(
                x: CGFloat(index) * windowButtonsSpacing, y: 0, width: windowButtonsCircle,
                height: windowButtonsCircle)
            button.setBoundsSize(natural)
            addSubview(button)
        }
    }

    required init?(coder: NSCoder) { nil }

    override var intrinsicContentSize: NSSize { windowLightsSize }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        let actions = [
            #selector(NSWindow.performClose(_:)), #selector(NSWindow.miniaturize(_:)),
            #selector(NSWindow.zoom(_:)),
        ]
        for (button, action) in zip(buttons, actions) {
            button.target = window?.parent
            button.action = action
        }
    }
}

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
