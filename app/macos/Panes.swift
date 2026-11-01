// The bars at the top of a split's pages and the focused page's outline, like Arc's Split View:
// the app places them over each page's card (send_panes in app/src/shell.rs). The pages are
// windows of their own, so the bars and the outline are child windows too.

import AppKit
import SwiftUI

/// A page of the split shown, as the app sends it.
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

/// The bars and the outline of the split a window shows, made as it needs them.
@MainActor
final class PaneBars {
    private let parent: NSWindow
    private let model: AppModel
    private var bars: [(panel: ChildPanel, host: NSHostingView<PaneBar>)] = []
    private let outline: ChildPanel = {
        let view = NSView()
        view.wantsLayer = true
        view.layer?.cornerRadius = paneRadius
        view.layer?.borderWidth = outlineWidth
        let panel = ChildPanel(view: view)
        // The page under it keeps the clicks.
        panel.ignoresMouseEvents = true
        return panel
    }()

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        self.model = model
    }

    func place(_ panes: [Pane]) {
        while bars.count < panes.count {
            let host = NSHostingView(rootView: PaneBar(model: model, pane: nil))
            host.sizingOptions = []
            host.wantsLayer = true
            // Rounded like the card at its top only: the page goes on below.
            host.layer?.cornerRadius = paneRadius
            // Its top is where y is least when the view counts down from it.
            host.layer?.maskedCorners =
                host.isFlipped
                ? [.layerMinXMinYCorner, .layerMaxXMinYCorner]
                : [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]
            host.layer?.masksToBounds = true
            let panel = ChildPanel(view: host)
            panel.ignoresMouseEvents = false
            bars.append((panel, host))
        }
        for (index, bar) in bars.enumerated() {
            guard index < panes.count else {
                bar.panel.place(in: parent, nil)
                continue
            }
            let pane = panes[index]
            bar.host.rootView = PaneBar(model: model, pane: pane)
            bar.host.layer?.backgroundColor = NSColor(color(model.cardColor)).cgColor
            bar.panel.place(
                in: parent, NSRect(x: pane.x, y: pane.y, width: pane.width, height: pane.bar),
                raise: false)
        }
        // Around the focused page, once at least two show.
        let focused = panes.count > 1 ? panes.first(where: \.focused) : nil
        if let focused {
            outline.contentView?.layer?.borderColor =
                NSColor(model.themeColor).cgColor
        }
        outline.place(
            in: parent,
            focused.map { NSRect(x: $0.x, y: $0.y, width: $0.width, height: $0.height) },
            raise: false)
        // Above the pages, which CEF may order above them.
        DispatchQueue.main.async { [self] in
            let ours = bars.map(\.panel) + [outline]
            guard let last = parent.childWindows?.last, !ours.contains(where: { $0 === last })
            else { return }
            for panel in ours where panel.isVisible {
                parent.removeChildWindow(panel)
                parent.addChildWindow(panel, ordered: .above)
            }
        }
    }

    func close() {
        bars.forEach { $0.panel.close() }
        outline.close()
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
public func placePanes(_ id: UInt32, _ panes: UnsafePointer<CChar>) {
    MainActor.assumeIsolated {
        let data = Data(String(cString: panes).utf8)
        let panes = (try? JSONDecoder().decode([Pane].self, from: data)) ?? []
        nativeWindows[id]?.paneBars.place(panes)
    }
}
