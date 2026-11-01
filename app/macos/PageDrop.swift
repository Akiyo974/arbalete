// A tab dragged from the sidebar onto the page splits it, like Arc's: dropped on an edge of a
// page shown alone, it opens beside it, side by side or one above the other; dropped on a page
// of a split, it joins the split beside that page. A tinted card shows where it would go. Like
// Arc's too, any drag from the sidebar resting just beside it shows the previous or next Space.

import AppKit
import Combine
import SwiftUI

/// Where a tab dropped on the page goes: beside the tab `tab`, before it with `before`, the pages
/// one above the other with `stacked`; `preview` is the room it would take, in the drop's view.
struct PageDropTarget: Equatable {
    var tab: Int
    var before: Bool
    var stacked: Bool
    var preview: NSRect
}

/// The card's margin from the page's room (CARD_MARGIN in app/src/shell.rs).
private let cardMargin: CGFloat = 10
/// Like Arc's, the room beside the sidebar where a drag shows the previous Space, at its left,
/// or the next one, at its right.
private let leadingEdge: CGFloat = 80
private let trailingEdge: CGFloat = 30

/// The drop over the page, shown while a tab is dragged from the sidebar.
@MainActor
final class PageDrop {
    private let parent: NSWindow
    private let model: AppModel
    private let view = DropView()
    private let panel: ChildPanel
    private let edge = EdgeView()
    private let edgePanel: ChildPanel
    private var watch: AnyCancellable?
    private var timer: Timer?
    /// The docked sidebar, from the window's top left corner, if it shows.
    var sidebar: NSRect?
    /// The split's pages shown, from the card's top left corner.
    var panes: [Pane] = []

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        self.model = model
        panel = ChildPanel(view: view)
        panel.ignoresMouseEvents = false
        edgePanel = ChildPanel(view: edge)
        edgePanel.ignoresMouseEvents = false
        edge.registerForDraggedTypes([.string])
        edge.rested = { model.edge($0 ? true : nil) }
        view.registerForDraggedTypes([.string])
        view.locate = { [unowned self] point in target(at: point) }
        view.drop = { [unowned self] target in drop(target) }
        // Once the row's drag has said whether it carries a tab.
        watch = model.$dragged.sink { [weak self] _ in
            DispatchQueue.main.async { self?.update() }
        }
    }

    func close() {
        timer?.invalidate()
        panel.close()
        edgePanel.close()
    }

    private func update() {
        timer?.invalidate()
        timer = nil
        guard model.dragged != nil, let content = parent.contentView else {
            panel.place(in: parent, nil)
            edgePanel.place(in: parent, nil)
            return
        }
        view.tint = NSColor(model.themeColor)
        view.show(nil)
        let left = sidebar?.maxX ?? 0
        panel.place(
            in: parent,
            NSRect(
                x: left, y: 0, width: max(0, content.bounds.width - left),
                height: content.bounds.height))
        edgePanel.place(
            in: parent,
            sidebar.map {
                NSRect(x: $0.minX - leadingEdge, y: $0.minY, width: leadingEdge, height: $0.height)
            })
        // A drag that ends anywhere else leaves the mouse's buttons up.
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard NSEvent.pressedMouseButtons == 0, let self else { return }
                self.model.dragged = nil
                self.model.dropTarget = nil
            }
        }
    }

    /// Where a tab dropped at `point`, from the view's top left corner, would go.
    private func target(at point: NSPoint) -> PageDropTarget? {
        // Just beside the docked sidebar, the next Space shows.
        let beside = sidebar != nil && point.x < trailingEdge
        model.edge(beside ? false : nil)
        guard !beside, model.draggedTab, let dragged = model.dragged else { return nil }
        if panes.count > 1 {
            guard panes.count < 4, !panes.contains(where: { $0.tab == dragged }) else { return nil }
            let stacked = panes[0].x == panes[1].x
            for pane in panes {
                let frame = NSRect(
                    x: pane.x + cardMargin, y: pane.y + cardMargin, width: pane.width,
                    height: pane.height)
                guard frame.contains(point) else { continue }
                let before = stacked ? point.y < frame.midY : point.x < frame.midX
                var half = frame
                if stacked {
                    half.size.height /= 2
                    if !before { half.origin.y += half.height }
                } else {
                    half.size.width /= 2
                    if !before { half.origin.x += half.width }
                }
                return PageDropTarget(tab: pane.tab, before: before, stacked: stacked, preview: half)
            }
            return nil
        }
        guard let selected = model.page?.selected, selected != dragged else { return nil }
        // Arc's edges: up to 50 points at the top and the bottom, then 45% at either side.
        let room = view.bounds.insetBy(dx: cardMargin, dy: cardMargin)
        let band = min(room.height / 3, 50)
        let side = room.width * 0.45
        let (top, bottom) = (point.y < room.minY + band, point.y > room.maxY - band)
        let (leading, trailing) = (point.x < room.minX + side, point.x > room.maxX - side)
        guard top || bottom || leading || trailing else { return nil }
        let stacked = top || bottom
        let before = stacked ? top : leading
        var half = room
        if stacked {
            half.size.height /= 2
            if !before { half.origin.y += half.height }
        } else {
            half.size.width /= 2
            if !before { half.origin.x += half.width }
        }
        return PageDropTarget(tab: selected, before: before, stacked: stacked, preview: half)
    }

    private func drop(_ target: PageDropTarget) {
        guard let dragged = model.dragged else { return }
        model.split(dragged, onto: target.tab, before: target.before)
        // A split made of a page shown alone lays its pages out as the drop asked.
        if target.stacked, panes.count < 2 { model.run("pane/flip") }
        model.dragged = nil
        model.dropTarget = nil
    }
}

/// The room left of the sidebar, where a drag shows the previous Space; it takes no drop.
private final class EdgeView: NSView {
    var rested: (Bool) -> Void = { _ in }

    override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
        draggingUpdated(sender)
    }

    override func draggingUpdated(_ sender: any NSDraggingInfo) -> NSDragOperation {
        rested(true)
        return []
    }

    override func draggingExited(_ sender: (any NSDraggingInfo)?) {
        rested(false)
    }
}

/// The view taking the drop, which draws where the tab would go.
private final class DropView: NSView {
    var locate: (NSPoint) -> PageDropTarget? = { _ in nil }
    var drop: (PageDropTarget) -> Void = { _ in }
    var tint = NSColor.controlAccentColor
    private var shown: PageDropTarget?

    override var isFlipped: Bool { true }

    func show(_ shown: PageDropTarget?) {
        guard shown != self.shown else { return }
        self.shown = shown
        needsDisplay = true
    }

    override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
        draggingUpdated(sender)
    }

    override func draggingUpdated(_ sender: any NSDraggingInfo) -> NSDragOperation {
        show(locate(convert(sender.draggingLocation, from: nil)))
        return shown == nil ? [] : .move
    }

    override func draggingExited(_ sender: (any NSDraggingInfo)?) {
        show(nil)
    }

    override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
        guard let shown else { return false }
        show(nil)
        drop(shown)
        return true
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let shown else { return }
        tint.withAlphaComponent(0.5).setFill()
        NSBezierPath(roundedRect: shown.preview.insetBy(dx: 4, dy: 4), xRadius: 6, yRadius: 6)
            .fill()
    }
}
