// The sidebar (ui/src/App.tsx): the toolbar, the address, the displayed Space's favorites, pinned
// and today's tabs, and the footer that switches Spaces.
//
// Views keep their own state in State values spelled out, without the @State macro, which only
// Xcode can expand, not the Command Line Tools.

import AppKit
import SwiftUI

struct SidebarView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        if model.libraryShown {
            LibraryView(model: model)
        } else {
            VStack(spacing: 0) {
                SidebarToolbar(model: model)
                if let space = model.space, model.draftSpace == space.id {
                    // Like Arc's, a new Space's form takes the address's and the tabs' room.
                    SpaceDraft(model: model, space: space)
                } else {
                    if !(model.state?.toolbar ?? false) {
                        AddressRow(model: model)
                    }
                    if model.state?.incognito == true, let page = model.page {
                        IncognitoSections(model: model, page: page)
                    } else if let page = model.page, model.space != nil {
                        // Like Arc's, the profile's favorites stay while the Spaces slide below.
                        Favorites(model: model, page: page)
                        SpacePager(model: model)
                    } else {
                        Spacer()
                    }
                }
                RecentDownloads(model: model)
                if model.state?.incognito == true {
                    // Like Arc's, only a button for a new tab.
                    HStack {
                        Spacer()
                        IconButton(symbol: "plus", label: "New Tab") { model.run("overlay/show") }
                    }
                    .frame(height: 40)
                } else {
                    Footer(model: model)
                }
            }
            .padding(.horizontal, Measure.inset)
            .foregroundStyle(.primary)
            // Like Arc's, the sidebar's empty room offers the displayed Space's menu.
            .overlay(RightClick(underOthers: true) {
                if let page = model.page { model.run("menu/space/\(page.space)") }
            })
        }
    }
}

/// The sidebar's toolbar's symbols, as large as Arc's.
private let toolbarSymbol: CGFloat = 16

/// The window's buttons' room, the sidebar's toggle, and the page's navigation buttons.
struct SidebarToolbar: View {
    @ObservedObject var model: AppModel

    var body: some View {
        HStack(spacing: 2) {
            IconButton(symbol: "sidebar.left", label: "Toggle sidebar", symbolSize: toolbarSymbol) {
                model.run("sidebar/toggle")
            }
            Spacer()
            if !(model.state?.toolbar ?? false) {
                NavButtons(model: model)
            }
        }
        .padding(.leading, 72)
        .frame(height: 40)
        // Centered with the window's buttons, on a split's bars, like Arc's.
        .padding(.top, windowButtonsCenter - 20)
    }
}

/// The page's back, forward and reload buttons.
struct NavButtons: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let page = model.state?.page
        IconButton(symbol: "arrow.left", label: "Back", symbolSize: toolbarSymbol) {
            model.run("back")
        }
            .disabled(!(page?.canGoBack ?? false))
        IconButton(symbol: "arrow.right", label: "Forward", symbolSize: toolbarSymbol) {
            model.run("forward")
        }
            .disabled(!(page?.canGoForward ?? false))
        IconButton(symbol: "arrow.clockwise", label: "Reload", symbolSize: toolbarSymbol) {
            model.run("reload")
        }
        // Like Arc's, a site in developer mode offers to reload it afresh.
        if model.activeTab?.developer == true {
            Menu {
                Button("Reload the Page") { model.run("reload") }
                Button("Clear Cache and Refresh") { model.run("menu/choose/site/any/clear-cache") }
                Button("Clear Cookies and Refresh") {
                    model.run("menu/choose/site/any/clear-cookies")
                }
            } label: {
                Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Reload Options")
        }
    }
}

/// A tab whose site is in developer mode, like Arc's: a dashed yellow outline, measured.
struct DeveloperOutline: View {
    var body: some View {
        RoundedRectangle(cornerRadius: Measure.radius)
            .strokeBorder(color("#f9d000"), style: StrokeStyle(lineWidth: 1.5, dash: [5, 3]))
            .allowsHitTesting(false)
    }
}

/// A borderless button showing an SF Symbol, highlighted on hover.
struct IconButton: View {
    let symbol: String
    let label: String
    var size: CGFloat = 28
    /// The symbol's point size.
    var symbolSize: CGFloat = 13
    let action: () -> Void
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: symbolSize, weight: .medium))
                .frame(width: size, height: size)
                .background(
                    RoundedRectangle(cornerRadius: 7).fill(.primary.opacity(hovered ? 0.1 : 0)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(hovered ? 1 : 0.75)
        .onHover { hovered = $0 }
        .help(label)
        .accessibilityLabel(label)
    }
}

/// The selected tab's host: a click edits the address in the Command Bar, as ⌘L does.
struct AddressRow: View {
    @ObservedObject var model: AppModel
    private let _copied = State<Bool>(initialValue: false)
    private var copied: Bool {
        get { _copied.wrappedValue }
        nonmutating set { _copied.wrappedValue = newValue }
    }
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        let tab = model.activeTab
        HStack(spacing: 2) {
            Button {
                model.run("overlay/edit")
            } label: {
                Text(tab.map { domain($0.url) } ?? "")
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.leading, 10)
                    .frame(height: Measure.rowFill)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(tab?.url ?? "")
            if tab != nil {
                if copied {
                    Text("Copied").font(.caption).padding(.horizontal, 6)
                } else if hovered {
                    IconButton(symbol: "link", label: "Copy link", size: 26) {
                        model.run("link/copy")
                        copied = true
                        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { copied = false }
                    }
                }
                SiteControlsButton(model: model)
                ForEach(model.state?.extensions ?? []) { ext in
                    PinnedExtension(model: model, extension: ext)
                }
            }
        }
        .padding(.trailing, 4)
        .background(RoundedRectangle(cornerRadius: Measure.radius).fill(Ink.item))
        .overlay(alignment: .bottom) {
            if let page = model.state?.page, page.loading {
                ProgressView(value: page.progress)
                    .progressViewStyle(.linear)
                    .controlSize(.mini)
                    .padding(.horizontal, 10)
            }
        }
        .onHover { hovered = $0 }
        .padding(.bottom, 8)
    }
}

/// What the address row shows of a page's address: its host, without "www.", or the whole
/// address without a host, like about:blank.
func domain(_ url: String) -> String {
    guard let host = URL(string: url)?.host, !host.isEmpty else { return url }
    return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
}

/// Opens the selected tab's site controls in a panel under the button, which the app places by
/// the button's frame in the window.
struct SiteControlsButton: View {
    @ObservedObject var model: AppModel
    /// Where the button is: "docked" for a frame in the window, or "toolbar" for one in the bar
    /// above the page.
    var view = "docked"

    var body: some View {
        GeometryReader { geometry in
            IconButton(symbol: "slider.horizontal.3", label: "Site controls", size: 26) {
                model.run("site/show/\(view)/\(inMainWindow(geometry.frame(in: .global)))")
            }
        }
        .frame(width: 26, height: 26)
    }
}

/// An extension's button by the address, which opens its popup or tells it it was clicked.
struct PinnedExtension: View {
    @ObservedObject var model: AppModel
    let `extension`: Extension
    var view = "docked"

    var body: some View {
        GeometryReader { geometry in
            Button {
                let rect = inMainWindow(geometry.frame(in: .global))
                model.run("extension/popup/\(`extension`.id)/\(view)/\(rect)")
            } label: {
                Favicon(url: `extension`.icon, size: 16)
                    .frame(width: 26, height: 26)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(`extension`.name)
        }
        .frame(width: 26, height: 26)
    }
}

/// A Space's page: its profile's favorites, then its header, pinned tabs and today's tabs.
struct SpaceSections: View {
    @ObservedObject var model: AppModel
    let page: SpacePage
    let space: Space
    /// The tab list's size, whose room below the rows takes drops.
    private let _width = State<CGFloat>(initialValue: 0)
    private let _height = State<CGFloat>(initialValue: 0)

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 1) {
                    SpaceHeader(model: model, page: page, space: space)
                    if !page.collapsed {
                        ItemRows(
                            model: model, items: page.pinned, parent: page.roots.pinned,
                            selected: page.selected, pinned: true)
                        // Dropped on, the divider puts the node last among the pinned tabs.
                        DividerRow(model: model, clearable: !page.today.isEmpty)
                            .dropDestination(for: String.self) { ids, _ in
                                drop(ids) { model.move($0, to: page.roots.pinned, page.pinned.count) }
                            }
                    }
                    // Dropped on, New Tab puts the node first among today's tabs.
                    NewTabRow(model: model)
                        .dropDestination(for: String.self) { ids, _ in
                            drop(ids) { model.move($0, to: page.roots.today, 0) }
                        }
                    if !page.collapsed {
                        ItemRows(
                            model: model, items: page.today, parent: page.roots.today,
                            selected: page.selected)
                    }
                    // Like Arc's, the room below the rows moves the window and, dropped on, puts
                    // what a drag carries after today's last tab.
                    WindowDragArea()
                        .frame(maxWidth: .infinity, minHeight: 8, maxHeight: .infinity)
                        .onDrop(
                            of: [.plainText],
                            delegate: TailDrop(model: model, page: page, width: _width.wrappedValue))
                }
                .frame(minHeight: _height.wrappedValue, alignment: .top)
                // Rows moving or dropped slide to their places, as quickly as Arc's.
                .animation(.easeInOut(duration: 0.12), value: (page.pinned + page.today).map(\.id))
            }
            .scrollIndicators(.never)
            // Like Arc's, the tab list's empty room moves the window; a double-click there does
            // not zoom it.
            .background(WindowDragArea())
            .onGeometryChange(for: CGSize.self) { $0.size } action: {
                _width.wrappedValue = $0.width
                _height.wrappedValue = $0.height
            }
        }
    }
}

/// An incognito window's tabs, like Arc's: under their heading, New Tab and today's tabs only.
struct IncognitoSections: View {
    @ObservedObject var model: AppModel
    let page: SpacePage

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 1) {
                HStack {
                    Text("Incognito Tabs").font(.system(size: 13, weight: .semibold))
                    Spacer()
                }
                .padding(.horizontal, 6)
                .frame(height: 32)
                .foregroundStyle(.secondary)
                NewTabRow(model: model)
                ItemRows(
                    model: model, items: page.today, parent: page.roots.today,
                    selected: page.selected)
                WindowDragArea()
                    .frame(maxWidth: .infinity, minHeight: 8, maxHeight: .infinity)
            }
        }
        .scrollIndicators(.never)
        .background(WindowDragArea())
    }
}

/// The displayed Space's tabs between its neighbors', like Arc's: a swipe slides them along, the
/// neighbor coming in beside it, and past a threshold carries on to it.
struct SpacePager: View {
    @ObservedObject var model: AppModel

    var body: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            HStack(spacing: 0) {
                ForEach(model.neighbors(), id: \.id) { neighbor in
                    Group {
                        if let (page, space) = neighbor.shown {
                            SpaceSections(model: model, page: page, space: space)
                        } else {
                            Color.clear
                        }
                    }
                    .frame(width: width)
                }
            }
            .offset(x: model.pagerShift - width)
            .onAppear { model.pageWidth = width }
            .onChange(of: width) { _, now in model.pageWidth = now }
        }
        .clipped()
        // Past the last Space, a "+" comes in from the edge, its ring filling as the swipe goes.
        .overlay(alignment: .trailing) {
            let share = model.creatingShare
            if share > 0 {
                CreateDial(share: share, tint: model.accent)
                    .offset(x: 48 * (1 - min(1, share * 2)))
                    .padding(.trailing, 8)
            }
        }
    }
}

/// The "+" a swipe past the last Space fills, like Arc's: a ring around it, then whole, tinted.
struct CreateDial: View {
    let share: CGFloat
    let tint: Color

    var body: some View {
        let full = share >= 1
        ZStack {
            Circle().fill(full ? tint : Color.white.opacity(0.9))
            Circle().trim(from: 0, to: share)
                .stroke(tint, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .padding(1.5)
            Image(systemName: "plus").font(.system(size: 15, weight: .semibold))
                .foregroundStyle(full ? Color.white : tint)
        }
        .frame(width: 30, height: 30)
        .shadow(color: .black.opacity(0.2), radius: 2, y: 1)
        .animation(.easeOut(duration: 0.15), value: full)
    }
}

/// The profile's favorites, in a grid of up to four columns.
struct Favorites: View {
    @ObservedObject var model: AppModel
    let page: SpacePage
    private let _size = State<CGSize>(initialValue: .zero)

    /// The favorites as a drag shows them, like Arc's: the one dragged moves to where it would
    /// land, or an empty tile opens there for a tab from elsewhere.
    private var shownFavorites: [FavoriteSlot] {
        var slots = page.favorites.map { FavoriteSlot(tab: $0) }
        guard let slot = model.favoriteSlot, let dragged = model.dragged else { return slots }
        if let from = slots.firstIndex(where: { $0.tab?.id == dragged }) {
            let moved = slots.remove(at: from)
            slots.insert(moved, at: min(from < slot ? slot - 1 : slot, slots.count))
        } else {
            slots.insert(FavoriteSlot(tab: nil), at: min(slot, slots.count))
        }
        return slots
    }

    var body: some View {
        let count = page.favorites.count
        if count == 0 {
            if page.favoritesHint {
                // Dropped on, the hint makes the node the first favorite.
                FavoritesHint(model: model, space: page.space)
                    .dropDestination(for: String.self) { ids, _ in
                        drop(ids) { model.move($0, to: page.roots.favorites, 0) }
                    }
            }
        } else {
            let rows = max(1, (count + 3) / 4)
            let columns = max(1, (count + rows - 1) / rows)
            LazyVGrid(
                columns: Array(
                    repeating: GridItem(.flexible(minimum: Measure.tile), spacing: Measure.inset),
                    count: columns),
                spacing: Measure.inset
            ) {
                ForEach(shownFavorites, id: \.self) { slot in
                    if let tab = slot.tab {
                        FavoriteTile(model: model, tab: tab, active: tab.id == page.selected)
                            .opacity(tab.id == model.dragged && model.favoriteSlot != nil ? 0 : 1)
                            .onDrag { model.startDrag(.tab(tab)) }
                    } else {
                        Color.clear.frame(height: Measure.tile)
                    }
                }
            }
            .animation(.easeInOut(duration: 0.12), value: shownFavorites)
            .onGeometryChange(for: CGSize.self) { $0.size } action: { _size.wrappedValue = $0 }
            .onDrop(
                of: [.plainText],
                delegate: FavoritesDrop(
                    model: model, favorites: page.roots.favorites, count: count,
                    columns: columns, size: _size.wrappedValue))
            .padding(.bottom, 8)
        }
    }
}

struct FavoriteTile: View {
    @ObservedObject var model: AppModel
    let tab: Tab
    let active: Bool
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        Button {
            model.run("select/\(tab.id)")
        } label: {
            Favicon(url: tab.favicon, size: 20)
                .frame(maxWidth: .infinity)
                .frame(height: Measure.tile)
                .background(
                    RoundedRectangle(cornerRadius: Measure.radius)
                        .fill(active ? Ink.selected : hovered ? Ink.itemHovered : Ink.item)
                        .shadow(color: active ? Ink.selectedShadow : .clear, radius: 1, y: 1))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(tab.label)
        .onHover { hovered = $0 }
        .rightClick { model.run("menu/node/\(tab.id)") }
    }
}

/// The empty favorites' invitation to drag tabs there; its × hides it for the profile until it
/// has favorites again.
struct FavoritesHint: View {
    @ObservedObject var model: AppModel
    let space: Int

    var body: some View {
        VStack(spacing: 4) {
            Image(systemName: "star.circle.fill").font(.system(size: 16))
            Text("Drag tabs here to keep them close").font(.system(size: 13, weight: .semibold))
            Text("Favorites stay one click away in every Space of this profile")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding(12)
        .frame(maxWidth: .infinity)
        // Dashed, like Arc's.
        .background(
            RoundedRectangle(cornerRadius: Measure.radius)
                .strokeBorder(.primary.opacity(0.3), style: StrokeStyle(lineWidth: 1, dash: [4, 3])))
        .overlay(alignment: .topTrailing) {
            IconButton(symbol: "xmark", label: "Hide this hint", size: 20) {
                model.run("favorites/hide-hint/\(space)")
            }
            .padding(4)
        }
        .padding(.bottom, 8)
    }
}

/// A click collapses or expands the Space; its "…" opens its menu.
struct SpaceHeader: View {
    @ObservedObject var model: AppModel
    let page: SpacePage
    let space: Space
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        HStack(spacing: 8) {
            Button {
                model.run("space/toggle/\(page.space)")
            } label: {
                // Like Arc's: the emoji where the tabs' icons stand, and none without one.
                HStack(spacing: 6) {
                    if editing == "emoji" {
                        SpaceField(model: model, space: page.space, field: "emoji", value: "")
                            .font(.system(size: 16))
                            .frame(width: 24)
                    } else if let emoji = space.emoji {
                        Group {
                            if hovered {
                                Image(systemName: "chevron.right")
                                    .rotationEffect(.degrees(page.collapsed ? 0 : 90))
                                    .font(.system(size: 10, weight: .semibold))
                            } else {
                                Text(emoji).font(.system(size: 16))
                            }
                        }
                        .frame(width: 24)
                    }
                    if editing == "name" {
                        SpaceField(model: model, space: page.space, field: "name", value: space.title)
                            .font(.system(size: 13, weight: .semibold))
                    } else {
                        Text(space.title)
                            .font(.system(size: 13, weight: .semibold))
                            .lineLimit(1)
                    }
                    Spacer()
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if hovered {
                IconButton(symbol: "ellipsis", label: "Space options", size: 24) {
                    model.run("menu/space/\(page.space)")
                }
            }
        }
        .padding(.horizontal, 6)
        .frame(height: 32)
        .foregroundStyle(.secondary)
        .onHover { hovered = $0 }
        .rightClick { model.run("menu/space/\(page.space)") }
    }
}

extension SpaceHeader {
    /// What of the Space is edited in its header, if anything.
    private var editing: String? {
        model.editingSpace?.space == page.space ? model.editingSpace?.field : nil
    }
}

/// A Space's name or icon to edit, from its menu.
struct SpaceEdit: Decodable, Equatable {
    var space: Int
    var field: String
}

/// Edits a Space's name in place, or its icon: the character palette opens, and the first emoji
/// picked or typed is the icon; an empty one removes it.
struct SpaceField: View {
    @ObservedObject var model: AppModel
    let space: Int
    let field: String
    let value: String
    private let _text = State<String>(initialValue: "")
    @FocusState private var focused: Bool

    var body: some View {
        TextField("", text: _text.projectedValue)
            .textFieldStyle(.plain)
            .focused($focused)
            .onAppear {
                _text.wrappedValue = value
                focused = true
                if field == "emoji" {
                    DispatchQueue.main.async { NSApp.orderFrontCharacterPalette(nil) }
                }
            }
            .onChange(of: _text.wrappedValue) { _, now in
                if field == "emoji", let first = now.first { done(String(first)) }
            }
            .onSubmit { done(_text.wrappedValue) }
            .onExitCommand { model.editingSpace = nil }
            .onChange(of: focused) { _, now in
                // Clicked away: a name is kept, an icon not picked is left as it was.
                guard !now, model.editingSpace != nil else { return }
                if field == "name" { done(_text.wrappedValue) } else { model.editingSpace = nil }
            }
    }

    private func done(_ text: String) {
        guard model.editingSpace != nil else { return }
        model.editingSpace = nil
        let value = text.trimmingCharacters(in: .whitespaces)
        if field == "emoji" {
            model.run("space/emoji/\(space)/\(percentEncoded(value))")
        } else if !value.isEmpty, value != self.value {
            model.run("space/rename/\(space)/\(percentEncoded(value))")
        }
    }
}

/// Between the pinned tabs and today's: hovered, it offers to close today's tabs.
struct DividerRow: View {
    @ObservedObject var model: AppModel
    let clearable: Bool
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        HStack(spacing: 6) {
            Rectangle().fill(.primary.opacity(0.12)).frame(height: 1)
            if clearable && hovered {
                Button("↓ Clear") { model.run("today/clear") }
                    .buttonStyle(.plain)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .help("Close today's tabs")
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 20)
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
    }
}

struct NewTabRow: View {
    @ObservedObject var model: AppModel
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        Button {
            model.run("overlay/show")
        } label: {
            HStack(spacing: 8) {
                Image(systemName: "plus").frame(width: 16)
                Text("New Tab")
                Spacer()
            }
            .padding(.horizontal, 10)
            .frame(height: Measure.rowFill)
            .background(
                RoundedRectangle(cornerRadius: Measure.radius).fill(hovered ? Ink.item : .clear))
            .padding(.vertical, (Measure.row - Measure.rowFill) / 2)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .onHover { hovered = $0 }
    }
}

/// Tabs, folders and split rows, as deep as folders go.
struct ItemRows: View {
    @ObservedObject var model: AppModel
    let items: [Item]
    /// Their container: a section's root, or a folder.
    let parent: Int
    let selected: Int?
    /// Among the pinned tabs, whose close button only deselects them, like Arc's.
    var pinned = false
    var depth = 0

    var body: some View {
        ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
            DropRow(model: model, item: item, place: { model.move($0, to: parent, index + $1) }) {
                switch item {
                case .tab(let tab):
                    TabRow(model: model, tab: tab, active: tab.id == selected, pinned: pinned)
                        .overlay {
                            if tab.developer == true {
                                DeveloperOutline()
                                    .frame(height: Measure.rowFill)
                            }
                        }
                        .overlay {
                            if case .split(tab.id, let leading) = model.dropTarget {
                                SplitHalf(leading: leading)
                            }
                        }
                case .folder(let id, let title, let expanded, _):
                    FolderRow(model: model, id: id, title: title, expanded: expanded)
                case .split(let id, let tabs):
                    SplitRow(model: model, id: id, tabs: tabs, selected: selected)
                }
            }
            .padding(.leading, CGFloat(depth) * Measure.indent)
            if case .folder(let id, _, true, let children) = item {
                ItemRows(
                    model: model, items: children, parent: id, selected: selected,
                    pinned: pinned, depth: depth + 1)
            }
        }
    }
}

/// Runs `move` with the id a drag carries, if it carries one.
func drop(_ ids: [String], _ move: (Int) -> Void) -> Bool {
    guard let id = ids.first.flatMap({ Int($0) }) else { return false }
    move(id)
    return true
}

/// A tab's row: a click selects it, its × closes it; on the selected row, closing may only
/// deselect a pinned tab. Its menu's Rename Tab edits its name in place.
struct TabRow: View {
    @ObservedObject var model: AppModel
    let tab: Tab
    let active: Bool
    var pinned = false
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        HStack(spacing: 6) {
            // A 16-point icon in a 24-point slot, like Arc's.
            Favicon(url: tab.favicon, size: 16).opacity(tab.asleep == true ? 0.55 : 1)
                .frame(width: 24, height: 24)
            if model.renaming == tab.id {
                NameField(model: model, id: tab.id, title: tab.label, blank: true)
            } else {
                Text(tab.label).lineLimit(1).opacity(tab.asleep == true ? 0.55 : 1)
                if tab.muted == true {
                    Image(systemName: "speaker.slash").font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                if hovered || active {
                    // Like Arc's: a minus for a pinned tab, which stays, an x for the others.
                    CloseButton(symbol: pinned ? "minus" : "xmark", help: "Close \(tab.label)") {
                        model.run("close/\(tab.id)")
                    }
                    .opacity(hovered ? 1 : 0)
                }
            }
        }
        .padding(.leading, 6)
        .padding(.trailing, 6)
        .font(.system(size: 14))
        .foregroundStyle(model.titleInk)
        .frame(height: Measure.rowFill)
        .background(
            RoundedRectangle(cornerRadius: Measure.radius)
                .fill(
                    active
                        ? Ink.selected
                        : model.marked.contains(tab.id) ? Ink.marked : hovered ? Ink.item : .clear)
                .shadow(color: active ? Ink.selectedShadow : .clear, radius: 1, y: 1))
        .padding(.vertical, (Measure.row - Measure.rowFill) / 2)
        .contentShape(Rectangle())
        .onTapGesture { model.click(tab.id) }
        .onHover { hovered = $0 }
        .rightClick {
            // Several tabs selected at once share one menu.
            let tabs = model.acting(on: tab.id)
            if tabs.count > 1 {
                model.run("menu/tabs/\(percentEncoded("\(tabs)"))")
            } else {
                model.run("menu/node/\(tab.id)")
            }
        }
    }
}

/// A click collapses or expands the folder, a double click edits its title.
struct FolderRow: View {
    @ObservedObject var model: AppModel
    let id: Int
    let title: String
    let expanded: Bool
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: expanded ? "folder" : "folder.fill")
                .font(.system(size: 13))
                .frame(width: 16)
            if model.renaming == id {
                NameField(model: model, id: id, title: title, blank: false)
            } else {
                Text(title).lineLimit(1)
                Spacer(minLength: 0)
            }
        }
        .padding(.horizontal, 10)
        .font(.system(size: 14))
        .frame(height: Measure.rowFill)
        .background(
            RoundedRectangle(cornerRadius: Measure.radius).fill(hovered ? Ink.item : .clear))
        .padding(.vertical, (Measure.row - Measure.rowFill) / 2)
        .contentShape(Rectangle())
        .gesture(
            TapGesture(count: 2).onEnded { model.renaming = id }
                .exclusively(before: TapGesture().onEnded { model.run("toggle/\(id)") }))
        .onHover { hovered = $0 }
        .rightClick { model.run("menu/node/\(id)") }
    }
}

/// A row's name being edited. Enter and leaving the field save it, Escape keeps the old one. A
/// blank folder name keeps the old one; a blank tab name gives the tab its page's title back.
struct NameField: View {
    @ObservedObject var model: AppModel
    let id: Int
    let title: String
    let blank: Bool
    private let _text = State<String>(initialValue: "")
    private var text: String {
        get { _text.wrappedValue }
        nonmutating set { _text.wrappedValue = newValue }
    }
    @FocusState private var focused: Bool

    var body: some View {
        TextField("", text: _text.projectedValue)
            .textFieldStyle(.plain)
            .focused($focused)
            .onAppear {
                text = title
                focused = true
            }
            .onSubmit { focused = false }
            .onExitCommand {
                text = title
                focused = false
            }
            .onChange(of: focused) { _, now in
                guard !now else { return }
                let value = text.trimmingCharacters(in: .whitespaces)
                if (!value.isEmpty || blank) && value != title {
                    model.run("rename/\(id)/\(percentEncoded(value))")
                }
                model.renaming = nil
            }
    }
}

/// A split's tabs in one row, in equal parts: a click on one shows the split with that tab
/// focused, the only one with a background, like Arc's.
struct SplitRow: View {
    @ObservedObject var model: AppModel
    let id: Int
    let tabs: [Tab]
    let selected: Int?
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        // Its tabs share the row evenly, whatever they show, like Arc's.
        let count = tabs.count + (joining == nil ? 0 : 1)
        LazyVGrid(
            columns: Array(repeating: GridItem(.flexible(minimum: 0), spacing: 4), count: count),
            spacing: 0
        ) {
            ForEach(Array(tabs.enumerated()), id: \.element.id) { index, tab in
                if joining == index { DropChip(tint: model.themeColor) }
                chip(tab)
            }
            if joining == tabs.count { DropChip(tint: model.themeColor) }
        }
        .padding(3)
        .frame(height: Measure.rowFill)
        .background(
            RoundedRectangle(cornerRadius: Measure.radius)
                .fill(active ? Ink.selected : hovered ? Ink.item : .clear)
                .shadow(color: active ? Ink.selectedShadow : .clear, radius: 1, y: 1))
        .padding(.vertical, (Measure.row - Measure.rowFill) / 2)
        .onHover { hovered = $0 }
    }

    /// The split holds the selected tab.
    private var active: Bool { tabs.contains { $0.id == selected } }

    /// Where a tab dropped on the row would join it, if one is.
    private var joining: Int? {
        if case .join(id, let index) = model.dropTarget { return index }
        return nil
    }

    /// A tab of the split: while the row is hovered, a button takes its title's place, which
    /// closes it, or with Option takes it out of the split, like Arc's. The title stays, unseen,
    /// so that nothing moves.
    private func chip(_ tab: Tab) -> some View {
        HStack(spacing: 6) {
            Favicon(url: tab.favicon, size: 14)
            Text(tab.label).lineLimit(1).opacity(hovered ? 0 : 1)
            Spacer(minLength: 0)
        }
        .overlay(alignment: .trailing) {
            if hovered {
                let separate = model.optionHeld
                ChipButton(
                    symbol: separate ? "rectangle.lefthalf.inset.filled.arrow.left" : "xmark",
                    help: separate ? "Separate from Split View" : "Close \(tab.label)"
                ) {
                    model.run(separate ? "separate/\(tab.id)" : "close/\(tab.id)")
                }
            }
        }
        .padding(.leading, 8)
        .padding(.trailing, 4)
        .font(.system(size: 14))
        .frame(height: 28)
        .background(
            RoundedRectangle(cornerRadius: 8).fill(tab.id == selected ? Ink.child : .clear))
        .contentShape(Rectangle())
        .onTapGesture { model.run("select/\(tab.id)") }
        .rightClick { model.run("menu/node/\(id)") }
        .help(tab.label)
    }
}

/// A row's close button, like Arc's: a 24-point square, tinted while hovered, its glyph
/// a little faded.
struct CloseButton: View {
    let symbol: String
    let help: String
    let action: () -> Void
    private let _hovered = State<Bool>(initialValue: false)

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(Color.ink((0x000000, 0.565), (0xFFFFFF, 0.592)))
                .frame(width: 24, height: 24)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(_hovered.wrappedValue ? Color.ink((0x000000, 0.1), (0xFFFFFF, 0.1)) : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { _hovered.wrappedValue = $0 }
        .help(help)
    }
}

/// A split tab's button, tinted while hovered.
private struct ChipButton: View {
    let symbol: String
    let help: String
    let action: () -> Void
    private let _hovered = State<Bool>(initialValue: false)

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
                .frame(width: 22, height: 22)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(.primary.opacity(_hovered.wrappedValue ? 0.1 : 0)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { _hovered.wrappedValue = $0 }
        .help(help)
    }
}

/// A tab's icon, or a placeholder without one.
struct Favicon: View {
    let url: String?
    let size: CGFloat

    var body: some View {
        Group {
            if let image = uiImage(url) {
                Image(nsImage: image).resizable().interpolation(.high)
            } else {
                RoundedRectangle(cornerRadius: 4).fill(.primary.opacity(0.15))
            }
        }
        .frame(width: size, height: size)
    }
}

/// The library button, one icon per Space, and the "+" that creates a Space.
struct Footer: View {
    @ObservedObject var model: AppModel

    /// Like Arc's, a new Space's form leaves only the Spaces' icons.
    private var drafting: Bool { model.draftSpace != nil }

    var body: some View {
        HStack(spacing: 0) {
            IconButton(symbol: "books.vertical", label: "Library") {
                model.openLibrary(nil)
            }
            .opacity(drafting ? 0 : 1)
            .disabled(drafting)
            Spacer()
            HStack(spacing: 2) {
                ForEach(model.state?.spaces ?? []) { space in
                    SpaceIcon(model: model, space: space, displayed: space.id == model.state?.space)
                }
            }
            Spacer()
            Menu {
                Button("New Space…") { model.newSpace() }
                Button("New Folder") { model.run("folder/new") }
                    .keyboardShortcut("n", modifiers: [.command, .shift])
                Divider()
                Button("New Tab") { model.run("overlay/show") }
                    .keyboardShortcut("t", modifiers: .command)
                Divider()
                Button("Profiles…") { model.profilesShown = true }
            } label: {
                Image(systemName: "plus").font(.system(size: 13, weight: .medium))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .frame(width: 28, height: 28)
            .help("New")
            .opacity(drafting ? 0 : 1)
            .disabled(drafting)
            .popover(isPresented: $model.profilesShown, arrowEdge: .top) {
                ProfilesView(model: model)
            }
        }
        .frame(height: 40)
    }
}

/// A Space's emoji, or a dot without one: a click displays the Space, a right click opens its
/// menu, like Arc's.
struct SpaceIcon: View {
    @ObservedObject var model: AppModel
    let space: Space
    let displayed: Bool
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        Button {
            model.run("space/\(space.id)")
        } label: {
            Group {
                if let emoji = space.emoji {
                    Text(emoji).font(.system(size: 13)).opacity(displayed ? 1 : 0.5)
                } else {
                    Circle()
                        .fill(.primary.opacity(displayed ? 0.9 : 0.35))
                        .frame(width: 6, height: 6)
                }
            }
            .frame(width: 24, height: 24)
            .background(
                RoundedRectangle(cornerRadius: 6).fill(.primary.opacity(hovered ? 0.1 : 0)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(space.title)
        .onHover { hovered = $0 }
        .rightClick { model.run("menu/space/\(space.id)") }
    }
}

extension View {
    /// Runs `action` on a right click, which SwiftUI views otherwise leave to a context menu.
    func rightClick(_ action: @escaping () -> Void) -> some View {
        overlay(RightClick(action: action))
    }
}

/// Catches right clicks only, letting every other event through to the views under it.
struct RightClick: NSViewRepresentable {
    /// Over views that catch their own right clicks, it lets them have them.
    var underOthers = false
    let action: () -> Void

    func makeNSView(context: Context) -> Catcher {
        let view = Catcher()
        view.action = action
        view.underOthers = underOthers
        return view
    }

    func updateNSView(_ view: Catcher, context: Context) {
        view.action = action
    }

    final class Catcher: NSView {
        var action: () -> Void = {}
        var underOthers = false

        override func hitTest(_ point: NSPoint) -> NSView? {
            let type = NSApp.currentEvent?.type
            let right =
                type == .rightMouseDown || type == .rightMouseUp
                || (type == .leftMouseDown && NSApp.currentEvent?.modifierFlags.contains(.control) == true)
            guard right, let hit = super.hitTest(point) else { return nil }
            if underOthers, let root = window?.contentView?.superview {
                isHidden = true
                defer { isHidden = false }
                if root.hitTest(superview?.convert(point, to: nil) ?? point) is Catcher { return nil }
            }
            return hit
        }

        override func rightMouseDown(with event: NSEvent) { action() }
        override func mouseDown(with event: NSEvent) { action() }
    }
}

/// The tab list's empty room, which moves the window like Arc's: a double-click there does not
/// zoom it.
struct WindowDragArea: NSViewRepresentable {
    func makeNSView(context: Context) -> WindowDrag {
        let view = WindowDrag()
        view.doubleClicks = false
        return view
    }

    func updateNSView(_ view: WindowDrag, context: Context) {}
}

/// A value as a command's path holds it.
func percentEncoded(_ value: String) -> String {
    value.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? ""
}

/// A view's frame, from the top left corner of the window it shows in, as a command's path gives
/// it: in the main window, the one a panel like the sidebar belongs to, from its content's top
/// left corner, as "x/y/width/height".
@MainActor func inMainWindow(_ frame: CGRect) -> String {
    var rect = frame
    if let from = NSApp.currentEvent?.window, let main = from.parent,
        let fromHeight = from.contentView?.frame.height,
        let mainHeight = main.contentView?.frame.height
    {
        let bottomLeft = NSRect(
            x: frame.minX, y: fromHeight - frame.maxY, width: frame.width, height: frame.height)
        let inMain = main.convertFromScreen(from.convertToScreen(bottomLeft))
        rect = CGRect(
            x: inMain.minX, y: mainHeight - inMain.maxY, width: inMain.width, height: inMain.height)
    }
    return [rect.minX, rect.minY, rect.width, rect.height]
        .map { String(Int($0.rounded())) }.joined(separator: "/")
}
