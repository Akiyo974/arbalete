// Peek, like Arc's: the app shows a link's page in a card over the window (peek in
// app/src/shell/peek.rs); here, the card with its bar and the page drawn in it, the page under it
// fading back, a click elsewhere closing it, and its controls at its right, which close it or
// keep its page in a tab or a split.

import AppKit
import SwiftUI

/// Peek's card's corners and bar, like Arc's (PANE_BAR_HEIGHT in app/src/shell.rs).
private let peekRadius: CGFloat = 10
private let peekBar: CGFloat = 32
/// Like Arc's, measured: the controls 12 points right of the card, 8 below its top, 32 across.
private let controlsGap: CGFloat = 12
private let controlsTop: CGFloat = 8
private let peekControl: CGFloat = 34
/// Between the controls, like Arc's, measured.
private let controlsSpacing: CGFloat = 10
/// Like Arc's, measured: the controls' dark navy, and their labels' indigo, border and keys.
private let controlFill = Color(red: 0.03, green: 0.03, blue: 0.16)
private let labelFill = Color(red: 0.07, green: 0.05, blue: 0.31)
private let labelEdge = Color(red: 0.2, green: 0.19, blue: 0.45)
private let labelInk = Color(red: 0.87, green: 0.86, blue: 1)
private let keyFill = Color(red: 0.8, green: 0.78, blue: 1)
private let keyInk = Color(red: 0.2, green: 0.16, blue: 0.75)
/// How far a control's label stands from it, like Arc's.
private let labelGap: CGFloat = 4
/// How long the card takes to grow from the link, like Arc's, measured.
private let growDuration = 0.2

/// What Peek's bar shows of its page.
@MainActor
final class PeekPage: ObservableObject {
    @Published var url = ""
    /// The color at the top of the page, as #rrggbb, empty while unknown.
    @Published var color = ""
    @Published var back = false
    @Published var forward = false
    @Published var loading = false
}

/// The label of the control hovered, if any.
@MainActor
private final class PeekLabel: ObservableObject {
    @Published var text = ""
    /// The keys of its shortcut, if any.
    @Published var keys: [String] = []
}

/// Peek's card: its bar on top, its page under it.
private final class PeekCard: NSView {
    let bar: NSView
    let page: NSView

    init(bar: NSView, page: NSView) {
        self.bar = bar
        self.page = page
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = peekRadius
        layer?.masksToBounds = true
        addSubview(page)
        addSubview(bar)
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        bar.frame = NSRect(x: 0, y: 0, width: bounds.width, height: peekBar)
        page.frame = NSRect(
            x: 0, y: peekBar, width: bounds.width, height: max(0, bounds.height - peekBar))
    }
}

@MainActor
final class PeekChrome {
    private let parent: NSWindow
    private let catcher: Overlay
    /// Its card, which its page is drawn in.
    private let card: Overlay
    let page = PageHost()
    let state = PeekPage()
    /// The tab whose Easel's board Peek shows, borrowed from its page's container.
    var lender: Int?
    private let controls: Overlay
    /// The label of the control hovered, beside it.
    private let label: Overlay
    private let labelText = PeekLabel()
    private let labelHost: NSView
    /// Fades the page under it back, or brings it forward again.
    var fade: (Bool) -> Void = { _ in }

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        let clicks = Catcher()
        clicks.closed = { model.run("close-peek") }
        // Over the whole window, as it changes size too.
        clicks.autoresizingMask = [.width, .height]
        catcher = Overlay(view: clicks)
        let bar = NSHostingView(rootView: PeekBar(model: model, page: state))
        bar.sizingOptions = []
        bar.safeAreaRegions = []
        card = Overlay(view: PeekCard(bar: bar, page: page))
        let labelHost = NSHostingView(rootView: PeekLabelView(label: labelText))
        labelHost.safeAreaRegions = []
        self.labelHost = labelHost
        label = Overlay(view: labelHost)
        let host = NSHostingView(rootView: PeekControls(model: model, hovered: { _, _ in }))
        host.sizingOptions = []
        host.safeAreaRegions = []
        controls = Overlay(view: host)
        host.rootView = PeekControls(model: model) { [weak self] index, text in
            self?.show(text, beside: index)
        }
    }

    /// Shows `text` at the left of the control at `index`, over the card, where it always has
    /// room; or hides it without text.
    private func show(_ text: (String, [String])?, beside index: Int) {
        guard let text, controls.isVisible else {
            label.place(in: parent, nil)
            return
        }
        labelText.text = text.0
        labelText.keys = text.1
        labelHost.layoutSubtreeIfNeeded()
        let size = labelHost.fittingSize
        let button = controls.view.frame
        let top = button.minY + CGFloat(index) * (peekControl + controlsSpacing)
        label.place(
            in: parent,
            NSRect(
                x: button.minX - labelGap - size.width, y: top + (peekControl - size.height) / 2,
                width: size.width, height: size.height))
    }

    /// Shows the card at `card`, in the window's points from its top left corner, with its
    /// controls beside it, or takes them away.
    func place(_ card: NSRect?) {
        guard let card, let content = parent.contentView else {
            if self.card.isVisible { fade(false) }
            label.place(in: parent, nil)
            controls.place(in: parent, nil)
            self.card.place(in: parent, nil)
            catcher.place(in: parent, nil)
            return
        }
        let showing = !self.card.isVisible
        catcher.place(in: parent, content.bounds)
        self.card.place(in: parent, card)
        self.card.view.needsLayout = true
        self.card.view.layoutSubtreeIfNeeded()
        controls.place(
            in: parent,
            NSRect(
                x: card.maxX + controlsGap, y: card.minY + controlsTop,
                width: peekControl, height: peekControl * 3 + controlsSpacing * 2))
        guard showing else { return }
        fade(true)
        // Like Arc's, it grows from the link clicked, under the pointer.
        let mouse = parent.mouseLocationOutsideOfEventStream
        grow(from: NSPoint(x: mouse.x, y: content.bounds.height - mouse.y))
    }

    /// Grows the card from a link's size at `point`, in the window's points from its top left
    /// corner, to its frame, its controls fading in.
    private func grow(from point: NSPoint) {
        guard let layer = card.view.layer, let controlsLayer = controls.view.layer ?? {
            controls.view.wantsLayer = true
            return controls.view.layer
        }() else { return }
        let frame = card.view.frame
        guard frame.width > 0, frame.height > 0 else { return }
        let source = NSRect(x: point.x - 80, y: point.y - 20, width: 160, height: 40)
        // From its top left corner, where its layer's anchor is: scaled, then moved.
        var from = CATransform3DMakeTranslation(source.minX - frame.minX, source.minY - frame.minY, 0)
        from = CATransform3DScale(from, source.width / frame.width, source.height / frame.height, 1)
        let grow = CABasicAnimation(keyPath: "transform")
        grow.fromValue = from
        grow.toValue = CATransform3DIdentity
        grow.duration = growDuration
        grow.timingFunction = CAMediaTimingFunction(name: .easeOut)
        layer.add(grow, forKey: "grow")
        let appear = CABasicAnimation(keyPath: "opacity")
        appear.fromValue = 0
        appear.toValue = 1
        appear.duration = growDuration
        controlsLayer.add(appear, forKey: "appear")
    }

    /// Takes its page's view away, its browser closing, which destroys it; or gives an Easel's
    /// board back to its tab, or lets it go, saved.
    func pageClosed() {
        for view in page.subviews {
            if view is EaselView, let lender, let container = pageContainers[lender] {
                container.host.addSubview(view)
            } else {
                view.removeFromSuperview()
            }
        }
        lender = nil
        page.layer?.backgroundColor = nil
    }

    func close() {
        label.close()
        controls.close()
        card.close()
        catcher.close()
    }
}

/// Over the window behind Peek: a click closes it.
private final class Catcher: NSView {
    var closed: () -> Void = {}

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override func mouseDown(with event: NSEvent) { closed() }
}

/// Peek's bar, like a split's page's: back, forward, reload or stop, and its page's host.
private struct PeekBar: View {
    @ObservedObject var model: AppModel
    @ObservedObject var page: PeekPage

    var body: some View {
        ZStack {
            HStack(spacing: 0) {
                button("arrow.left", "Back", enabled: page.back) { model.run("peek-page", "back") }
                button("arrow.right", "Forward", enabled: page.forward) {
                    model.run("peek-page", "forward")
                }
                if page.loading {
                    button("xmark", "Stop", enabled: true) { model.run("peek-page", "stop") }
                } else {
                    button("arrow.clockwise", "Reload", enabled: true) { model.run("peek-page", "reload") }
                }
                Spacer()
            }
            Label(domain(page.url), systemImage: "link")
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                .padding(.horizontal, 120)
                .help(page.url)
        }
        .padding(.horizontal, 4)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .foregroundStyle(.primary)
        .background(color(tint))
        // A hairline under the bar, over the page.
        .overlay(alignment: .bottom) {
            Rectangle().fill(.primary.opacity(0.05)).frame(height: 1)
        }
        .environment(\.colorScheme, luminance(tint) < 0.179 ? .dark : .light)
    }

    /// Like Arc's, the color at the top of the page, or the card's until it is known.
    private var tint: String { page.color.isEmpty ? model.cardColor : page.color }

    private func button(
        _ symbol: String, _ label: String, enabled: Bool, action: @escaping () -> Void
    ) -> some View {
        IconButton(symbol: symbol, label: label, size: 24, action: action)
            .disabled(!enabled)
            .opacity(enabled ? 1 : 0.35)
    }
}

/// Peek's controls, like Arc's: dark round buttons, each naming itself beside it on hover
/// (`hovered`, with its index and its label, or none as the pointer leaves it); close it, open its
/// page in a tab, or beside the selected tab in a split.
private struct PeekControls: View {
    let model: AppModel
    let hovered: (Int, (String, [String])?) -> Void

    var body: some View {
        VStack(spacing: controlsSpacing) {
            PeekControl(
                symbol: "xmark", label: "Close", keys: ["⌘", "W"], hovered: { hovered(0, $0) }
            ) { model.run("close-peek") }
            PeekControl(
                symbol: "arrow.up.left.and.arrow.down.right", label: "Expand", keys: ["⌘", "O"],
                hovered: { hovered(1, $0) }
            ) { model.run("keep-peek", ["split": false]) }
            PeekControl(
                symbol: "rectangle.split.2x1", label: "Create Split View", keys: [],
                hovered: { hovered(2, $0) }
            ) { model.run("keep-peek", ["split": true]) }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

/// A control's label, like Arc's: lavender on an indigo capsule, with its shortcut's keys.
private struct PeekLabelView: View {
    @ObservedObject var label: PeekLabel

    var body: some View {
        HStack(spacing: 4) {
            Text(label.text)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(labelInk)
                .padding(.trailing, label.keys.isEmpty ? 0 : 4)
            ForEach(label.keys, id: \.self) { key in
                Text(key)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(keyInk)
                    .frame(width: 17, height: 17)
                    .background(RoundedRectangle(cornerRadius: 4).fill(keyFill))
                    .overlay(RoundedRectangle(cornerRadius: 4).stroke(keyInk.opacity(0.4), lineWidth: 0.5))
            }
        }
        .padding(.leading, 10)
        .padding(.trailing, label.keys.isEmpty ? 10 : 6)
        .frame(height: 28)
        .background(Capsule().fill(labelFill))
        .overlay(Capsule().stroke(labelEdge, lineWidth: 0.5))
        .fixedSize()
    }
}

private struct PeekControl: View {
    let symbol: String
    let label: String
    let keys: [String]
    let hovered: ((String, [String])?) -> Void
    let action: () -> Void
    private let _over = State<Bool>(initialValue: false)
    private var over: Bool {
        get { _over.wrappedValue }
        nonmutating set { _over.wrappedValue = newValue }
    }

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: peekControl, height: peekControl)
                .background(Circle().fill(controlFill.opacity(over ? 1 : 0.92)))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .onHover {
            over = $0
            hovered($0 ? (label, keys) : nil)
        }
    }
}

@_cdecl("arbalete_ui_card")
public func placeCard(
    _ id: UInt32, _ shown: Bool, _ empty: Bool, _ x: Double, _ y: Double, _ width: Double,
    _ height: Double
) {
    MainActor.assumeIsolated {
        guard let model = nativeWindows[id]?.model else { return }
        let card = shown ? NSRect(x: x, y: y, width: width, height: height) : nil
        if model.card != card { model.card = card }
        if model.cardEmpty != empty { model.cardEmpty = empty }
    }
}

/// The view Peek's page is drawn in, in the window `id`: CEF makes the page's browser in it.
@_cdecl("arbalete_ui_peek_view")
public func peekView(_ id: UInt32) -> UnsafeMutableRawPointer? {
    // As an address: a pointer may not leave the main actor's closure.
    let address: UInt = MainActor.assumeIsolated {
        guard let page = nativeWindows[id]?.peek.page else { return 0 }
        return UInt(bitPattern: Unmanaged.passUnretained(page).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// Takes Peek's closing page's view away in the window `id`.
@_cdecl("arbalete_ui_peek_closed")
public func peekClosed(_ id: UInt32) {
    MainActor.assumeIsolated { nativeWindows[id]?.peek.pageClosed() }
}

@_cdecl("arbalete_ui_peek")
public func placePeek(
    _ id: UInt32, _ shown: Bool, _ x: Double, _ y: Double, _ width: Double, _ height: Double
) {
    MainActor.assumeIsolated {
        nativeWindows[id]?.peek.place(
            shown ? NSRect(x: x, y: y, width: width, height: height) : nil)
    }
}

/// Gives Peek's bar in the window `id` its page's address and state.
@_cdecl("arbalete_ui_peek_bar")
public func peekBar(
    _ id: UInt32, _ url: UnsafePointer<CChar>, _ color: UnsafePointer<CChar>, _ back: Bool,
    _ forward: Bool, _ loading: Bool
) {
    MainActor.assumeIsolated {
        guard let page = nativeWindows[id]?.peek.state else { return }
        let url = String(cString: url)
        let color = String(cString: color)
        if page.url != url { page.url = url }
        if page.color != color { page.color = color }
        if page.back != back { page.back = back }
        if page.forward != forward { page.forward = forward }
        if page.loading != loading { page.loading = loading }
    }
}
