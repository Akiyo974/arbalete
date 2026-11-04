// The handles between a split's pages, which resize them, like Arc's Split View: the app places
// them in the room between the pages (place_split_handles in app/src/shell.rs). They are views of
// the card's stage, above the pages' containers.

import AppKit

/// A handle's look, like Arc's: a short thin pill, shown while the pointer is over it or drags it.
private let pillLength: CGFloat = 24
private let pillWidth: CGFloat = 2
/// How far past the room between the pages a handle takes the pointer, over their edges.
private let reach: CGFloat = 6

/// Moves the room between two pages of a split, which a double click shares evenly.
final class SplitHandleView: NSView {
    var index = 0
    var stacked = false
    var send: (String) -> Void = { _ in }
    private let pill = CALayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        pill.backgroundColor = NSColor.black.withAlphaComponent(0.4).cgColor
        pill.cornerRadius = pillWidth / 2
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
        // Over the whole handle, whatever size it takes since.
        addTrackingArea(
            NSTrackingArea(
                rect: .zero,
                options: [
                    .mouseEnteredAndExited, .mouseMoved, .cursorUpdate, .activeAlways,
                    .inVisibleRect,
                ],
                owner: self))
    }

    private var dragging = false
    private var hovered = false
    private var cursor: NSCursor { stacked ? .resizeUpDown : .resizeLeftRight }

    /// Shows the resize cursor, again once the card's window and the pages under the handle,
    /// which track the pointer too, have set theirs.
    private func showCursor() {
        cursor.set()
        DispatchQueue.main.async { [weak self] in
            guard let self, hovered || dragging else { return }
            cursor.set()
        }
    }

    override func mouseEntered(with event: NSEvent) {
        hovered = true
        // Between stacked pages, like Arc's, the grabber that rests there shows instead (the
        // backdrop's LoadingLine).
        pill.isHidden = stacked
        showCursor()
    }

    override func mouseMoved(with event: NSEvent) { showCursor() }

    override func mouseExited(with event: NSEvent) {
        hovered = false
        pill.isHidden = stacked || dragging
        if !dragging { NSCursor.arrow.set() }
    }

    override func mouseUp(with event: NSEvent) {
        if dragging { send("resized") }
        dragging = false
        let point = convert(event.locationInWindow, from: nil)
        pill.isHidden = stacked || !bounds.contains(point)
    }

    override func cursorUpdate(with event: NSEvent) { showCursor() }

    override func resetCursorRects() { addCursorRect(bounds, cursor: cursor) }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 2 { send("pane/even/\(index)") }
    }

    override func mouseDragged(with event: NSEvent) {
        dragging = true
        showCursor()
        guard let window, let content = window.contentView else { return }
        // In the window's points from its content's top left corner, like the app's.
        let point = window.convertPoint(fromScreen: NSEvent.mouseLocation)
        let along = stacked ? content.frame.height - point.y : point.x
        send("pane/resize/\(index)/\(along)")
    }
}

/// The handles of the split a window shows, made as it needs them.
@MainActor
final class SplitHandles {
    private let pages: PagePlacer
    private let model: AppModel
    private var views: [SplitHandleView] = []
    /// Where the handles go, in the window's points from its top left corner.
    private var frames: [NSRect] = []
    private var stacked = false

    init(pages: PagePlacer, model: AppModel) {
        self.pages = pages
        self.model = model
    }

    /// Shows a handle at each frame, in the window's points from its top left corner, and hides
    /// the others.
    func place(stacked: Bool, frames: [NSRect]) {
        self.stacked = stacked
        self.frames = frames
        arrange()
    }

    /// Lays the handles out on the card's stage, above the pages, while the card shows.
    func arrange() {
        while views.count < frames.count {
            let view = SplitHandleView(frame: .zero)
            view.send = { [weak model] path in model?.run(path) }
            views.append(view)
        }
        guard let holder = pages.holder else {
            views.forEach { $0.removeFromSuperview() }
            return
        }
        for (index, view) in views.enumerated() {
            guard index < frames.count else {
                view.removeFromSuperview()
                continue
            }
            view.index = index
            view.stacked = stacked
            // From the card's top left corner, as its stage counts.
            view.frame = frames[index].insetBy(dx: stacked ? 0 : -reach, dy: stacked ? -reach : 0)
                .offsetBy(dx: -pages.cardOrigin.x, dy: -pages.cardOrigin.y)
            view.needsLayout = true
            // Raised only below a page: raising it would end a drag of it.
            let order = holder.subviews
            let below =
                order.firstIndex(of: view).map { at in
                    order[(at + 1)...].contains { $0 is PageContainer }
                } ?? true
            if below { holder.addSubview(view, positioned: .above, relativeTo: nil) }
        }
    }

    func close() { views.forEach { $0.removeFromSuperview() } }
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
