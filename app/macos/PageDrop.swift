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
    /// Like Arc's, it takes the tab's place in the split, which the tab leaves.
    var replace = false
    /// Where among the split's pages it would open.
    var index: Int?
}

/// The card's margin from the page's room, and the room between a split's pages (CARD_MARGIN
/// and SPLIT_GAP in app/src/shell.rs).
private let cardMargin: CGFloat = 10
private let splitGap: CGFloat = 10
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
    /// The sidebar, docked or floating, from the window's top left corner, if it shows.
    var sidebar: NSRect?
    /// The split's pages shown, from the card's top left corner.
    var panes: [Pane] = [] {
        didSet { if previewed == "pane/preview/none" { stable = panes } }
    }
    /// The pages as they stood before an empty one opened among them for a drag, which the drag
    /// keeps finding its places by: the pages move to make room meanwhile.
    private var stable: [Pane] = []

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
        view.shown = { [unowned self] target in preview(target) }
        view.card = { [unowned self] in cardImage() }
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

    /// Like Arc's, the page shown alone gives a tab dragged over it the room it would take.
    /// Over a split, like Arc's, an empty page opens among its pages where the tab would go.
    private func preview(_ target: PageDropTarget?) {
        let page: String
        if let target, stable.count < 2 {
            page = "page/preview/\(target.before ? "before" : "after")/"
                + (target.stacked ? "stacked" : "side")
        } else {
            page = "page/preview/none"
        }
        var moved = false
        if page != previewedPage {
            previewedPage = page
            model.run(page)
            moved = true
        }
        let pane = target?.index.map { "pane/preview/\($0)" } ?? "pane/preview/none"
        if pane != previewed {
            previewed = pane
            model.run(pane)
            moved = true
        }
        // The pages laid out again, CEF may order their window above the drop's, which would
        // take the drag from it: the drop goes back on top.
        if moved { DispatchQueue.main.async { [weak self] in self?.raise() } }
    }

    /// Puts the drop back above the window's other child windows, the pages' among them.
    private func raise() {
        guard panel.isVisible, parent.childWindows?.last !== panel else { return }
        parent.removeChildWindow(panel)
        parent.addChildWindow(panel, ordered: .above)
    }
    private var previewedPage = "page/preview/none"
    /// The card's frame, from the window's top left corner, before it gave a drag room.
    private var stableCard: NSRect?
    private var previewed = "pane/preview/none"

    /// What a drag shows over the page, like Arc's: a dark card with the tab's icon and title.
    private func cardImage() -> NSImage {
        let tab = model.dragged.flatMap { model.item($0) }?.tabs.first
        let size = cardSize
        return NSImage(size: size, flipped: true) { rect in
            let shape = NSBezierPath(roundedRect: rect, xRadius: 14, yRadius: 14)
            NSColor.black.withAlphaComponent(0.92).setFill()
            shape.fill()
            let icon: CGFloat = 20
            let top = rect.midY - 28
            uiImage(tab?.favicon)?.draw(
                in: NSRect(x: rect.midX - icon / 2, y: top, width: icon, height: icon))
            let style = NSMutableParagraphStyle()
            style.alignment = .center
            style.lineBreakMode = .byTruncatingTail
            NSString(string: tab?.label ?? "").draw(
                with: NSRect(x: 8, y: top + icon + 6, width: rect.width - 16, height: 44),
                options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine],
                attributes: [
                    .font: NSFont.systemFont(ofSize: 12, weight: .semibold),
                    .foregroundColor: NSColor.white,
                    .paragraphStyle: style,
                ])
            return true
        }
    }

    private func update() {
        timer?.invalidate()
        timer = nil
        guard model.dragged != nil, let content = parent.contentView else {
            preview(nil)
            panel.place(in: parent, nil)
            edgePanel.place(in: parent, nil)
            return
        }
        view.tint = NSColor(model.accent)
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
        // A drag that ends anywhere else leaves the mouse's buttons up; a drop takes a moment
        // after they come up, which must still find the drag.
        var released: Date?
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard NSEvent.pressedMouseButtons == 0, let self else {
                    released = nil
                    return
                }
                let since = released ?? Date()
                released = since
                guard Date().timeIntervalSince(since) >= 0.5 else { return }
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
        let panes = stable
        if panes.count > 1 {
            guard !panes.contains(where: { $0.tab == dragged }) else { return nil }
            let stacked = panes[0].x == panes[1].x
            let frames = panes.map {
                NSRect(x: $0.x + cardMargin, y: $0.y + cardMargin, width: $0.width, height: $0.height)
            }
            for (index, frame) in frames.enumerated() {
                guard frame.contains(point) else { continue }
                // Like Arc's: its first quarter along the split puts the tab before it, its last
                // after it, which between two pages is the same place, and its middle replaces it.
                let share = stacked
                    ? (point.y - frame.minY) / frame.height : (point.x - frame.minX) / frame.width
                if share > 0.25 && share < 0.75 {
                    return PageDropTarget(
                        tab: panes[index].tab, before: true, stacked: stacked, preview: frame,
                        replace: true)
                }
                // A split holds four pages at most.
                guard panes.count < 4 else { return nil }
                let before = share <= 0.25
                let at = index + (before ? 0 : 1)
                // The empty page the app opens there, the pages sharing the room evenly with it.
                let room = frames.reduce(frames[0]) { $0.union($1) }
                let count = CGFloat(panes.count + 1)
                var slot = room
                if stacked {
                    slot.size.height = (room.height - splitGap * (count - 1)) / count
                    slot.origin.y = room.minY + CGFloat(at) * (slot.height + splitGap)
                } else {
                    slot.size.width = (room.width - splitGap * (count - 1)) / count
                    slot.origin.x = room.minX + CGFloat(at) * (slot.width + splitGap)
                }
                return PageDropTarget(
                    tab: panes[index].tab, before: before, stacked: stacked, preview: slot,
                    index: at)
            }
            return nil
        }
        guard let selected = model.page?.selected, selected != dragged else { return nil }
        // Arc's edges: up to 50 points at the top and the bottom, then 45% at either side, of
        // the card as it stood before it gave the tab room, in the view's coordinates.
        if previewedPage == "page/preview/none" { stableCard = model.card }
        let room =
            stableCard?.offsetBy(dx: -(sidebar?.maxX ?? 0), dy: 0)
            ?? view.bounds.insetBy(dx: cardMargin, dy: cardMargin)
        let band = min(room.height / 3, 50)
        let side = room.width * 0.45
        let (top, bottom) = (point.y < room.minY + band, point.y > room.maxY - band)
        let (leading, trailing) = (point.x < room.minX + side, point.x > room.maxX - side)
        guard top || bottom || leading || trailing else { return nil }
        let stacked = top || bottom
        let before = stacked ? top : leading
        // The half the page gives up, as the app lays it out (SPLIT_GAP, 10).
        var half = room
        if stacked {
            half.size.height = (room.height - splitGap) / 2
            if !before { half.origin.y = room.maxY - half.height }
        } else {
            half.size.width = (room.width - splitGap) / 2
            if !before { half.origin.x = room.maxX - half.width }
        }
        return PageDropTarget(tab: selected, before: before, stacked: stacked, preview: half)
    }

    private func drop(_ target: PageDropTarget) {
        preview(nil)
        guard let dragged = model.dragged else { return }
        // A full split's page replaced leaves first, the tab joining beside its neighbor.
        if target.replace, stable.count >= 4,
            let index = stable.firstIndex(where: { $0.tab == target.tab })
        {
            let next = index + 1 < stable.count
            model.run("separate/\(target.tab)")
            model.split(dragged, onto: stable[next ? index + 1 : index - 1].tab, before: next)
        } else {
            model.split(dragged, onto: target.tab, before: target.before)
            // Replaced, the tab leaves the split, as a row after it.
            if target.replace { model.run("separate/\(target.tab)") }
        }
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
    /// Called as where the tab would go changes.
    var shown: (PageDropTarget?) -> Void = { _ in }
    /// What the drag shows over the page.
    var card: () -> NSImage = { NSImage() }
    var tint = NSColor.controlAccentColor
    private var target: PageDropTarget?
    private let shape = DragShape()

    override var isFlipped: Bool { true }

    func show(_ target: PageDropTarget?) {
        guard target != self.target else { return }
        self.target = target
        shown(target)
        needsDisplay = true
    }

    override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
        draggingUpdated(sender)
    }

    override func draggingUpdated(_ sender: any NSDraggingInfo) -> NSDragOperation {
        show(locate(convert(sender.draggingLocation, from: nil)))
        // Like Arc's, over the page the drag turns into a card, following the pointer.
        if target != nil {
            shape.morph(sender, in: self, size: cardSize, image: card())
        } else {
            shape.restore(sender, in: self)
        }
        return target == nil ? [] : .move
    }

    override func draggingExited(_ sender: (any NSDraggingInfo)?) {
        show(nil)
        if let sender { shape.restore(sender, in: self) }
    }

    override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
        shape.forget()
        guard let target else { return false }
        show(nil)
        drop(target)
        return true
    }

    /// Like Arc's, the room the tab would take shows as an empty page, outlined; a page it would
    /// replace dims under it.
    override func draw(_ dirtyRect: NSRect) {
        guard let target else { return }
        // A page it would replace takes an outline in the theme's color, like Arc's.
        if target.replace {
            let page = NSBezierPath(
                roundedRect: target.preview.insetBy(dx: 1, dy: 1), xRadius: 10, yRadius: 10)
            tint.setStroke()
            page.lineWidth = 2
            page.stroke()
            return
        }
        let page = NSBezierPath(
            roundedRect: target.preview.insetBy(dx: 0.75, dy: 0.75), xRadius: 10, yRadius: 10)
        NSColor.white.withAlphaComponent(0.04).setFill()
        page.fill()
        NSColor.white.withAlphaComponent(0.35).setStroke()
        page.lineWidth = 1.5
        page.stroke()
    }
}

/// The card a drag shows over the page, about a quarter of the window high like Arc's.
private let cardSize = CGSize(width: 92, height: 200)
