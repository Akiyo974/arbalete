// Dragging rows in the sidebar, like Arc's, whose tab list is an outline view with AppKit's own
// feedback: a thin line where the row would go, a folder tinted when dropped into, and a tab
// splitting in two cards when one rests on its middle. The rows stay in place meanwhile.

import AppKit
import SwiftUI

/// Where a drag over the sidebar's rows would land.
enum DropTarget: Equatable {
    /// Before or after a row, by its id.
    case before(Int)
    case after(Int)
    /// Into a folder, after what it holds.
    case into(Int)
    /// Beside a tab, in a split: before it with `leading`.
    case split(Int, leading: Bool)
    /// Into a split, at an index among its tabs.
    case join(Int, Int)
}

/// How long the pointer rests on a tab's middle before the tab offers a split, as in Arc.
private let splitDelay = 0.2

/// Like Arc's, a drag resting this long within `edgeSlop` points at the sidebar's edge shows the
/// previous or next Space; the edge is `edgeBand` points wide.
private let edgeDelay = 0.35
private let edgeSlop: CGFloat = 3
let edgeBand: CGFloat = 20

extension AppModel {
    /// The color that marks where a drop lands and the focused pane, like Arc's: the one standing
    /// out most on the theme (maxContrastColor); without colors, Arc's lavender in a dark window,
    /// measured, and its indigo in a light one.
    var themeColor: Color {
        guard let theme = space?.theme, let lead = theme.colors.first else {
            return color(systemDark() ? "#e2e1f3" : "#333ce8")
        }
        return color(ThemePalette(lead).strongest(dark: isDark(theme)))
    }

    /// The color around the focused page of a split: the theme's, or without colors, Arc's blue,
    /// a brighter one in the dark, measured.
    var focusColor: Color {
        guard space?.theme.colors.isEmpty ?? true else { return themeColor }
        // Drawn as #3139fb is on Arc's, once the display's colors take their share.
        return color(systemDark() ? "#2f38ff" : "#333ce8")
    }

    /// The color of a Space's main buttons, like Arc's: its theme's, or its blue without colors.
    var accent: Color {
        (space?.theme.colors.isEmpty ?? true) ? color("#333ce8") : themeColor
    }

    /// The selected tab's row, like Arc's: in the dark, it falls to a row at rest's while another
    /// app is in front, measured.
    var selectedInk: Color {
        active ? Ink.selected : Ink.selectedBehind
    }

    /// Its shadow, which would show through the dimmed row.
    var selectedShadow: Color {
        active ? Ink.selectedShadow : .clear
    }

    /// The sidebar's buttons, like Arc's: dimmed alike while another app is in front, measured.
    var buttonInk: AnyShapeStyle {
        active ? AnyShapeStyle(.primary) : AnyShapeStyle(Ink.buttonBehind)
    }

    /// The tabs' titles, like Arc's: the theme's title color, or the sidebar's ink without colors.
    var titleInk: Color {
        guard let theme = space?.theme, let lead = theme.colors.first else { return Ink.primary }
        let titles = ThemePalette(lead).titles
        return color(isDark(theme) ? titles.dark : titles.light)
    }

    /// A click on a tab, like Arc's: with ⌘ it joins the tabs selected at once or leaves them,
    /// with ⇧ the tabs from the one shown to it are; otherwise it is selected alone.
    func click(_ id: Int) {
        let flags = NSEvent.modifierFlags
        let shown = activeTab?.id
        if flags.contains(.command) {
            var marked = self.marked.isEmpty ? [shown].compactMap { $0 } : self.marked
            if let index = marked.firstIndex(of: id) {
                marked.remove(at: index)
            } else {
                marked.append(id)
            }
            self.marked = marked.count > 1 ? marked : []
            return
        }
        if flags.contains(.shift), let shown, let from = order.firstIndex(of: shown),
            let to = order.firstIndex(of: id), from != to
        {
            marked = Array(order[min(from, to)...max(from, to)])
            return
        }
        marked = []
        run("select", id)
    }

    /// The tabs a drag or a menu from the tab `id` acts on, in the sidebar's order: the ones
    /// selected at once, when it is one of them.
    func acting(on id: Int) -> [Int] {
        marked.contains(id) ? order.filter(marked.contains) : [id]
    }

    /// The displayed Space's tabs, top to bottom.
    private var order: [Int] {
        guard let page else { return [] }
        return (page.pinned + page.today).flatMap(\.tabs).map(\.id)
    }

    /// A row starts a drag: the node it carries, and whether it is a tab, which may split.
    func startDrag(_ item: Item) -> NSItemProvider {
        dragged = item.id
        draggedItem = item
        if case .tab = item { draggedTab = true } else { draggedTab = false }
        draggedTabs = draggedTab ? acting(on: item.id) : [item.id]
        return NSItemProvider(object: String(item.id) as NSString)
    }

    /// The pointer moves over a row: a split shows once it rests on a tab; until then, the drop
    /// lands where `nearest` says, like Arc's: after the tab in its bottom band, else before.
    func hover(_ target: DropTarget?, nearest: DropTarget? = nil) {
        // A row may still hear of a drag once it has ended.
        guard dragged != nil else {
            dropTarget = nil
            return
        }
        guard case .split(let tab, let leading) = target else {
            splitWait?.cancel()
            splitWait = nil
            dropTarget = target
            return
        }
        if case .split(tab, _) = dropTarget {
            dropTarget = .split(tab, leading: leading)
            return
        }
        pendingSplit = .split(tab, leading: leading)
        dropTarget = nearest
        guard splitWait == nil else { return }
        let wait = DispatchWorkItem { [weak self] in
            guard let self else { return }
            splitWait = nil
            if case .split(tab, _) = pendingSplit { dropTarget = pendingSplit }
        }
        splitWait = wait
        DispatchQueue.main.asyncAfter(deadline: .now() + splitDelay, execute: wait)
    }

    /// The pointer leaves the row `id`: its feedback goes, unless another row's took over.
    func leave(_ id: Int) {
        switch dropTarget {
        case .before(id), .after(id), .into(id), .split(id, _), .join(id, _): dropTarget = nil
        default: break
        }
        if case .split(id, _) = pendingSplit {
            splitWait?.cancel()
            splitWait = nil
            pendingSplit = nil
        }
    }

    /// A drag rests at the sidebar's leading edge, its trailing one, or neither (nil): after a
    /// while, the previous or the next Space shows, so that the drag carries on into it.
    func edge(_ leading: Bool?) {
        guard let leading else {
            edgeWait?.cancel()
            edgeWait = nil
            edgeAt = nil
            return
        }
        let here = NSEvent.mouseLocation
        if let at = edgeAt, hypot(here.x - at.x, here.y - at.y) <= edgeSlop { return }
        edgeAt = here
        edgeWait?.cancel()
        let wait = DispatchWorkItem { [weak self] in
            guard let self, NSEvent.pressedMouseButtons != 0 else { return }
            edgeWait = nil
            // Like Arc's, held there, it switches again every `edgeDelay`.
            edgeAt = nil
            run(leading ? "previous-space" : "next-space")
        }
        edgeWait = wait
        DispatchQueue.main.asyncAfter(deadline: .now() + edgeDelay, execute: wait)
    }

    /// The drag ends: what it carried, and where it landed, if anywhere.
    func endDrag() -> (Int, DropTarget)? {
        edge(nil)
        splitWait?.cancel()
        splitWait = nil
        pendingSplit = nil
        defer {
            dragged = nil
            dropTarget = nil
        }
        guard let dragged, let dropTarget else { return nil }
        return (dragged, dropTarget)
    }
}

/// A row of the sidebar's tabs, folders and splits, which a drag may land before, after, into or
/// beside, like Arc's.
struct DropRow<Content: View>: View {
    @ObservedObject var model: AppModel
    let item: Item
    /// Moves the node dropped before the row (0) or after it (1), among its parent's children.
    let place: (Int, Int) -> Void
    @ViewBuilder let content: Content
    private let _size = State<CGSize>(initialValue: .zero)

    var body: some View {
        content
            .onGeometryChange(for: CGSize.self) { $0.size } action: { _size.wrappedValue = $0 }
            .background {
                if model.dropTarget == .into(item.id) {
                    // Like Arc's, across the whole sidebar.
                    Rectangle().fill(model.themeColor.opacity(0.5))
                        .padding(.horizontal, -Measure.inset)
                }
            }
            .overlay(alignment: .top) {
                if model.dropTarget == .before(item.id) {
                    InsertionLine(tint: model.themeColor).offset(y: -4)
                }
            }
            .overlay(alignment: .bottom) {
                if model.dropTarget == .after(item.id) {
                    InsertionLine(tint: model.themeColor).offset(y: 4)
                }
            }
            .onDrag { model.startDrag(item) }
            .onDrop(
                of: [.plainText],
                delegate: RowDrop(model: model, item: item, size: _size.wrappedValue, place: place))
    }
}

/// The tab list's room below its rows, like Arc's: what a drag carries goes after today's last
/// tab, which shows the line under it.
struct TailDrop: DropDelegate {
    let model: AppModel
    let page: SpacePage
    /// The tab list's width, whose edges switch Spaces.
    let width: CGFloat

    func dropUpdated(info: DropInfo) -> DropProposal? {
        let x = info.location.x
        model.edge(x <= edgeBand ? true : x >= width - edgeBand ? false : nil)
        // Without today's tabs, the line shows under New Tab.
        model.hover(page.today.last.map { .after($0.id) } ?? .into(page.roots.today))
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) {
        if let last = page.today.last { model.leave(last.id) } else { model.hover(nil) }
    }

    func performDrop(info: DropInfo) -> Bool {
        // Only what a drag from the sidebar carries.
        guard model.dragged != nil else { return false }
        let ids = model.draggedTabs
        model.marked = []
        _ = model.endDrag()
        ids.enumerated().forEach { model.move($1, to: page.roots.today, page.today.count + $0) }
        return true
    }
}

/// What a drag shows, changed while it is over a place that takes it, like Arc's tile over the
/// favorites or card over the page, and given back once it leaves.
final class DragShape {
    /// What each dragged item showed before it changed.
    private var rows: [(frame: NSRect, components: [NSDraggingImageComponent])] = []

    /// Shows `image`, `size` large, centered on the pointer, in `view`'s coordinates.
    func morph(_ sender: any NSDraggingInfo, in view: NSView, size: CGSize, image: NSImage) {
        let point = view.convert(sender.draggingLocation, from: nil)
        let keep = rows.isEmpty
        sender.enumerateDraggingItems(
            options: [], for: view, classes: [NSPasteboardItem.self], searchOptions: [:]
        ) { item, _, _ in
            if keep { self.rows.append((item.draggingFrame, item.imageComponents ?? [])) }
            item.setDraggingFrame(
                NSRect(
                    x: point.x - size.width / 2, y: point.y - size.height / 2,
                    width: size.width, height: size.height),
                contents: image)
        }
    }

    /// Gives the drag back what it showed.
    func restore(_ sender: any NSDraggingInfo, in view: NSView) {
        guard !rows.isEmpty else { return }
        var index = 0
        sender.enumerateDraggingItems(
            options: [], for: view, classes: [NSPasteboardItem.self], searchOptions: [:]
        ) { item, _, _ in
            guard index < self.rows.count else { return }
            let row = self.rows[index]
            index += 1
            guard !row.components.isEmpty else { return }
            item.draggingFrame = row.frame
            item.imageComponentsProvider = { row.components }
        }
        rows = []
    }

    /// Forgets what the drag showed, once it dropped.
    func forget() { rows = [] }
}

/// A Space's icon under a drag: resting there a moment slides to that Space, whose tabs then
/// take the drop. The icon itself takes none.
struct SpaceIconDrop: DropDelegate {
    let model: AppModel
    let space: Int

    func dropEntered(info: DropInfo) {
        guard model.dragged != nil else { return }
        model.spaceWait?.cancel()
        let go = DispatchWorkItem { [model, space] in
            if model.dragged != nil { model.slide(to: space) }
        }
        model.spaceWait = go
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: go)
    }

    func dropUpdated(info: DropInfo) -> DropProposal? { DropProposal(operation: .forbidden) }

    func dropExited(info: DropInfo) {
        model.spaceWait?.cancel()
        model.spaceWait = nil
    }

    func performDrop(info: DropInfo) -> Bool { false }
}

/// A section's edge taking a drop, like the divider or New Tab: what the drag carries goes at
/// `index` in `parent`, moved, the line showing where. Not SwiftUI's drop destination, which
/// copies, its pointer showing a "+".
struct SectionDrop: DropDelegate {
    let model: AppModel
    let parent: Int
    let index: Int
    /// Where the line shows.
    let shown: DropTarget?

    func dropUpdated(info: DropInfo) -> DropProposal? {
        model.hover(shown)
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) { model.hover(nil) }

    func performDrop(info: DropInfo) -> Bool {
        guard model.dragged != nil else { return false }
        let ids = model.draggedTabs
        model.marked = []
        _ = model.endDrag()
        ids.enumerated().forEach { model.move($1, to: parent, index + $0) }
        return true
    }
}

/// Where on a row a drag lands, as in Arc: a band at its top and bottom, a quarter of its height
/// but at most 8 points, inserts before or after it; its middle drops into a folder, or beside a
/// tab in a split, on the side of the row the pointer is.
private struct RowDrop: DropDelegate {
    let model: AppModel
    let item: Item
    let size: CGSize
    let place: (Int, Int) -> Void

    func dropUpdated(info: DropInfo) -> DropProposal? {
        // The row is inset from the sidebar's edges.
        let band = edgeBand - Measure.inset
        let x = info.location.x
        model.edge(x <= band ? true : x >= size.width - band ? false : nil)
        model.hover(target(at: info.location), nearest: fallback(at: info.location))
        return DropProposal(operation: NSEvent.modifierFlags.contains(.option) ? .copy : .move)
    }

    func dropExited(info: DropInfo) { model.leave(item.id) }

    func performDrop(info: DropInfo) -> Bool {
        let ids = model.draggedTabs
        model.marked = []
        guard let (id, target) = model.endDrag(), !ids.contains(item.id), id != item.id else {
            return false
        }
        switch target {
        // Several tabs keep their order: each goes before the row, or after it from the last.
        case .before: ids.forEach { place($0, 0) }
        case .after:
            // After an open folder is before what it holds.
            if case .folder(let folder, _, true, _) = item {
                ids.enumerated().forEach { model.move($1, to: folder, $0) }
            } else {
                ids.reversed().forEach { place($0, 1) }
            }
        case .into(let folder):
            if case .folder(_, _, _, let children) = item {
                ids.enumerated().forEach { model.move($1, to: folder, children.count + $0) }
            }
        case .split(let tab, let leading): model.split(id, onto: tab, before: leading)
        case .join(_, let index):
            guard case .split(_, let tabs) = item, !tabs.isEmpty else { return false }
            if index == 0 {
                model.split(id, onto: tabs[0].id, before: true)
            } else {
                model.split(id, onto: tabs[min(index, tabs.count) - 1].id)
            }
        }
        return true
    }

    /// Like Arc's, measured: a band at the top inserts before; at the bottom, after a folder;
    /// elsewhere, into a folder, or beside a tab in a split once it waited (splitDelay).
    private func target(at point: CGPoint) -> DropTarget? {
        let band = min(size.height / 4, 8)
        if point.y < band { return .before(item.id) }
        let bottom = point.y > size.height - band
        switch item {
        case .folder(let id, _, let open, let children) where !bottom || (open && !children.isEmpty):
            return model.dragged == id ? nil : .into(id)
        case .tab(let tab) where model.draggedTab && model.dragged != tab.id:
            return .split(tab.id, leading: point.x < size.width / 2)
        case .split(let id, let tabs)
        where model.draggedTab && tabs.count < 4 && !tabs.contains { $0.id == model.dragged }:
            // Between the tabs the pointer is closest to, as they share the row evenly.
            let slot = (point.x / max(size.width, 1) * CGFloat(tabs.count)).rounded()
            return .join(id, min(tabs.count, max(0, Int(slot))))
        default:
            return bottom ? .after(item.id) : .before(item.id)
        }
    }

    /// Where a drop lands until a split shows: after the row in its bottom band, else before.
    private func fallback(at point: CGPoint) -> DropTarget {
        point.y > size.height - min(size.height / 4, 8) ? .after(item.id) : .before(item.id)
    }
}

/// Where a row dropped would go, as AppKit draws it: a thin line from a small ring.
struct InsertionLine: View {
    let tint: Color

    var body: some View {
        HStack(spacing: 0) {
            Circle().strokeBorder(tint, lineWidth: 2).frame(width: 8, height: 8)
            Rectangle().fill(tint).frame(height: 2)
        }
        .padding(.leading, 4)
        .allowsHitTesting(false)
    }
}

/// Where a tab dropped on a split would join it: an empty card, tinted, among its tabs.
struct DropChip: View {
    let tint: Color

    var body: some View {
        RoundedRectangle(cornerRadius: 8).fill(tint.opacity(0.5))
            .frame(maxHeight: .infinity)
            .frame(maxWidth: .infinity)
    }
}

/// Over a tab that a tab resting on its middle would join in a split, as in Arc: the row stays,
/// and a light chip shows on the half the dropped tab would take, where the split row puts it.
struct SplitHalf: View {
    let leading: Bool

    var body: some View {
        HStack(spacing: 4) {
            if !leading { Color.clear }
            RoundedRectangle(cornerRadius: 10).fill(Ink.splitDrop).frame(height: 28)
            if leading { Color.clear }
        }
        .padding(3)
        .frame(height: Measure.rowFill)
        .padding(.vertical, (Measure.row - Measure.rowFill) / 2)
        .allowsHitTesting(false)
    }
}

/// A tile of the favorites' grid while a drag goes on: a favorite, or the empty one opening for
/// a tab or a split from elsewhere.
struct FavoriteSlot: Hashable {
    let item: Item?

    static func == (lhs: Self, rhs: Self) -> Bool { lhs.item?.id == rhs.item?.id }
    func hash(into hasher: inout Hasher) { hasher.combine(item?.id) }
}

/// The favorites taking a drag, like Arc's: under the pointer, what the drag carries turns from
/// its row into a tile of the favorites, and back into its row once it leaves them. AppKit's
/// own, since SwiftUI's drops cannot change what a drag shows.
struct FavoritesTarget: NSViewRepresentable {
    let model: AppModel
    /// The tile it turns into.
    let tile: CGSize
    /// Where the pointer is, from the view's top left corner, or nil once it leaves.
    let hover: (CGPoint?) -> Void
    /// Drops the node with this id, which the drag carries.
    let drop: (Int) -> Bool

    func makeNSView(context: Context) -> Target {
        let view = Target()
        view.registerForDraggedTypes([.string])
        return view
    }

    func updateNSView(_ view: Target, context: Context) {
        view.model = model
        view.tile = tile
        view.hover = hover
        view.drop = drop
    }

    final class Target: NSView {
        var model: AppModel?
        var tile = CGSize.zero
        var hover: (CGPoint?) -> Void = { _ in }
        var drop: (Int) -> Bool = { _ in false }
        private let shape = DragShape()

        override var isFlipped: Bool { true }

        /// Clicks go through to the tiles under it: only drags stop here.
        override func hitTest(_ point: NSPoint) -> NSView? {
            switch NSApp.currentEvent?.type {
            case .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .mouseMoved:
                return nil
            default:
                return super.hitTest(point)
            }
        }

        /// The node the drag carries, from its pasteboard: the model may forget its drag as the
        /// mouse button comes up, before the drop.
        private func carried(_ sender: any NSDraggingInfo) -> Int? {
            sender.draggingPasteboard.string(forType: .string).flatMap { Int($0) }
        }

        /// Like Arc's, the favorites take tabs and splits, not folders.
        private func takes(_ sender: any NSDraggingInfo) -> Bool {
            guard let id = carried(sender), let item = model?.item(id) else { return false }
            if case .folder = item { return false }
            return true
        }

        override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
            guard takes(sender) else { return [] }
            shape.morph(sender, in: self, size: tile, image: tileImage(carried(sender)))
            return draggingUpdated(sender)
        }

        override func draggingUpdated(_ sender: any NSDraggingInfo) -> NSDragOperation {
            guard takes(sender) else { return [] }
            hover(convert(sender.draggingLocation, from: nil))
            return .move
        }

        override func draggingExited(_ sender: (any NSDraggingInfo)?) {
            hover(nil)
            if let sender { shape.restore(sender, in: self) }
        }

        override func prepareForDragOperation(_ sender: any NSDraggingInfo) -> Bool {
            takes(sender)
        }

        override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
            shape.forget()
            guard let id = carried(sender) else { return false }
            return drop(id)
        }

        /// A favorite's tile for what the drag carries: its icons on the tile's fill, like Arc's.
        private func tileImage(_ id: Int?) -> NSImage {
            let tabs = id.flatMap { model?.item($0) }?.tabs ?? []
            let size = tile
            return NSImage(size: size, flipped: true) { rect in
                let shape = NSBezierPath(
                    roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: Measure.radius,
                    yRadius: Measure.radius)
                NSColor.white.withAlphaComponent(0.35).setFill()
                shape.fill()
                NSColor.white.withAlphaComponent(0.6).setStroke()
                shape.lineWidth = 1
                shape.stroke()
                let icon: CGFloat = 20
                let gap: CGFloat = 8
                let width = CGFloat(tabs.count) * icon + CGFloat(max(0, tabs.count - 1)) * gap
                var x = (size.width - width) / 2
                for tab in tabs {
                    uiImage(tab.favicon)?.draw(
                        in: NSRect(x: x, y: (size.height - icon) / 2, width: icon, height: icon))
                    x += icon + gap
                }
                return true
            }
        }
    }
}
