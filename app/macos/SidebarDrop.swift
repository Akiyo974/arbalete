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

extension AppModel {
    /// The sidebar's theme color, which marks where a drop lands, like Arc's.
    /// The displayed Space's leading color, or Arc's indigo without one.
    var themeColor: Color { color(space?.theme.colors.first ?? "#333ce8") }

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
        run("select/\(id)")
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
        if case .tab = item { draggedTab = true } else { draggedTab = false }
        draggedTabs = draggedTab ? acting(on: item.id) : [item.id]
        return NSItemProvider(object: String(item.id) as NSString)
    }

    /// The pointer moves over a row: a split shows once it rests on a tab's middle.
    func hover(_ target: DropTarget?) {
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
        guard splitWait == nil else { return }
        dropTarget = nil
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

    /// The drag ends: what it carried, and where it landed, if anywhere.
    func endDrag() -> (Int, DropTarget)? {
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
                    Rectangle().fill(model.themeColor.opacity(0.5))
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

/// Where on a row a drag lands, as in Arc: a band at its top and bottom, a quarter of its height
/// but at most 8 points, inserts before or after it; its middle drops into a folder, or beside a
/// tab in a split, on the side of the row the pointer is.
private struct RowDrop: DropDelegate {
    let model: AppModel
    let item: Item
    let size: CGSize
    let place: (Int, Int) -> Void

    func dropUpdated(info: DropInfo) -> DropProposal? {
        model.hover(target(at: info.location))
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

    private func target(at point: CGPoint) -> DropTarget? {
        let band = min(size.height / 4, 8)
        if point.y < band { return .before(item.id) }
        if point.y > size.height - band { return .after(item.id) }
        switch item {
        case .folder(let id, _, _, _):
            return model.dragged == id ? nil : .into(id)
        case .tab(let tab) where model.draggedTab && model.dragged != tab.id:
            return .split(tab.id, leading: point.x < size.width / 2)
        case .split(let id, let tabs)
        where model.draggedTab && tabs.count < 4 && !tabs.contains { $0.id == model.dragged }:
            // Between the tabs the pointer is closest to, as they share the row evenly.
            let slot = (point.x / max(size.width, 1) * CGFloat(tabs.count)).rounded()
            return .join(id, min(tabs.count, max(0, Int(slot))))
        default:
            return point.y < size.height / 2 ? .before(item.id) : .after(item.id)
        }
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

/// A tab that a tab resting on its middle would join in a split, as in Arc: two cards side by
/// side, the tab's and an empty one, tinted, on the side the dropped tab would take.
struct SplitPreview: View {
    let tab: Tab
    let leading: Bool
    let tint: Color

    var body: some View {
        HStack(spacing: 6) {
            if leading { empty }
            HStack(spacing: 6) {
                Favicon(url: tab.favicon, size: 14)
                Text(tab.label).lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(RoundedRectangle(cornerRadius: 12).fill(.primary.opacity(0.1)))
            if !leading { empty }
        }
        .padding(EdgeInsets(top: 2, leading: 14, bottom: 2, trailing: 8))
        .frame(height: Measure.rowFill)
        .padding(.vertical, (Measure.row - Measure.rowFill) / 2)
    }

    private var empty: some View {
        RoundedRectangle(cornerRadius: 12).fill(tint.opacity(0.5)).frame(maxWidth: .infinity)
    }
}

/// A tile of the favorites' grid while a drag goes on: a favorite, or the empty one opening for
/// a tab from elsewhere.
struct FavoriteSlot: Hashable {
    let tab: Tab?

    static func == (lhs: Self, rhs: Self) -> Bool { lhs.tab?.id == rhs.tab?.id }
    func hash(into hasher: inout Hasher) { hasher.combine(tab?.id) }
}

/// Where in the favorites' grid a drag lands: before the tile under the pointer, or after it
/// past its middle.
struct FavoritesDrop: DropDelegate {
    let model: AppModel
    let favorites: Int
    let count: Int
    let columns: Int
    let size: CGSize

    func dropUpdated(info: DropInfo) -> DropProposal? {
        let spacing: CGFloat = 6
        let width = (size.width - spacing * CGFloat(columns - 1)) / CGFloat(columns)
        let column = min(columns - 1, max(0, Int(info.location.x / (width + spacing))))
        let row = max(0, Int(info.location.y / (50 + spacing)))
        let past = info.location.x - CGFloat(column) * (width + spacing) > width / 2
        model.favoriteSlot = min(count, row * columns + column + (past ? 1 : 0))
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) { model.favoriteSlot = nil }

    func performDrop(info: DropInfo) -> Bool {
        let (slot, dragged) = (model.favoriteSlot, model.dragged)
        model.favoriteSlot = nil
        _ = model.endDrag()
        guard let slot, let dragged else { return false }
        model.move(dragged, to: favorites, slot)
        return true
    }
}
