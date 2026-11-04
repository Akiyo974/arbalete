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
    /// The card's top left corner, from the window's.
    var cardX: Double
    var cardY: Double
}

/// The focused pane's outline, like Arc's: the theme's color, 2 points wide.
private let outlineWidth: CGFloat = 2
/// The card's, like Arc's (CARD_RADIUS in app/src/mac.rs).
private let paneRadius: CGFloat = 6

/// A bar's view: every click in it is taken at once, the card's window being key or not.
private final class BarHost: NSHostingView<PaneBar> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// A view the page under it keeps the clicks through.
private final class Outline: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

/// The view CEF draws a page in: it adds its own view in it, which it keeps the size of.
private final class PageHost: NSView {
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

    /// Whether CEF's view of its page is `view`.
    func holds(_ view: AnyObject) -> Bool {
        host.subviews.contains { $0 === view }
    }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        arrange()
    }

    /// The bar on top, the page under it, as AppKit sizes the container.
    private func arrange() {
        bar.isHidden = barHeight == 0
        bar.frame = NSRect(x: 0, y: 0, width: bounds.width, height: barHeight)
        host.frame = NSRect(
            x: 0, y: barHeight, width: bounds.width, height: max(0, bounds.height - barHeight))
    }
}

/// The tabs' pages' containers, by tab, wherever they show: a page shows in one window at a time.
@MainActor var pageContainers: [Int: PageContainer] = [:]

/// Lets a CEF window, like the card's, take the keyboard: CEF keeps it from it, the views it held
/// taking the keyboard through the window above, but a page drawn in it needs it.
@MainActor private func letTakeKeyboard(_ window: NSWindow) {
    guard !window.canBecomeKey, let base = object_getClass(window) else { return }
    let name = "Arbalete" + NSStringFromClass(base)
    var keyable: AnyClass? = NSClassFromString(name)
    if keyable == nil, let made = objc_allocateClassPair(base, name, 0) {
        let canBecomeKey: @convention(block) (AnyObject) -> Bool = { _ in true }
        class_addMethod(
            made, #selector(getter: NSWindow.canBecomeKey),
            imp_implementationWithBlock(canBecomeKey), "c@:")
        objc_registerClassPair(made)
        keyable = made
    }
    if let keyable { object_setClass(window, keyable) }
}

/// The pages a window shows, in their containers, with the bar above a page alone and the focused
/// page's outline, made as it needs them.
@MainActor
final class PagePlacer {
    private let model: AppModel
    private weak var window: NSWindow?
    /// The card's window, which CEF makes: found by its frame, as CEF gives no handle to it.
    private weak var card: NSWindow?
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
    /// The view of the card's window that holds the pages, once found, and the card's top left
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
    }

    /// A container for a tab's page, made hidden in the window until it shows: CEF makes the
    /// page's browser in its view.
    func container(for tab: Int) -> PageContainer? {
        if let container = pageContainers[tab] { return container }
        guard let holder = window?.contentView?.superview else { return nil }
        let container = PageContainer(model: model)
        holder.addSubview(container)
        pageContainers[tab] = container
        return container
    }

    /// The card's window, whose frame is `frame` in the window's points from its top left corner.
    private func cardWindow(at frame: NSRect) -> NSWindow? {
        guard let window else { return nil }
        func at(_ child: NSWindow) -> Bool {
            let x = child.frame.minX - window.frame.minX
            let y = window.frame.maxY - child.frame.maxY
            return abs(x - frame.minX) < 2 && abs(y - frame.minY) < 2
                && abs(child.frame.width - frame.width) < 2
                && abs(child.frame.height - frame.height) < 2
        }
        if let card, card.parent === window, at(card) { return card }
        card = window.childWindows?.first { !($0 is NSPanel) && at($0) }
        if let card {
            letTakeKeyboard(card)
            keyObserver.map(NotificationCenter.default.removeObserver)
            keyObserver = NotificationCenter.default.addObserver(
                forName: NSWindow.didBecomeKeyNotification, object: card, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.lowerCard() }
            }
        }
        return card
    }

    /// Watches the card's window take the keyboard (lowerCard).
    private var keyObserver: NSObjectProtocol?

    /// Puts the card's window back under the panels above it, like the revealed sidebar or a drag's
    /// room, as AppKit brings it to the front once a click in a page makes it key.
    private func lowerCard() {
        guard let window else { return }
        DispatchQueue.main.async {
            for panel in window.childWindows ?? [] where panel is NSPanel && panel.isVisible {
                window.removeChildWindow(panel)
                window.addChildWindow(panel, ordered: .above)
            }
        }
    }

    /// Lays `panes` out in the card at `frame`, or hides them all without it. Returns the card's
    /// content view.
    func place(_ panes: [Pane], card frame: NSRect?) -> NSView? {
        let cardWindow = frame.flatMap(cardWindow(at:))
        defer { didPlace() }
        guard let frame, let cardWindow, let content = cardWindow.contentView,
            let container = content.superview
        else {
            hide(except: [])
            toolbar.removeFromSuperview()
            outline.removeFromSuperview()
            holder = nil
            return nil
        }
        holder = container
        cardOrigin = frame.origin
        // Rects from the card's top left corner, in its content view's coordinates, then in the
        // view above it that holds ours: over Chromium's content view rather than in it, whose hit
        // test would hand the pages and bars clicks at the wrong place.
        func placed(_ x: Double, _ y: Double, _ width: Double, _ height: Double) -> NSRect {
            let rect = NSRect(
                x: x, y: content.isFlipped ? y : content.bounds.height - y - height, width: width,
                height: height)
            return container.convert(rect, from: content)
        }
        func raise(_ view: NSView) {
            if view.superview !== container || container.subviews.last !== view {
                container.addSubview(view, positioned: .above, relativeTo: nil)
            }
        }
        let pages = panes.filter { !$0.toolbar && $0.tab >= 0 }
        let toolbarPane = panes.first(where: \.toolbar)
        for pane in pages {
            guard let page = pageContainers[pane.tab] else { continue }
            if page.superview !== container { container.addSubview(page) }
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
        return content
    }

    /// Hides the pages it showed but `tabs`', which another window may show since.
    private func hide(except tabs: [Int]) {
        for tab in shown where !tabs.contains(tab) {
            guard let page = pageContainers[tab], page.window === card || page.window === window
            else { continue }
            page.isHidden = true
        }
        shown = tabs
    }

    /// Moves the containers in its window into `other`'s, hidden, its window closing.
    func handOver(to other: PagePlacer) {
        guard let holder = other.window?.contentView?.superview else { return }
        for page in pageContainers.values
        where page.window != nil && (page.window === window || page.window === card) {
            page.isHidden = true
            holder.addSubview(page)
        }
        shown = []
    }

    func close() {
        keyObserver.map(NotificationCenter.default.removeObserver)
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
        let closing = Unmanaged<AnyObject>.fromOpaque(view).takeUnretainedValue()
        guard let (tab, container) = pageContainers.first(where: { $0.value.holds(closing) })
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

/// Gives a tab's page's window the keyboard, the page taking it.
@_cdecl("arbalete_ui_focus_page")
public func focusPage(_ tab: Int64) {
    MainActor.assumeIsolated {
        guard let page = pageContainers[Int(tab)], !page.isHidden else { return }
        page.window?.makeKey()
    }
}
