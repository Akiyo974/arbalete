// The bars at the top of a split's pages and the focused page's outline, like Arc's Split View,
// or the bar above a page alone: the app places them over each page (send_panes in
// app/src/shell.rs). They are views of the card's window, above the pages it holds, so they move
// with them in one go, and the sidebar passes over them, like Arc's.

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
    /// The card's top left corner, from the window's.
    var cardX: Double
    var cardY: Double
}

/// The focused pane's outline, like Arc's: the theme's color, 2 points wide.
private let outlineWidth: CGFloat = 2
/// The card's, like Arc's (CARD_RADIUS in app/src/mac.rs).
private let paneRadius: CGFloat = 6

/// A bar's view: the card's window never becomes the key one, so every click in it is a first
/// one, which the bar's buttons take at once.
private final class BarHost: NSHostingView<PaneBar> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// A view the page under it keeps the clicks through.
private final class Outline: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

/// The bars and the outline of the split a window shows, made as it needs them.
@MainActor
final class PaneBars {
    private let model: AppModel
    private var bars: [BarHost] = []
    private let outline: Outline = {
        let view = Outline()
        view.wantsLayer = true
        view.layer?.cornerRadius = paneRadius
        view.layer?.borderWidth = outlineWidth
        return view
    }()

    /// The card's view the bars were last placed over.
    private weak var card: NSView?

    init(model: AppModel) {
        self.model = model
    }

    /// Lays `panes` out in `card`, the card's view, or takes them away without it.
    func place(_ panes: [Pane], in card: NSView?) {
        while bars.count < panes.count {
            let host = BarHost(rootView: PaneBar(model: model, pane: nil))
            host.sizingOptions = []
            host.wantsLayer = true
            host.layer?.cornerRadius = paneRadius
            host.layer?.masksToBounds = true
            bars.append(host)
        }
        guard let card else {
            close()
            return
        }
        self.card = card
        // Rects from the card's top left corner, in its own coordinates.
        func frame(_ x: Double, _ y: Double, _ width: Double, _ height: Double) -> NSRect {
            NSRect(
                x: x, y: card.isFlipped ? y : card.bounds.height - y - height, width: width,
                height: height)
        }
        // In the card's window, over Chromium's content view rather than in it: its hit test
        // would hand the bars clicks at the wrong place.
        let container = card.window?.contentView?.superview ?? card
        func placed(_ rect: NSRect) -> NSRect { container.convert(rect, from: card) }
        // Above the pages, which CEF may add or order since.
        let covered = container.subviews.last.map(ours) == false
        func raise(_ view: NSView) {
            if view.superview !== container || covered {
                container.addSubview(view, positioned: .above, relativeTo: nil)
            }
        }
        for (index, host) in bars.enumerated() {
            guard index < panes.count else {
                host.removeFromSuperview()
                continue
            }
            let pane = panes[index]
            host.rootView = PaneBar(model: model, pane: pane)
            host.layer?.backgroundColor = NSColor(color(pane.color ?? model.cardColor)).cgColor
            // Rounded like the card at its top only: the page goes on below. Its top is where y
            // is least when the bar counts down from it, whatever the card does.
            host.layer?.maskedCorners =
                host.isFlipped
                ? [.layerMinXMinYCorner, .layerMaxXMinYCorner]
                : [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]
            host.frame = placed(frame(pane.x, pane.y, pane.width, pane.bar))
            raise(host)
        }
        cutCorners(of: panes.map { frame($0.x, $0.y, $0.width, $0.height) }, in: card)
        // Around the focused page, once at least two show.
        if panes.count > 1, let focused = panes.first(where: \.focused) {
            outline.layer?.borderColor = NSColor(model.themeColor).cgColor
            outline.frame = placed(frame(focused.x, focused.y, focused.width, focused.height))
            raise(outline)
        } else {
            outline.removeFromSuperview()
        }
    }

    /// Cuts the pages' bottom corners, at `frames`, out of what CEF draws under them: it fills
    /// each page's room with the page's color, which would show past its rounded corners.
    private func cutCorners(of frames: [NSRect], in card: NSView) {
        guard
            let compositor = card.subviews.first(where: {
                $0.className == "ViewsCompositorSuperview"
            }), let layer = compositor.layer
        else { return }
        guard !frames.isEmpty else {
            layer.mask = nil
            return
        }
        let mask = layer.mask as? CAShapeLayer ?? CAShapeLayer()
        mask.fillRule = .evenOdd
        let path = CGMutablePath()
        path.addRect(layer.bounds)
        // The bottom is where y is least, unless the view counts down from its top.
        let flipped = compositor.isFlipped
        for frame in frames.map({ compositor.convert($0, from: card) }) {
            let y = flipped ? frame.maxY : frame.minY
            let inward: CGFloat = flipped ? -1 : 1
            for (x, sideways) in [(frame.minX, CGFloat(1)), (frame.maxX, -1)] {
                // From the corner, along the bottom, round the arc, and back down the side.
                path.move(to: CGPoint(x: x, y: y))
                path.addLine(to: CGPoint(x: x + sideways * paneRadius, y: y))
                path.addArc(
                    tangent1End: CGPoint(x: x, y: y),
                    tangent2End: CGPoint(x: x, y: y + inward * paneRadius), radius: paneRadius)
                path.addLine(to: CGPoint(x: x, y: y + inward * paneRadius))
                path.closeSubpath()
            }
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        mask.frame = layer.bounds
        mask.path = path
        layer.mask = mask
        CATransaction.commit()
    }

    private func ours(_ view: NSView) -> Bool {
        view === outline || bars.contains { $0 === view }
    }

    func close() {
        if let card { cutCorners(of: [], in: card) }
        bars.forEach { $0.removeFromSuperview() }
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

@_cdecl("arbalete_ui_panes")
public func placePanes(
    _ id: UInt32, _ card: UnsafeMutableRawPointer?, _ panes: UnsafePointer<CChar>
) {
    MainActor.assumeIsolated {
        let data = Data(String(cString: panes).utf8)
        let panes = (try? JSONDecoder().decode([Pane].self, from: data)) ?? []
        // The card's view, which its window keeps alive.
        let card = card.map { Unmanaged<NSView>.fromOpaque($0).takeUnretainedValue() }
        nativeWindows[id]?.paneBars.place(panes, in: card)
        let split = panes.filter { !$0.toolbar }
        nativeWindows[id]?.pageDrop.panes = split
        if let model = nativeWindows[id]?.model, model.panes != split { model.panes = split }
    }
}
