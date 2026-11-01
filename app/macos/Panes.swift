// The bars at the top of a split's pages and the focused page's outline, like Arc's Split View:
// the app places them over each page (send_panes in app/src/shell.rs). They are views of the
// card's window, above the pages it holds, so they move with them in one go.

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
}

/// The focused pane's outline, like Arc's: the theme's color, 2 points wide.
private let outlineWidth: CGFloat = 2
private let paneRadius: CGFloat = 6

/// A view the page under it keeps the clicks through.
private final class Outline: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

/// The bars and the outline of the split a window shows, made as it needs them.
@MainActor
final class PaneBars {
    private let model: AppModel
    private var bars: [NSHostingView<PaneBar>] = []
    private let outline: Outline = {
        let view = Outline()
        view.wantsLayer = true
        view.layer?.cornerRadius = paneRadius
        view.layer?.borderWidth = outlineWidth
        return view
    }()

    init(model: AppModel) {
        self.model = model
    }

    /// Lays `panes` out in `card`, the card's view, or takes them away without it.
    func place(_ panes: [Pane], in card: NSView?) {
        while bars.count < panes.count {
            let host = NSHostingView(rootView: PaneBar(model: model, pane: nil))
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
        // Rects from the card's top left corner, in its own coordinates.
        func frame(_ x: Double, _ y: Double, _ width: Double, _ height: Double) -> NSRect {
            NSRect(
                x: x, y: card.isFlipped ? y : card.bounds.height - y - height, width: width,
                height: height)
        }
        // Above the pages, which CEF may add or order since.
        let covered = card.subviews.last.map(ours) == false
        func raise(_ view: NSView) {
            if view.superview !== card || covered {
                card.addSubview(view, positioned: .above, relativeTo: nil)
            }
        }
        for (index, host) in bars.enumerated() {
            guard index < panes.count else {
                host.removeFromSuperview()
                continue
            }
            let pane = panes[index]
            host.rootView = PaneBar(model: model, pane: pane)
            host.layer?.backgroundColor = NSColor(color(model.cardColor)).cgColor
            // Rounded like the card at its top only: the page goes on below.
            host.layer?.maskedCorners =
                card.isFlipped
                ? [.layerMinXMinYCorner, .layerMaxXMinYCorner]
                : [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]
            host.frame = frame(pane.x, pane.y, pane.width, pane.bar)
            raise(host)
        }
        // Around the focused page, once at least two show.
        if panes.count > 1, let focused = panes.first(where: \.focused) {
            outline.layer?.borderColor = NSColor(model.themeColor).cgColor
            outline.frame = frame(focused.x, focused.y, focused.width, focused.height)
            raise(outline)
        } else {
            outline.removeFromSuperview()
        }
    }

    private func ours(_ view: NSView) -> Bool {
        view === outline || bars.contains { $0 === view }
    }

    func close() {
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
        if let pane {
            ZStack {
                HStack(spacing: 0) {
                    button("arrow.left", "Back", enabled: pane.back) { act(pane, "back") }
                    button("arrow.right", "Forward", enabled: pane.forward) { act(pane, "forward") }
                    if pane.focused {
                        button("arrow.clockwise", "Reload", enabled: true) { act(pane, "reload") }
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
            .environment(\.colorScheme, isDark(model.space?.theme) ? .dark : .light)
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
    }
}
