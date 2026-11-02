// Peek, like Arc's: the app shows a link's page in a card over the window (peek in
// app/src/shell.rs); here, the scrim that dims the window behind it, which closes it when
// clicked, and its controls at its right, which close it or keep its page in a tab or a split.

import AppKit
import SwiftUI

/// How far Peek's controls stand from its card.
private let controlsGap: CGFloat = 8
private let peekControl: CGFloat = 32

@MainActor
final class PeekChrome {
    private let parent: NSWindow
    private let scrim: ChildPanel
    private let controls: ChildPanel

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        let dim = Scrim()
        dim.closed = { model.run("peek/close") }
        scrim = ChildPanel(view: dim)
        scrim.ignoresMouseEvents = false
        let host = NSHostingView(rootView: PeekControls(model: model))
        host.sizingOptions = []
        controls = ChildPanel(view: host)
        controls.ignoresMouseEvents = false
    }

    /// Shows the scrim over the window and the controls beside the card at `card`, in the
    /// window's points from its top left corner, or takes them away.
    func place(_ card: NSRect?) {
        guard let card, let content = parent.contentView else {
            controls.place(in: parent, nil)
            scrim.place(in: parent, nil)
            return
        }
        scrim.place(in: parent, content.bounds, raise: false)
        controls.place(
            in: parent,
            NSRect(
                x: card.maxX + controlsGap, y: card.minY, width: peekControl,
                height: peekControl * 3 + controlsGap * 2), raise: false)
    }

    func close() {
        controls.close()
        scrim.close()
    }
}

/// Dims the window behind Peek; a click closes it.
private final class Scrim: NSView {
    var closed: () -> Void = {}

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.withAlphaComponent(0.25).cgColor
    }

    required init?(coder: NSCoder) { nil }

    override func mouseDown(with event: NSEvent) { closed() }
}

/// Peek's controls: close it, open its page in a tab, or beside the selected tab in a split.
private struct PeekControls: View {
    let model: AppModel

    var body: some View {
        VStack(spacing: controlsGap) {
            control("xmark", "Close") { model.run("peek/close") }
            control("arrow.up.forward", "Open in a Tab") { model.run("peek/tab") }
            control("rectangle.split.2x1", "Open in Split View") { model.run("peek/split") }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    private func control(_ symbol: String, _ label: String, action: @escaping () -> Void)
        -> some View
    {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .frame(width: peekControl, height: peekControl)
                .background(.regularMaterial, in: Circle())
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .help(label)
        .accessibilityLabel(label)
    }
}

@_cdecl("arbalete_ui_empty_card")
public func placeEmptyCard(
    _ id: UInt32, _ shown: Bool, _ x: Double, _ y: Double, _ width: Double, _ height: Double
) {
    MainActor.assumeIsolated {
        let card = shown ? NSRect(x: x, y: y, width: width, height: height) : nil
        if nativeWindows[id]?.model.emptyCard != card { nativeWindows[id]?.model.emptyCard = card }
    }
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
