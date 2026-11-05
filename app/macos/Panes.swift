// The tabs' pages in native containers, each a split's page's bar over a view CEF draws the page
// in, the bar above a page alone and the focused page's outline, like Arc's Split View: the app
// tells where each page shows (send_panes in app/src/shell.rs). They are views of the card's
// window, so a bar and its page move in one go, and the sidebar passes over them, like Arc's.

import AppKit
import SwiftUI

/// A page of the split shown, as the app sends it: from the card's top left corner.
struct Pane: Decodable, Equatable {
    var tab: Int
    var split: Int?
    var x: Double
    var y: Double
    var width: Double
    var height: Double
    /// The bar's height, at the top of the pane.
    var bar: Double
    var url: String
    var back: Bool
    var forward: Bool
    var loading: Bool
    var focused: Bool
    /// The color at the top of the page, as #rrggbb, which tints the bar like Arc's.
    var color: String?
    /// Whether its site is in developer mode: its bar is the developer bar.
    var developer: Bool
    /// The bar above a page alone, in toolbar or developer mode, rather than a split's page.
    var toolbar: Bool
    /// A page shown alone, rather than in a split.
    var alone: Bool
    /// Square, the card filling the room: a page alone in full screen, like Arc's.
    var square: Bool
    /// The card's top left corner, from the window's.
    var cardX: Double
    var cardY: Double
}

/// The focused pane's outline, like Arc's: the theme's color, 2 points wide.
private let outlineWidth: CGFloat = 2
/// The card's, like Arc's (CARD_RADIUS in app/src/mac.rs).
private let paneRadius: CGFloat = 6

/// A bar's view: every click in it is taken at once, its window being key or not.
private final class BarHost: NSHostingView<PaneBar> {
    required init(rootView: PaneBar) {
        super.init(rootView: rootView)
        // Up to the card's top, under the window's title bar: no room kept for it.
        safeAreaRegions = []
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// A view the page under it keeps the clicks through.
private final class Outline: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

/// The view CEF draws a page in: it adds its own view in it, which it keeps the size of.
final class PageHost: NSView {
    override func didAddSubview(_ subview: NSView) {
        super.didAddSubview(subview)
        subview.frame = bounds
        subview.autoresizingMask = [.width, .height]
    }
}

/// A tab's page, as a native container: a split's page's bar on top, the page under it, one
/// block that AppKit lays out, like Arc's.
@MainActor
final class PageContainer: NSView {
    fileprivate let host = PageHost()
    fileprivate let bar: BarHost
    fileprivate var barHeight: CGFloat = 0 {
        didSet { if barHeight != oldValue { arrange() } }
    }

    init(model: AppModel) {
        bar = BarHost(rootView: PaneBar(model: model, pane: nil))
        bar.sizingOptions = []
        super.init(frame: NSRect(x: 0, y: 0, width: 800, height: 600))
        wantsLayer = true
        layer?.cornerRadius = paneRadius
        layer?.masksToBounds = true
        isHidden = true
        addSubview(host)
        addSubview(bar)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    /// The view CEF draws the page's developer tools in, while open: under the page, which
    /// shows in the room they leave it, like Chrome's docked tools; or in a window of their own.
    fileprivate var tools: PageHost?
    /// Where the developer tools leave the page, from their top left corner, as they ask.
    fileprivate var room: NSRect? {
        didSet { if room != oldValue { arrange() } }
    }
    /// The window the developer tools show in while undocked.
    fileprivate var toolsWindow: NSWindow?

    /// Whether CEF's view of its page is at `view`: only its address, as CEF may have destroyed
    /// it since.
    func holds(_ view: UnsafeMutableRawPointer) -> Bool {
        host.subviews.contains { Unmanaged.passUnretained($0).toOpaque() == view }
    }

    /// Whether CEF's view of its developer tools is at `view`, as `holds` compares.
    fileprivate func holdsTools(_ view: UnsafeMutableRawPointer) -> Bool {
        tools?.subviews.contains { Unmanaged.passUnretained($0).toOpaque() == view } == true
    }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        arrange()
    }

    /// The bar on top, the page under it, as AppKit sizes the container; with docked developer
    /// tools, the tools under the bar and the page over them in its room.
    fileprivate func arrange() {
        bar.isHidden = barHeight == 0
        bar.frame = NSRect(x: 0, y: 0, width: bounds.width, height: barHeight)
        let body = NSRect(
            x: 0, y: barHeight, width: bounds.width, height: max(0, bounds.height - barHeight))
        guard let tools, tools.superview === self else {
            host.frame = body
            return
        }
        tools.frame = body
        // Over the tools until they say where the page goes.
        guard let room else {
            host.frame = body
            return
        }
        // The page leaves the tools' resizer whole on the sides it touches them, so that a drag
        // finds it.
        let left = room.minX > 0 ? toolsGrip : 0
        let top = room.minY > 0 ? toolsGrip : 0
        let right = body.width - room.maxX > 0 ? toolsGrip : 0
        let bottom = body.height - room.maxY > 0 ? toolsGrip : 0
        host.frame = NSRect(
            x: room.minX + left, y: body.minY + room.minY + top,
            width: max(0, room.width - left - right), height: max(0, room.height - top - bottom))
    }

    /// Makes the view the page's developer tools are drawn in, docked under the page.
    fileprivate func makeTools() -> PageHost {
        if let tools { return tools }
        let tools = PageHost()
        addSubview(tools, positioned: .below, relativeTo: host)
        self.tools = tools
        arrange()
        return tools
    }

    /// Takes the developer tools away, their view and its window, which destroys their browser.
    fileprivate func closeTools() {
        tools?.removeFromSuperview()
        tools = nil
        room = nil
        toolsWindow?.close()
        toolsWindow = nil
        arrange()
    }

    /// Docks the developer tools under the page, or shows them in a window of their own, whose
    /// close button closes them.
    fileprivate func dockTools(_ docked: Bool, closed: @escaping () -> Void) {
        guard let tools else { return }
        if docked {
            addSubview(tools, positioned: .below, relativeTo: host)
            toolsWindow?.close()
            toolsWindow = nil
            arrange()
            return
        }
        let window = ToolsWindow(closed: closed)
        tools.removeFromSuperview()
        window.contentView = tools
        window.center()
        window.makeKeyAndOrderFront(nil)
        toolsWindow = window
        arrange()
    }
}

/// How much of the developer tools' resizer, a few points across the page's edge, the page
/// leaves uncovered.
private let toolsGrip: CGFloat = 3

/// The window undocked developer tools show in: closing it closes them.
private final class ToolsWindow: NSWindow, NSWindowDelegate {
    private let closed: () -> Void

    init(closed: @escaping () -> Void) {
        self.closed = closed
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 960, height: 640),
            styleMask: [.titled, .closable, .resizable, .miniaturizable], backing: .buffered,
            defer: false)
        title = "Developer Tools"
        isReleasedWhenClosed = false
        delegate = self
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        closed()
        return false
    }
}

/// The tabs' pages' containers, by tab, wherever they show: a page shows in one window at a time.
@MainActor var pageContainers: [Int: PageContainer] = [:]

/// Lets the pages drawn in our views take the clicks: before taking one, a page asks its window's
/// content view what is under the pointer, and drops the click unless it is the page, which our
/// views put beside the content view rather than in it. A page the window finds under the pointer
/// takes the click, a tab's, its developer tools' or Peek's.
@MainActor private let letPagesTakeClicks: Void = {
    guard let page = NSClassFromString("RenderWidgetHostViewCocoa") else { return }
    let selector = NSSelectorFromString("shouldIgnoreMouseEvent:")
    guard let method = class_getInstanceMethod(page, selector) else { return }
    typealias ShouldIgnore = @convention(c) (NSView, Selector, NSEvent) -> Bool
    let inherited = unsafeBitCast(method_getImplementation(method), to: ShouldIgnore.self)
    let shouldIgnore: @convention(block) (NSView, NSEvent) -> Bool = { view, event in
        if view.window === event.window, let frame = view.window?.contentView?.superview,
            frame.hitTest(event.locationInWindow) === view,
            view.enclosing(PageHost.self) != nil
        {
            return false
        }
        return inherited(view, selector, event)
    }
    method_setImplementation(method, imp_implementationWithBlock(shouldIgnore))
}()

extension NSView {
    /// The view of type `type` this view is in, if any.
    fileprivate func enclosing<View: NSView>(_ type: View.Type) -> View? {
        var view = superview
        while let current = view {
            if let found = current as? View { return found }
            view = current.superview
        }
        return nil
    }
}

/// A view counting down from its top, which lets the clicks outside its subviews through to
/// what is under it, like the window's margins, which move it.
private final class Layer: NSView {
    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let view = super.hitTest(point)
        return view === self ? nil : view
    }
}

/// The pages a window shows, in their containers, with the bar above a page alone and the focused
/// page's outline, made as it needs them.
@MainActor
final class PagePlacer {
    private let model: AppModel
    private weak var window: NSWindow?
    /// The card's window, which CEF makes and lays out: found by its frame, as CEF gives no handle
    /// to it, and kept out of sight, the card showing in the window instead.
    private weak var cefCard: NSWindow?
    /// The card, in the window above its content view, which the app rounds; the pages'
    /// containers on its stage, which the library shifts (shift in app/src/mac.rs).
    private let card = Layer()
    private let stage = Layer()
    /// Over the window's content view, filling the window: the card is placed in it from the
    /// window's top left corner, as the app counts, however the window changes size after.
    private let layer = Layer()
    /// The bar above a page alone, in toolbar or developer mode.
    private let toolbar: BarHost
    private let outline: Outline = {
        let view = Outline()
        view.wantsLayer = true
        view.layer?.cornerRadius = paneRadius
        view.layer?.borderWidth = outlineWidth
        return view
    }()
    /// The tabs whose pages it shows.
    private var shown: [Int] = []
    /// The card's stage, which holds the pages, while the card shows, and the card's top left
    /// corner, in the window's points from its own.
    private(set) weak var holder: NSView?
    private(set) var cardOrigin: NSPoint = .zero
    /// Called once the pages are placed, for what goes above them, like a split's handles.
    var didPlace: () -> Void = {}

    init(in window: NSWindow, model: AppModel) {
        self.window = window
        self.model = model
        toolbar = BarHost(rootView: PaneBar(model: model, pane: nil))
        toolbar.sizingOptions = []
        toolbar.wantsLayer = true
        toolbar.layer?.cornerRadius = paneRadius
        toolbar.layer?.masksToBounds = true
        card.wantsLayer = true
        card.isHidden = true
        layer.autoresizingMask = [.width, .height]
        layer.addSubview(card)
        stage.frame = card.bounds
        stage.autoresizingMask = [.width, .height]
        card.addSubview(stage)
        overlays[ObjectIdentifier(window)] = { [weak self] in self?.overlay() }
        letPagesTakeClicks
        // Like Chrome's, the mouse's back and forward buttons move the page under the pointer.
        mouseButtons = NSEvent.addLocalMonitorForEvents(matching: .otherMouseDown) {
            [weak self] event in
            guard [3, 4].contains(event.buttonNumber), let window = self?.window,
                event.window === window,
                let container = window.contentView?.superview?.hitTest(event.locationInWindow)?
                    .enclosing(PageContainer.self),
                let tab = pageContainers.first(where: { $0.value === container })?.key
            else { return event }
            model.run("select/\(tab)")
            model.run(event.buttonNumber == 3 ? "back" : "forward")
            return nil
        }
    }

    /// The pages last laid out, and the card's frame.
    private var last: (panes: [Pane], frame: NSRect?)?
    /// Moves the page under the pointer back or forward (init).
    private var mouseButtons: Any?

    /// Fades the card back, like Arc's behind Peek, or brings it forward again.
    func fade(_ back: Bool) {
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.2
            card.animator().alphaValue = back ? 0.3 : 1
        }
    }

    /// The view filling the window over its content view, which holds the card, and the views
    /// placed over it from the window's top left corner, like the sidebar.
    func overlay() -> NSView? { mount() ? layer : nil }

    /// Puts the card in the window, above its content view: over Chromium's content view rather
    /// than in it, whose hit test would hand the pages and bars clicks at the wrong place.
    private func mount() -> Bool {
        guard let content = window?.contentView, let frame = content.superview else { return false }
        if layer.superview !== frame {
            frame.addSubview(layer, positioned: .above, relativeTo: content)
        }
        // The view holding the content view, which AppKit sizes first as the window changes size.
        if layer.frame != frame.bounds { layer.frame = frame.bounds }
        return true
    }

    /// A container for a tab's page, made hidden on the card until it shows: CEF makes the page's
    /// browser in its view.
    func container(for tab: Int) -> PageContainer? {
        if let container = pageContainers[tab] { return container }
        guard mount() else { return nil }
        let container = PageContainer(model: model)
        container.isHidden = true
        stage.addSubview(container)
        pageContainers[tab] = container
        // Laid out before it was made, as a window opens: it shows where it was laid out.
        if shown.contains(tab), let last { _ = place(last.panes, card: last.frame) }
        return container
    }

    /// CEF's card's window, found by its frame, `frame` in the window's points from its top left
    /// corner.
    private func cefCard(at frame: NSRect) -> NSWindow? {
        guard let window else { return nil }
        func at(_ child: NSWindow) -> Bool {
            let x = child.frame.minX - window.frame.minX
            let y = window.frame.maxY - child.frame.maxY
            return abs(x - frame.minX) < 2 && abs(y - frame.minY) < 2
                && abs(child.frame.width - frame.width) < 2
                && abs(child.frame.height - frame.height) < 2
        }
        // Found once, it stays, whose frame may lag behind as the window changes size.
        if let cefCard, cefCard.parent === window { return cefCard }
        cefCard = window.childWindows?.first { !($0 is NSPanel) && at($0) }
        return cefCard
    }

    /// Lays `panes` out in the card at `frame`, or hides them all without it. Returns the card,
    /// once CEF's card's window is found.
    func place(_ panes: [Pane], card frame: NSRect?) -> NSView? {
        // Out of sight, as the app hides, CEF tells the card is too: it keeps its layout, which
        // nothing would place again as the window shows.
        if frame == nil, window?.isVisible == false, !card.isHidden { return card }
        last = (panes, frame)
        let cefCard = frame.flatMap(cefCard(at:))
        defer { didPlace() }
        guard let frame, let cefCard, mount() else {
            hide(except: [])
            toolbar.removeFromSuperview()
            outline.removeFromSuperview()
            card.isHidden = true
            holder = nil
            return nil
        }
        // Laid out still, but out of sight and out of the way of the clicks.
        cefCard.alphaValue = 0
        cefCard.ignoresMouseEvents = true
        card.frame = frame
        card.isHidden = false
        holder = stage
        cardOrigin = frame.origin
        // From the card's top left corner, as the stage counts.
        func placed(_ x: Double, _ y: Double, _ width: Double, _ height: Double) -> NSRect {
            NSRect(x: x, y: y, width: width, height: height)
        }
        func raise(_ view: NSView) {
            if view.superview !== stage || stage.subviews.last !== view {
                stage.addSubview(view, positioned: .above, relativeTo: nil)
            }
        }
        let pages = panes.filter { !$0.toolbar && $0.tab >= 0 }
        let toolbarPane = panes.first(where: \.toolbar)
        for pane in pages {
            guard let page = pageContainers[pane.tab] else { continue }
            if page.superview !== stage { stage.addSubview(page) }
            page.frame = placed(pane.x, pane.y, pane.width, pane.height)
            page.barHeight = pane.bar
            if pane.bar > 0 {
                page.bar.rootView = PaneBar(model: model, pane: pane)
                page.bar.layer?.backgroundColor =
                    NSColor(color(pane.color ?? model.cardColor)).cgColor
            }
            // Under the bar above it, the page's top corners are square.
            page.layer?.maskedCorners =
                toolbarPane == nil
                ? [.layerMinXMinYCorner, .layerMaxXMinYCorner, .layerMinXMaxYCorner,
                    .layerMaxXMaxYCorner]
                : [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]
            page.layer?.cornerRadius = pane.square ? 0 : paneRadius
            page.isHidden = false
        }
        hide(except: pages.map(\.tab))
        if let pane = toolbarPane {
            toolbar.rootView = PaneBar(model: model, pane: pane)
            toolbar.layer?.backgroundColor = NSColor(color(pane.color ?? model.cardColor)).cgColor
            toolbar.layer?.maskedCorners =
                toolbar.isFlipped
                ? [.layerMinXMinYCorner, .layerMaxXMinYCorner]
                : [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]
            toolbar.layer?.cornerRadius = pane.square ? 0 : paneRadius
            toolbar.frame = placed(pane.x, pane.y, pane.width, pane.bar)
            raise(toolbar)
        } else {
            toolbar.removeFromSuperview()
        }
        // Around the focused page, once at least two show.
        if pages.count > 1, let focused = pages.first(where: \.focused) {
            outline.layer?.borderColor = NSColor(model.focusColor).cgColor
            outline.frame = placed(focused.x, focused.y, focused.width, focused.height)
            raise(outline)
        } else {
            outline.removeFromSuperview()
        }
        return card
    }

    /// Hides the pages it showed but `tabs`', which another window may show since.
    private func hide(except tabs: [Int]) {
        for tab in shown where !tabs.contains(tab) {
            guard let page = pageContainers[tab], page.window === window else { continue }
            page.isHidden = true
        }
        shown = tabs
    }

    /// Moves the containers in its window into `other`'s, hidden, its window closing.
    func handOver(to other: PagePlacer) {
        guard other.mount() else { return }
        for page in pageContainers.values where page.window != nil && page.window === window {
            page.isHidden = true
            other.stage.addSubview(page)
        }
        shown = []
    }

    func close() {
        if let mouseButtons { NSEvent.removeMonitor(mouseButtons) }
        if let window { overlays[ObjectIdentifier(window)] = nil }
        toolbar.removeFromSuperview()
        outline.removeFromSuperview()
    }
}

/// A page's bar, like Arc's: back and forward, the host the page shows, and × to close it; the
/// focused page's has reload and the split's menu too.
struct PaneBar: View {
    @ObservedObject var model: AppModel
    let pane: Pane?

    var body: some View {
        if let pane, pane.toolbar {
            PageToolbar(model: model)
                // Its ink reads on its color: the page's at its top, or the card's.
                .environment(
                    \.colorScheme,
                    luminance(pane.color ?? model.cardColor) < 0.179 ? .dark : .light)
        } else if let pane, pane.developer {
            DeveloperBar(
                model: model, url: pane.url,
                focus: { if !pane.focused { model.run("select/\(pane.tab)") } },
                close: { model.run("close/\(pane.tab)") })
        } else if let pane {
            ZStack {
                HStack(spacing: 0) {
                    button("arrow.left", "Back", enabled: pane.back) { act(pane, "back") }
                    button("arrow.right", "Forward", enabled: pane.forward) { act(pane, "forward") }
                    if pane.focused {
                        // Like Arc's, it stops the page while it loads.
                        if pane.loading {
                            button("xmark", "Stop", enabled: true) { act(pane, "stop") }
                        } else {
                            button("arrow.clockwise", "Reload", enabled: true) { act(pane, "reload") }
                        }
                    }
                    Spacer()
                    if pane.focused, let split = pane.split {
                        button("rectangle.split.2x1", "Split View", enabled: true) {
                            model.run("menu/node/\(split)")
                        }
                    }
                    button("xmark", "Close", enabled: true) { model.run("close/\(pane.tab)") }
                }
                Button {
                    act(pane, "overlay/edit")
                } label: {
                    Label(domain(pane.url), systemImage: "link")
                        .font(.system(size: 12))
                        .lineLimit(1)
                        .padding(.horizontal, 8)
                        .frame(height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(pane.url)
            }
            .padding(.horizontal, 4)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .foregroundStyle(.primary)
            // A hairline under the bar, over the page.
            .overlay(alignment: .bottom) {
                Rectangle().fill(.primary.opacity(0.05)).frame(height: 1)
            }
            // Its ink reads on its color: the page's at its top, or the card's.
            .environment(
                \.colorScheme, luminance(pane.color ?? model.cardColor) < 0.179 ? .dark : .light)
        }
    }

    /// Runs `command` on the pane's page: it is focused first.
    private func act(_ pane: Pane, _ command: String) {
        if !pane.focused { model.run("select/\(pane.tab)") }
        model.run(command)
    }

    private func button(
        _ symbol: String, _ label: String, enabled: Bool, action: @escaping () -> Void
    ) -> some View {
        IconButton(symbol: symbol, label: label, size: 24, action: action)
            .disabled(!enabled)
            .opacity(enabled ? 1 : 0.35)
    }
}

/// What the app sends of a window's pages: where its card is, in the window's points from its top
/// left corner, if it shows, and where each page shows in it.
private struct Panes: Decodable {
    var card: [Double]?
    var panes: [Pane]
}

/// Shows the pages of the window `id` as `panes` (JSON) lays them out, and returns the card's
/// content view, once the card shows.
@_cdecl("arbalete_ui_panes")
public func placePanes(_ id: UInt32, _ panes: UnsafePointer<CChar>) -> UnsafeMutableRawPointer? {
    // As an address: a pointer may not leave the main actor's closure.
    let address: UInt = MainActor.assumeIsolated {
        let data = Data(String(cString: panes).utf8)
        guard let sent = try? JSONDecoder().decode(Panes.self, from: data),
            let native = nativeWindows[id]
        else { return 0 }
        let card = sent.card.flatMap {
            $0.count == 4 ? NSRect(x: $0[0], y: $0[1], width: $0[2], height: $0[3]) : nil
        }
        // The empty page a drag over the split opens, tab -1, has no page.
        let empty = sent.panes.first { $0.tab == -1 }
        let content = native.pages.place(sent.panes.filter { $0.tab != -1 }, card: card)
        let split = sent.panes.filter { $0.tab != -1 && !$0.toolbar && !$0.alone }
        native.pageDrop.emptyPane = empty
        native.pageDrop.panes = split
        if native.model.panes != split { native.model.panes = split }
        return content.map { UInt(bitPattern: Unmanaged.passUnretained($0).toOpaque()) } ?? 0
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// The view a tab's page is drawn in, in the window `id`, hidden until it shows: CEF makes the
/// page's browser in it.
@_cdecl("arbalete_ui_page_view")
public func pageView(_ id: UInt32, _ tab: Int64) -> UnsafeMutableRawPointer? {
    // As an address: a pointer may not leave the main actor's closure.
    let address: UInt = MainActor.assumeIsolated {
        guard let container = nativeWindows[id]?.pages.container(for: Int(tab)) else {
            return 0
        }
        return UInt(bitPattern: Unmanaged.passUnretained(container.host).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// Takes away the container holding `view`, CEF's view of a page closing: the view goes with it,
/// which destroys the page's browser.
@_cdecl("arbalete_ui_page_closed")
public func pageClosed(_ view: UnsafeMutableRawPointer?) {
    MainActor.assumeIsolated {
        guard let view else { return }
        guard let (tab, container) = pageContainers.first(where: { $0.value.holds(view) })
        else { return }
        pageContainers.removeValue(forKey: tab)
        container.removeFromSuperview()
    }
}

/// Moves the pages' containers out of the window `from`, which closes, into the window `to`,
/// hidden.
@_cdecl("arbalete_ui_hand_pages_over")
public func handPagesOver(_ from: UInt32, _ to: UInt32) {
    MainActor.assumeIsolated {
        guard let from = nativeWindows[from], let to = nativeWindows[to] else { return }
        from.pages.handOver(to: to.pages)
    }
}

/// The view a tab's page's developer tools are drawn in, docked under the page: CEF makes their
/// browser in it.
@_cdecl("arbalete_ui_tools_view")
public func toolsView(_ tab: Int64) -> UnsafeMutableRawPointer? {
    // As an address: a pointer may not leave the main actor's closure.
    let address: UInt = MainActor.assumeIsolated {
        guard let tools = pageContainers[Int(tab)]?.makeTools() else { return 0 }
        return UInt(bitPattern: Unmanaged.passUnretained(tools).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// Where a tab's page's docked developer tools leave the page, from their top left corner, or
/// nowhere yet without a width.
@_cdecl("arbalete_ui_tools_room")
public func toolsRoom(_ tab: Int64, _ x: Double, _ y: Double, _ width: Double, _ height: Double) {
    MainActor.assumeIsolated {
        pageContainers[Int(tab)]?.room =
            width > 0 ? NSRect(x: x, y: y, width: width, height: height) : nil
    }
}

/// Docks a tab's page's developer tools under the page, or shows them in a window of their own.
@_cdecl("arbalete_ui_tools_docked")
public func toolsDocked(_ tab: Int64, _ docked: Bool) {
    MainActor.assumeIsolated {
        pageContainers[Int(tab)]?.dockTools(docked) {
            nativeWindows.values.first?.model.run("inspector/close/\(tab)")
        }
    }
}

/// Takes away the developer tools whose browser's view is `view`, their browser closing.
@_cdecl("arbalete_ui_tools_closed")
public func toolsClosed(_ view: UnsafeMutableRawPointer?) {
    MainActor.assumeIsolated {
        guard let view else { return }
        pageContainers.values.first { $0.holdsTools(view) }?.closeTools()
    }
}

/// Gives a tab's page's window the keyboard, the page taking it.
@_cdecl("arbalete_ui_focus_page")
public func focusPage(_ tab: Int64) {
    MainActor.assumeIsolated {
        guard let page = pageContainers[Int(tab)], !page.isHidden else { return }
        page.window?.makeKey()
    }
}
