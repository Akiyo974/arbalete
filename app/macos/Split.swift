// The handles between a split's pages, which resize them, like Arc's Split View: the app places
// them in the room between the pages (place_split_handles in app/src/shell.rs). The pages are
// windows of their own above the main one's, so each handle is a child window too.

import AppKit

/// A handle's look: a short pill, shown while the pointer is over it.
private let pillLength: CGFloat = 24
private let pillWidth: CGFloat = 2

/// Moves the room between two pages of a split, which a double click shares evenly.
final class SplitHandleView: NSView {
    var index = 0
    var stacked = false
    var send: (String) -> Void = { _ in }
    private let pill = CALayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        pill.backgroundColor = NSColor.labelColor.withAlphaComponent(0.35).cgColor
        pill.cornerRadius = 1
        pill.isHidden = true
        layer?.addSublayer(pill)
    }

    required init?(coder: NSCoder) { nil }

    override func layout() {
        super.layout()
        let size = stacked
            ? NSSize(width: pillLength, height: pillWidth)
            : NSSize(width: pillWidth, height: pillLength)
        pill.frame = NSRect(
            x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2,
            width: size.width, height: size.height)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: bounds, options: [.mouseEnteredAndExited, .activeAlways, .cursorUpdate],
                owner: self))
    }

    override func mouseEntered(with event: NSEvent) { pill.isHidden = false }
    override func mouseExited(with event: NSEvent) { pill.isHidden = true }

    override func cursorUpdate(with event: NSEvent) {
        (stacked ? NSCursor.resizeUpDown : NSCursor.resizeLeftRight).set()
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 { send("pane/even/\(index)") }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let parent = window?.parent, let content = parent.contentView else { return }
        // In the main window's points from its content's top left corner, like the app's.
        let point = parent.convertPoint(fromScreen: NSEvent.mouseLocation)
        let along = stacked ? content.frame.height - point.y : point.x
        send("pane/resize/\(index)/\(along)")
    }
}

/// The handles of the split a window shows, made as it needs them.
@MainActor
final class SplitHandles {
    private let parent: NSWindow
    private let model: AppModel
    private var panels: [ChildPanel] = []

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        self.model = model
    }

    /// Shows a handle at each frame, in the window's points from its top left corner, and hides
    /// the others.
    func place(stacked: Bool, frames: [NSRect]) {
        while panels.count < frames.count {
            let view = SplitHandleView(frame: .zero)
            view.send = { [weak model] path in model?.run(path) }
            panels.append(ChildPanel(view: view))
        }
        for (index, panel) in panels.enumerated() {
            let frame = index < frames.count ? frames[index] : nil
            if let view = panel.contentView as? SplitHandleView {
                view.index = index
                view.stacked = stacked
                view.needsLayout = true
            }
            panel.place(in: parent, frame)
        }
        // Above the pages, which CEF may order above them once it shows the card again.
        DispatchQueue.main.async { [self] in
            for panel in panels where panel.isVisible {
                parent.removeChildWindow(panel)
                parent.addChildWindow(panel, ordered: .above)
            }
        }
    }

    func close() { panels.forEach { $0.close() } }
}

@_cdecl("arbalete_ui_split")
public func placeSplitHandles(
    _ id: UInt32, _ stacked: Bool, _ frames: UnsafePointer<Double>?, _ count: Int
) {
    MainActor.assumeIsolated {
        let values = frames.map { Array(UnsafeBufferPointer(start: $0, count: count * 4)) } ?? []
        let rects = (0..<count).map {
            NSRect(
                x: values[$0 * 4], y: values[$0 * 4 + 1], width: values[$0 * 4 + 2],
                height: values[$0 * 4 + 3])
        }
        nativeWindows[id]?.splitHandles.place(stacked: stacked, frames: rects)
    }
}
