// The sidebar (ui/src/App.tsx): the toolbar, the address, the displayed Space's favorites, pinned
// and today's tabs, and the footer that switches Spaces.
//
// Views keep their own state in State values spelled out, without the @State macro, which only
// Xcode can expand, not the Command Line Tools.

import AppKit
import SwiftUI

/// How long a Space's page takes to slide in, like Arc's.
let spaceSlide = 0.3

struct SidebarView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        if model.libraryShown {
            LibraryView(model: model)
        } else {
            VStack(spacing: 0) {
                SidebarToolbar(model: model)
                // Like Arc's, a new Space's form slides in from the right over the tabs' room,
                // which slides out to the left.
                ZStack {
                    if let space = model.space, model.draftSpace == space.id {
                        // Like Arc's, a new Space's form takes the address's and the tabs' room.
                        SpaceDraft(model: model, space: space)
                            // A swipe back, rightward, carries it along, like Arc's.
                            .offset(x: max(0, model.swipe))
                            .transition(.move(edge: .trailing))
                    } else {
                        VStack(spacing: 0) {
                            if !(model.state?.toolbar ?? false) {
                                AddressRow(model: model)
                            }
                            if model.state?.incognito == true, let page = model.page {
                                IncognitoSections(model: model, page: page)
                            } else if let page = model.page, model.space != nil {
                                // Like Arc's, the profile's favorites stay while the Spaces slide
                                // below.
                                Favorites(model: model, page: page)
                                    // Past the first or the last Space, like Arc's, the
                                    // favorites go along with the tabs.
                                    .offset(x: model.edgeShift)
                                SpacePager(model: model)
                            } else {
                                Spacer()
                            }
                        }
                        .transition(.move(edge: .leading))
                    }
                }
                .clipped()
                .animation(.easeInOut(duration: spaceSlide), value: model.draftSpace != nil)
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
            .overlay(alignment: .trailing) { CreateHint(model: model) }
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
        // In toolbar mode, the bar above the page has them, but in developer mode, like Arc's.
        let navigates = !(model.state?.toolbar ?? false) || model.activeTab?.developer == true
        // Like Arc's, too narrow for them all, the toggle and reload go into a "⋯" menu.
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 2) {
                IconButton(
                    symbol: "sidebar.left", label: "Toggle sidebar", symbolSize: toolbarSymbol
                ) {
                    model.run("sidebar/toggle")
                }
                Spacer(minLength: 0)
                if navigates {
                    NavButtons(model: model)
                }
            }
            HStack(spacing: 0) {
                Spacer(minLength: 0)
                if navigates {
                    NavButtons(model: model, compact: true)
                }
            }
        }
        .frame(height: 40)
        // Past the window's buttons, which take their own clicks.
        .background(WindowDragArea(doubleClicks: true))
        // Past the window's buttons as far as they stand apart, like Arc's, the toggle's symbol 5
        // points into its button; in full screen, without them, 20 points from the sidebar's edge.
        .padding(.leading, (model.fullScreen ? 20 : togglePlace) - Measure.inset - 5)
        // Like Arc's, floating, it shows the window's buttons, which it covers, where they stand.
        .overlay(alignment: .leading) {
            if model.sidebarFloating {
                WindowLights()
                    .frame(width: windowLightsSize.width, height: windowLightsSize.height)
                    .padding(.leading, windowButtonsLeading - Measure.inset)
            }
        }
        .foregroundStyle(model.buttonInk)
        // Centered with the window's buttons, on a split's bars, like Arc's.
        .padding(.top, windowButtonsCenter - 20)
    }
}

/// The page's back, forward and reload buttons.
struct NavButtons: View {
    @ObservedObject var model: AppModel
    /// Reload, and the sidebar's toggle, in a "⋯" menu, like Arc's in a narrow sidebar.
    var compact = false

    var body: some View {
        let page = model.state?.page
        // Measured on Arc's: the symbols' centers 34 then 28 points apart, 28 once narrow.
        HStack(spacing: 0) {
            IconButton(
                symbol: "arrow.left", label: "Back", size: compact ? 28 : 34, symbolSize: toolbarSymbol
            ) {
                model.run("back")
            }
                .disabled(!(page?.canGoBack ?? false))
            IconButton(
                symbol: "arrow.right", label: "Forward", size: compact ? 28 : 34,
                symbolSize: toolbarSymbol
            ) {
                model.run("forward")
            }
                .disabled(!(page?.canGoForward ?? false))
            if compact {
                MoreButton(model: model)
            } else {
                // Like Arc's, it stops the page while it loads.
                let loading = page?.loading == true
                IconButton(
                    symbol: loading ? "xmark" : "arrow.clockwise",
                    label: loading ? "Stop" : "Reload", size: 22, symbolSize: toolbarSymbol
                ) {
                    model.run(loading ? "stop" : "reload")
                }
                reloadOptions
            }
        }
    }

    /// Like Arc's, a site in developer mode offers to reload it afresh, from a chevron against
    /// the reload button.
    @ViewBuilder private var reloadOptions: some View {
        if model.activeTab?.developer == true {
            Menu {
                Button("Reload the Page") { model.run("reload") }
                Button("Clear Cache and Refresh") { model.run("menu/choose/site/any/clear-cache") }
                Button("Clear Cookies and Refresh") {
                    model.run("menu/choose/site/any/clear-cookies")
                }
            } label: {
                Image(systemName: "chevron.down").font(.system(size: 8, weight: .medium))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .frame(width: 10, height: 28)
            // A menu's label takes no style: it dims like the buttons by itself.
            .opacity(model.active ? 1 : 0.3)
            .help("Reload Options")
        }
    }
}

/// The narrow sidebar's "⋯", like Arc's: a popover with reload, force reload, clearing the
/// site's cookies and the sidebar's toggle; it turns into a "×" while open.
struct MoreButton: View {
    @ObservedObject var model: AppModel
    private let _shown = State<Bool>(initialValue: false)
    private var shown: Bool {
        get { _shown.wrappedValue }
        nonmutating set { _shown.wrappedValue = newValue }
    }

    var body: some View {
        Button {
            shown.toggle()
        } label: {
            Image(systemName: shown ? "xmark" : "ellipsis")
                .font(.system(size: toolbarSymbol, weight: .medium))
                .frame(width: 17, height: 28)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(shown ? 1 : 0.75)
        .help("More")
        .popover(isPresented: _shown.projectedValue, arrowEdge: .bottom) {
            VStack(spacing: 0) {
                row("arrow.clockwise", "Reload", "⌘R") { model.run("reload") }
                row("arrow.clockwise.circle", "Force Reload", "⇧⌘R") { model.run("reload/force") }
                row("arrow.triangle.2.circlepath", "Clear Cookies", nil) {
                    model.run("menu/choose/site/any/clear-cookies")
                }
                row("sidebar.left", "Toggle Sidebar", "⌘S") { model.run("sidebar/toggle") }
            }
            .padding(8)
            .frame(width: 200)
        }
    }

    private func row(
        _ symbol: String, _ title: String, _ shortcut: String?, action: @escaping () -> Void
    ) -> some View {
        MoreRow(symbol: symbol, title: title, shortcut: shortcut, accent: model.accent) {
            shown = false
            action()
        }
    }
}

/// A row of the "⋯" popover, highlighted in the accent on hover.
private struct MoreRow: View {
    let symbol: String
    let title: String
    let shortcut: String?
    let accent: Color
    let action: () -> Void
    private let _hovered = State<Bool>(initialValue: false)

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: symbol).font(.system(size: 15, weight: .medium)).frame(width: 22)
                Text(title).font(.system(size: 13, weight: .medium))
                Spacer(minLength: 8)
                if let shortcut {
                    Text(shortcut).font(.system(size: 12)).foregroundStyle(accent.opacity(0.45))
                }
            }
            .padding(.horizontal, 10)
            .frame(height: 38)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(accent.opacity(_hovered.wrappedValue ? 0.18 : 0)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { _hovered.wrappedValue = $0 }
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
    /// What it opens shows: it looks hovered, like Arc's.
    var active = false
    let action: () -> Void
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        let lit = hovered || active
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: symbolSize, weight: .medium))
                .frame(width: size, height: size)
                .background(
                    RoundedRectangle(cornerRadius: 7).fill(.primary.opacity(lit ? 0.1 : 0)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(lit ? 1 : 0.75)
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
                    .padding(.leading, 14)
                    .frame(height: Measure.rowFill)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(tab?.url ?? "")
            if tab != nil {
                // Like Arc's, the pinned extensions first, always there.
                ForEach(model.state?.extensions ?? []) { ext in
                    PinnedExtension(model: model, extension: ext)
                }
                // Like Arc's, the address's buttons show while the pointer is over it, and while
                // the site controls show.
                let shown = hovered || model.siteControlsShown
                if copied {
                    Text("Copied").font(.caption).padding(.horizontal, 6)
                } else if shown {
                    IconButton(symbol: "link", label: "Copy link", size: 26) {
                        model.run("link/copy")
                        copied = true
                        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { copied = false }
                    }
                }
                if shown {
                    SiteControlsButton(model: model)
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
        // 46 points from the window's top, like Arc's.
        .padding(.top, 46 - windowButtonsCenter - 20)
        .padding(.bottom, 7)
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
    var symbol = "slider.horizontal.3"
    var symbolSize: CGFloat = 13

    var body: some View {
        GeometryReader { geometry in
            IconButton(
                symbol: symbol, label: "Site controls", size: 26, symbolSize: symbolSize,
                active: model.siteControlsShown
            ) {
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
            // Like Arc's, its menu (extension_menu in app/src/menu.rs).
            .rightClick { model.run("extension/context/\(`extension`.id)") }
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
                    // Like Arc's, collapsing a Space folds its pinned tabs away, not today's.
                    if !page.collapsed {
                        ItemRows(
                            model: model, items: page.pinned, parent: page.roots.pinned,
                            selected: page.selected, pinned: true)
                            // Fading, not sliding, which would pass over the header.
                            .transition(.opacity)
                    }
                    // Dropped on, the divider puts the node last among the pinned tabs, the
                    // line showing under the last one.
                    DividerRow(model: model, clearable: !page.today.isEmpty, root: page.roots.pinned)
                        .onDrop(
                            of: [.plainText],
                            delegate: SectionDrop(
                                model: model, parent: page.roots.pinned, index: page.pinned.count,
                                shown: lastShown(page.pinned).map { .after($0) }
                                    ?? .into(page.roots.pinned)))
                    // Dropped on, New Tab puts the node first among today's tabs.
                    NewTabRow(model: model, root: page.roots.today)
                        .onDrop(
                            of: [.plainText],
                            delegate: SectionDrop(
                                model: model, parent: page.roots.today, index: 0,
                                shown: page.today.first.map { .before($0.id) }
                                    ?? .into(page.roots.today)))
                    ItemRows(
                        model: model, items: page.today, parent: page.roots.today,
                        selected: page.selected)
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
                // Folders open and close sliding, the pinned tabs fold away fading.
                .animation(.easeInOut(duration: 0.2), value: openFolders(page.pinned + page.today))
                .animation(.easeInOut(duration: 0.2), value: page.collapsed)
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
            // Like Arc's, a click on a Space a few away carries the page across those between.
            let carousel = model.carousel
            let pages = carousel ?? model.neighbors()
            let shift =
                carousel.map { -model.carouselShare * CGFloat($0.count - 1) * width }
                ?? model.pagerShift - width
            HStack(spacing: 0) {
                ForEach(pages, id: \.id) { neighbor in
                    Group {
                        // The app's latest page, once it sends the one a carousel goes to.
                        if let (page, space) = model.livePage(neighbor.id) ?? neighbor.shown {
                            SpaceSections(model: model, page: page, space: space)
                        } else {
                            Color.clear
                        }
                    }
                    .frame(width: width)
                }
            }
            .offset(x: shift)
            .onAppear { model.pageWidth = width }
            .onChange(of: width) { _, now in model.pageWidth = now }
        }
        .clipped()
    }
}

/// What a swipe past the last Space shows, like Arc's (its EdgeFeedbackView): along the sidebar's
/// right edge, 85 points wide, a light and a "+" slide in on a spring as the swipe passes the
/// last Space, the "+"'s ring filling as it goes.
struct CreateHint: View {
    @ObservedObject var model: AppModel
    /// How far in the light and the "+" have slid, from 0 to 1.
    private let _shown = State<CGFloat>(initialValue: 0)

    var body: some View {
        let share = model.creatingShare
        let shown = _shown.wrappedValue
        GeometryReader { geometry in
            let height = geometry.size.height
            ZStack(alignment: .topLeading) {
                // An oval twice as wide as the edge, centered on it once in, fading out from
                // its middle: Arc's radial gradient, from its key color at 22%.
                Rectangle()
                    .fill(
                        EllipticalGradient(
                            colors: [Color.white.opacity(0.22), Color.white.opacity(0)],
                            center: .center, startRadiusFraction: 0, endRadiusFraction: 0.5))
                    .frame(width: edgeWidth * 2, height: height)
                    .offset(x: (edgeWidth - 10) * (1 - shown))
                    .opacity(min(1, shown * 2))
                CreateDial(share: share, tint: model.accent)
                    .offset(x: edgeWidth + 5 - 47 * shown, y: (height - 30) / 2)
                    .opacity(min(1, shown / 0.2))
            }
            .frame(width: edgeWidth, height: height, alignment: .topLeading)
            .clipShape(RoundedRectangle(cornerRadius: 4))
            .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .allowsHitTesting(false)
        .onChange(of: share > 0) { _, now in
            withAnimation(.spring(response: 0.4, dampingFraction: 0.8)) {
                _shown.wrappedValue = now ? 1 : 0
            }
        }
    }

    private let edgeWidth: CGFloat = 85
}

/// The "+" a swipe past the last Space fills, like Arc's (its CircularProgressView): 30 points,
/// a disc in the theme's color with a white "+" and a white ring 2.5 points wide filling as the
/// swipe goes; once full, the ring thickens to fill the disc and the "+" takes the theme's color.
struct CreateDial: View {
    let share: CGFloat
    let tint: Color

    var body: some View {
        let full = share >= 1
        ZStack {
            Circle().fill(tint)
            Circle()
                .inset(by: 1.25)
                .trim(from: 0, to: min(1, share))
                .stroke(Color.white, lineWidth: full ? 30 : 2.5)
                .rotationEffect(.degrees(-90))
                // Core Animation's own duration, which Arc's change of width takes.
                .animation(.easeInOut(duration: 0.25), value: full)
            Image(systemName: "plus")
                .font(.system(size: 16, weight: .medium))
                .foregroundStyle(full ? tint : Color.white)
        }
        .frame(width: 30, height: 30)
        .clipShape(Circle())
        .shadow(color: .black.opacity(0.3), radius: 1.5, y: 1)
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
        var slots = page.favorites.map { FavoriteSlot(item: $0) }
        guard let slot = model.favoriteSlot, let dragged = model.dragged else { return slots }
        if let from = slots.firstIndex(where: { $0.item?.id == dragged }) {
            let moved = slots.remove(at: from)
            slots.insert(moved, at: min(from < slot ? slot - 1 : slot, slots.count))
        } else {
            slots.insert(FavoriteSlot(item: nil), at: min(slot, slots.count))
        }
        return slots
    }

    var body: some View {
        let count = page.favorites.count
        let favorites = page.roots.favorites
        if count == 0 {
            // Dropped on, the hint, or the room it leaves while a drag goes on, makes what the
            // drag carries the first favorite.
            let target = FavoritesTarget(
                model: model, tile: CGSize(width: Measure.tile, height: Measure.tile),
                hover: { model.favoritesTargeted = $0 != nil },
                drop: { dropFavorite($0, at: 0) })
            if page.favoritesHint {
                FavoritesHint(model: model, space: page.space)
                    .overlay(target.padding(.bottom, 8))
            } else if model.dragged != nil {
                // Without the hint, the empty favorites still take a drag, like Arc's.
                EmptyFavoritesDrop(model: model).overlay(target.padding(.bottom, 8))
            }
        } else {
            // Like Arc's, the tiles share one row, up to four, as a dragged one opens room.
            let shown = shownFavorites
            let rows = max(1, (shown.count + 3) / 4)
            let columns = max(1, (shown.count + rows - 1) / rows)
            let size = _size.wrappedValue
            let tileWidth =
                (size.width - Measure.inset * CGFloat(columns - 1)) / CGFloat(columns)
            LazyVGrid(
                columns: Array(
                    repeating: GridItem(.flexible(minimum: Measure.tile), spacing: Measure.inset),
                    count: columns),
                spacing: Measure.inset
            ) {
                ForEach(shown, id: \.self) { slot in
                    if let item = slot.item {
                        FavoriteTile(
                            model: model, item: item,
                            active: item.tabs.contains { $0.id == page.selected }
                        )
                        .opacity(item.id == model.dragged && model.favoriteSlot != nil ? 0 : 1)
                        .onDrag { model.startDrag(item) }
                    } else {
                        Color.clear.frame(height: Measure.tile)
                    }
                }
            }
            .animation(.easeInOut(duration: 0.12), value: shown)
            .onGeometryChange(for: CGSize.self) { $0.size } action: { _size.wrappedValue = $0 }
            .overlay(
                FavoritesTarget(
                    model: model,
                    tile: CGSize(width: max(Measure.tile, tileWidth), height: Measure.tile),
                    hover: { point in
                        model.favoriteSlot = point.map {
                            slot(at: $0, count: count, columns: columns, width: tileWidth)
                        }
                    },
                    drop: { dropFavorite($0, at: model.favoriteSlot) }))
            .padding(.bottom, 8)
        }
    }

    /// Where in the grid a drag at `point` lands: before the tile under it, or after it past its
    /// middle.
    private func slot(at point: CGPoint, count: Int, columns: Int, width: CGFloat) -> Int {
        let column = min(columns - 1, max(0, Int(point.x / (width + Measure.inset))))
        let row = max(0, Int(point.y / (Measure.tile + Measure.inset)))
        let past = point.x - CGFloat(column) * (width + Measure.inset) > width / 2
        return min(count, row * columns + column + (past ? 1 : 0))
    }

    /// Makes the node a drag carries a favorite at `slot`.
    private func dropFavorite(_ dragged: Int, at slot: Int?) -> Bool {
        model.favoriteSlot = nil
        model.favoritesTargeted = false
        _ = model.endDrag()
        guard let slot else { return false }
        model.move(dragged, to: page.roots.favorites, slot)
        return true
    }
}

/// The empty favorites' room while a drag goes on and their hint is hidden: a tile's height,
/// outlined, which lights up under the drag.
struct EmptyFavoritesDrop: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let targeted = model.favoritesTargeted
        RoundedRectangle(cornerRadius: Measure.radius)
            .fill(model.accent.opacity(targeted ? 0.2 : 0))
            .strokeBorder(
                model.accent.opacity(targeted ? 0.6 : 0.35),
                style: StrokeStyle(lineWidth: 1, dash: targeted ? [] : [4, 3]))
            .frame(height: Measure.tile)
            .padding(.bottom, 8)
            .animation(.easeInOut(duration: 0.12), value: targeted)
    }
}

/// A favorite's tile: a tab's icon or, like Arc's, a split's icons side by side.
struct FavoriteTile: View {
    @ObservedObject var model: AppModel
    let item: Item
    let active: Bool
    private var dark: Bool { isDark(model.space?.theme) }
    /// The tab under the pointer, among a split's.
    private let _hovered = State<Int?>(initialValue: nil)
    private var hovered: Int? {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        let tabs = item.tabs
        Group {
            if tabs.count > 1 && !active {
                // Like Arc's, an unselected split's tile is its tabs' own tiles, 2 points apart,
                // each lighting up under the pointer and showing its tab.
                HStack(spacing: 2) {
                    ForEach(Array(tabs.enumerated()), id: \.element.id) { index, tab in
                        part(tab, shape: partShape(index, of: tabs.count))
                    }
                }
            } else {
                // A tab's tile, or a selected split's: cut in equal parts by a 2-point line,
                // light grey like the outline in the dark.
                Button {
                    if let first = tabs.first { model.run("select/\(first.id)") }
                } label: {
                    HStack(spacing: 0) {
                        ForEach(Array(tabs.enumerated()), id: \.element.id) { index, tab in
                            if index > 0 {
                                Rectangle()
                                    .fill(dark ? Color(white: 0.85) : Ink.separator)
                                    .frame(width: 2)
                            }
                            Favicon(url: tab.favicon, size: tabs.count > 1 ? 13 : 20)
                                .frame(maxWidth: .infinity)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .frame(height: Measure.tile)
                    .background(
                        RoundedRectangle(cornerRadius: Measure.radius)
                            .fill(
                                active
                                    ? (dark ? Color.white.opacity(0.53) : model.selectedInk)
                                    : hovered != nil ? Ink.itemHovered : Ink.item)
                            .shadow(color: active ? model.selectedShadow : .clear, radius: 1, y: 1))
                    // Like Arc's, in the dark, the selected tile has a light grey outline.
                    .overlay(
                        RoundedRectangle(cornerRadius: Measure.radius)
                            .strokeBorder(Color(white: 0.85), lineWidth: 2)
                            .opacity(active && dark ? 1 : 0))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .onHover { hovered = $0 ? 0 : nil }
            }
        }
        .help(tabs.map(\.label).joined(separator: " | "))
        .rightClick { model.run("menu/node/\(item.id)") }
    }

    /// One tab's part of an unselected split's tile.
    private func part(_ tab: Tab, shape: UnevenRoundedRectangle) -> some View {
        Button {
            model.run("select/\(tab.id)")
        } label: {
            Favicon(url: tab.favicon, size: 13)
                .frame(maxWidth: .infinity)
                .frame(height: Measure.tile)
                .background(shape.fill(hovered == tab.id ? Ink.itemHovered : Ink.item))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { over in
            if over {
                hovered = tab.id
            } else if hovered == tab.id {
                hovered = nil
            }
        }
    }

    /// The tile's rounded corners on the outer sides of its first and last parts.
    private func partShape(_ index: Int, of count: Int) -> UnevenRoundedRectangle {
        let radius = Measure.radius
        let leading: CGFloat = index == 0 ? radius : 0
        let trailing: CGFloat = index == count - 1 ? radius : 0
        return UnevenRoundedRectangle(
            topLeadingRadius: leading, bottomLeadingRadius: leading,
            bottomTrailingRadius: trailing, topTrailingRadius: trailing)
    }
}

/// The empty favorites' invitation to drag tabs there; its × hides it for the profile until it
/// has favorites again.
struct FavoritesHint: View {
    @ObservedObject var model: AppModel
    let space: Int

    private var dark: Bool { isDark(model.space?.theme) }

    var body: some View {
        let targeted = model.favoritesTargeted
        VStack(spacing: 4) {
            // Like Arc's: in the dark, a star in the accent, darkened, on a white disc, which
            // glows; in the light, a white star on a disc in the accent.
            Image(systemName: "star.circle.fill")
                .symbolRenderingMode(.palette)
                .foregroundStyle(
                    dark
                        ? Color(
                            nsColor: NSColor(model.accent).blended(withFraction: 0.5, of: .black)!)
                        : .white,
                    dark ? .white : model.accent)
                .font(.system(size: 16))
                .shadow(color: model.accent.opacity(dark ? 0.8 : 0), radius: 4)
            // Our own words, as long as Arc's: two lines at its narrowest.
            Text("Drag tabs to Favorites").font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Ink.primary)
            Text("Favorites stay one click away in every Space")
                .font(.system(size: 12))
                .foregroundStyle(Ink.hint)
                .multilineTextAlignment(.center)
        }
        // Like Arc's, under a drag, the words fade behind the tile it shows.
        .opacity(targeted ? 0.5 : 1)
        .padding([.horizontal, .top], 12)
        .padding(.bottom, 10.5)
        .frame(maxWidth: .infinity)
        // Dashed, like Arc's.
        .background(
            RoundedRectangle(cornerRadius: Measure.radius)
                .strokeBorder(
                    dark ? Color.white.opacity(0.3) : model.accent.opacity(0.35),
                    style: StrokeStyle(lineWidth: 1, dash: targeted ? [] : [4, 3])))
        // Like Arc's, in the dark, the accent glows inside the border.
        .background(
            RoundedRectangle(cornerRadius: Measure.radius)
                .strokeBorder(model.accent.opacity(dark ? 0.25 : 0), lineWidth: 4)
                .blur(radius: 4)
                .clipShape(RoundedRectangle(cornerRadius: Measure.radius)))
        .overlay(alignment: .topTrailing) {
            IconButton(symbol: "xmark", label: "Hide this hint", size: 20, symbolSize: 10) {
                model.run("favorites/hide-hint/\(space)")
            }
            .padding(4)
        }
        .padding(.bottom, 8)
        .animation(.easeInOut(duration: 0.12), value: targeted)
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
                    if hovered || space.emoji != nil {
                        // Like Arc's, the pointer over the Space shows the chevron that folds
                        // its pinned tabs, in the emoji's place or pushing its name aside.
                        Group {
                            if hovered {
                                Image(systemName: "chevron.right")
                                    .rotationEffect(.degrees(page.collapsed ? 0 : 90))
                                    .font(.system(size: 11, weight: .semibold))
                            } else if let emoji = space.emoji {
                                SpaceGlyph(icon: emoji, size: 14)
                            }
                        }
                        .frame(width: 24)
                        .transition(.opacity)
                    }
                    if editing == "name" {
                        SpaceField(model: model, space: page.space, value: space.title)
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
        // Measured on Arc's: its icon 19.5 points under the favorites, 38 over the divider.
        .frame(height: 39)
        .foregroundStyle(.secondary)
        // Like Arc's, the whole row lights up under the pointer.
        .background(
            RoundedRectangle(cornerRadius: Measure.radius).fill(hovered ? Ink.item : .clear))
        .animation(.easeInOut(duration: 0.12), value: hovered)
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

/// Edits a Space's name in place; its icon has its own picker (EmojiPicker).
struct SpaceField: View {
    @ObservedObject var model: AppModel
    let space: Int
    let value: String
    private let _text = State<String>(initialValue: "")
    @FocusState private var focused: Bool

    var body: some View {
        TextField("", text: _text.projectedValue)
            .textFieldStyle(.plain)
            .focused($focused)
            .background(TakesKeyboard())
            .onAppear {
                _text.wrappedValue = value
                DispatchQueue.main.async { focused = true }
            }
            .onSubmit { done(_text.wrappedValue) }
            .onExitCommand { model.editingSpace = nil }
            .onChange(of: focused) { _, now in
                // Clicked away: a name is kept, an icon not picked is left as it was.
                guard !now, model.editingSpace != nil else { return }
                done(_text.wrappedValue)
            }
    }

    private func done(_ text: String) {
        guard model.editingSpace != nil else { return }
        model.editingSpace = nil
        let value = text.trimmingCharacters(in: .whitespaces)
        if !value.isEmpty, value != self.value {
            model.run("space/rename/\(space)/\(percentEncoded(value))")
        }
    }
}

/// Between the pinned tabs and today's: hovered, it offers to close today's tabs.
struct DividerRow: View {
    @ObservedObject var model: AppModel
    let clearable: Bool
    /// The pinned tabs' container, whose drop line shows on the divider while they are empty.
    var root: Int? = nil
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        HStack(spacing: 6) {
            Rectangle().fill(Ink.divider).frame(height: 1)
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
        .overlay(alignment: .top) {
            if let root, model.dropTarget == .into(root) { InsertionLine(tint: model.themeColor) }
        }
        // Like Arc's, New Tab's label 23.5 points under the line.
        .padding(.bottom, -2.5)
    }
}

struct NewTabRow: View {
    @ObservedObject var model: AppModel
    /// Today's container, whose drop line shows under New Tab while it is empty.
    var root: Int? = nil
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
        .font(.system(size: 14))
        .foregroundStyle(Ink.secondary)
        .onHover { hovered = $0 }
        .overlay(alignment: .bottom) {
            if let root, model.dropTarget == .into(root) {
                InsertionLine(tint: model.themeColor).offset(y: 4)
            }
        }
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
                // Like Arc's, a folder opening slides its rows down from under it.
                ItemRows(
                    model: model, items: children, parent: id, selected: selected,
                    pinned: pinned, depth: depth + 1)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
    }
}

/// A folder's icon, like Arc's: in the theme's color, its back lighter, outlined, its front
/// leaning open once the folder is.
struct FolderIcon: View {
    let open: Bool
    let tint: Color

    var body: some View {
        Canvas { context, size in
            let (w, h) = (size.width, size.height)
            let back = Color(nsColor: NSColor(tint).blended(withFraction: 0.5, of: .white) ?? .white)
            let outline = Color(nsColor: NSColor(tint).blended(withFraction: 0.8, of: .white) ?? .white)
            // The back, with its tab at the top left.
            var rear = Path()
            rear.addRoundedRect(
                in: CGRect(x: 0.5, y: h * 0.12, width: w - 1, height: h * 0.8),
                cornerSize: CGSize(width: 2, height: 2))
            rear.addRoundedRect(
                in: CGRect(x: 0.5, y: h * 0.04, width: w * 0.42, height: h * 0.2),
                cornerSize: CGSize(width: 1.5, height: 1.5))
            context.fill(rear, with: .color(back))
            // The front, straight while closed, leaning once open.
            var front = Path()
            if open {
                front.move(to: CGPoint(x: w * 0.2, y: h * 0.38))
                front.addLine(to: CGPoint(x: w - 0.5, y: h * 0.38))
                front.addLine(to: CGPoint(x: w * 0.8, y: h * 0.92))
                front.addLine(to: CGPoint(x: 0.5, y: h * 0.92))
                front.closeSubpath()
            } else {
                front.addRoundedRect(
                    in: CGRect(x: 0.5, y: h * 0.3, width: w - 1, height: h * 0.62),
                    cornerSize: CGSize(width: 2, height: 2))
            }
            context.fill(front, with: .color(tint))
            context.stroke(front, with: .color(outline), lineWidth: 1)
            context.stroke(rear, with: .color(outline), lineWidth: 1)
        }
        .frame(width: 16, height: 14)
    }
}

/// The last row `items` show, inside an open folder at their end, if any.
func lastShown(_ items: [Item]) -> Int? {
    guard let last = items.last else { return nil }
    if case .folder(_, _, true, let children) = last, let inner = lastShown(children) {
        return inner
    }
    return last.id
}

/// The open folders among `items`, as deep as they go.
func openFolders(_ items: [Item]) -> [Int] {
    items.flatMap { item -> [Int] in
        guard case .folder(let id, _, let expanded, let children) = item, expanded else { return [] }
        return [id] + openFolders(children)
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
                VStack(alignment: .leading, spacing: 0) {
                    Text(tab.label).lineLimit(1).opacity(tab.asleep == true ? 0.55 : 1)
                    if model.renameHint == tab.id {
                        Text("Double-click to rename")
                            .font(.system(size: 11))
                            .foregroundStyle(Ink.secondary)
                            .lineLimit(1)
                    }
                }
                .animation(.easeInOut(duration: 0.15), value: model.renameHint == tab.id)
                if tab.muted == true {
                    Image(systemName: "speaker.slash").font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                if hovered || active {
                    // Like Arc's: a minus for an open pinned tab, which closes it and stays,
                    // an x for the others, which removes a closed pinned tab.
                    CloseButton(
                        symbol: pinned && active ? "minus" : "xmark", help: "Close \(tab.label)"
                    ) {
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
                        ? model.selectedInk
                        : model.marked.contains(tab.id) ? Ink.marked : hovered ? Ink.item : .clear)
                .shadow(color: active ? model.selectedShadow : .clear, radius: 1, y: 1))
        .padding(.vertical, (Measure.row - Measure.rowFill) / 2)
        .contentShape(Rectangle())
        .onTapGesture {
            model.click(tab.id)
            // Like Arc's, a double-click renames it, without delaying a click to select it.
            if NSApp.currentEvent?.clickCount == 2 { model.renaming = tab.id }
        }
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

/// A click collapses or expands the folder; its menu's Rename edits its title.
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
            FolderIcon(open: expanded, tint: model.accent)
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
        // Like Arc's, a click opens or closes it at once; its menu renames it.
        .onTapGesture { model.run("toggle/\(id)") }
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
    /// Whether the field had the keyboard: until then, losing it ends nothing.
    private let _began = State<Bool>(initialValue: false)

    var body: some View {
        TextField("", text: _text.projectedValue)
            .textFieldStyle(.plain)
            .focused($focused)
            // The sidebar's panel takes the keyboard first, which a click on a button there
            // doesn't give it: the field would lose it as it shows.
            .background(TakesKeyboard())
            .onAppear {
                text = title
                DispatchQueue.main.async { focused = true }
            }
            .onSubmit { focused = false }
            .onExitCommand {
                text = title
                focused = false
            }
            .onChange(of: focused) { _, now in
                if now { _began.wrappedValue = true }
                guard !now, _began.wrappedValue else { return }
                let value = text.trimmingCharacters(in: .whitespaces)
                if (!value.isEmpty || blank) && value != title {
                    model.run("rename/\(id)/\(percentEncoded(value))")
                }
                model.renaming = nil
            }
    }
}

/// Makes its window the key one as it shows in it.
struct TakesKeyboard: NSViewRepresentable {
    func makeNSView(context: Context) -> View { View() }
    func updateNSView(_ view: View, context: Context) {}

    final class View: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            window?.makeKey()
        }
    }
}

/// The room a split tab's title needs to show at all: two or three letters and their ellipsis.
private let minTitle: CGFloat = 30
/// The room an ellipsis alone needs, standing for the title.
private let minEllipsis: CGFloat = 14

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
                .fill(active ? model.selectedInk : hovered ? Ink.item : .clear)
                .shadow(color: active ? model.selectedShadow : .clear, radius: 1, y: 1))
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
        // Like Arc's, as the row narrows, the title shortens, then an ellipsis stands for it, then
        // it goes, the icon staying where it is; on hover, the button shows
        // at the right, only where it has room, the title shortening beside it.
        GeometryReader { geometry in
            let width = geometry.size.width
            let leading: CGFloat = width >= 8 + 14 + 6 + minTitle + 4 ? 8 : 4
            let trailing = leading / 2
            let showsButton = hovered && width >= leading + 14 + 2 + 22 + trailing
            // What is left of the row for the title, past the icon, the button and the margins.
            let titleRoom = width - leading - 14 - 6 - trailing - (showsButton ? 2 + 22 : 0)
            HStack(spacing: 0) {
                Favicon(url: tab.favicon, size: 14)
                // Like Arc's, an ellipsis alone stands for the title once its letters no longer
                // fit.
                if titleRoom >= minEllipsis {
                    Text(titleRoom >= minTitle ? tab.label : "…")
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(width: titleRoom, alignment: .leading)
                        .padding(.leading, 6)
                }
                Spacer(minLength: 0)
                if showsButton {
                    let separate = model.optionHeld
                    ChipButton(
                        symbol: separate ? "rectangle.lefthalf.inset.filled.arrow.left" : "xmark",
                        help: separate ? "Separate from Split View" : "Close \(tab.label)"
                    ) {
                        model.run(separate ? "separate/\(tab.id)" : "close/\(tab.id)")
                    }
                    .padding(.leading, 2)
                }
            }
            .padding(.leading, leading)
            .padding(.trailing, trailing)
            .frame(width: width, height: 28, alignment: .leading)
            .clipped()
        }
        .font(.system(size: 14))
        .frame(height: 28)
        // Like Arc's, each tab of the split shown has its bubble.
        .background(RoundedRectangle(cornerRadius: 8).fill(active ? Ink.child : .clear))
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
            // Like Arc's, the minus 13 points wide, wider than the x.
            Image(systemName: symbol)
                .font(.system(size: symbol == "minus" ? 16 : 11, weight: symbol == "minus" ? .medium : .semibold))
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
                    .padding(size / 16)
            }
        }
        .frame(width: size, height: size)
    }
}

/// The library button, one icon per Space, and the "+" that creates a Space.
struct Footer: View {
    @ObservedObject var model: AppModel

    private let _menuShown = State<Bool>(initialValue: false)
    private var menuShown: Bool {
        get { _menuShown.wrappedValue }
        nonmutating set { _menuShown.wrappedValue = newValue }
    }
    /// The menu's and the "+"'s frames, from the window's top left corner.
    private let _menuFrame = State<CGRect>(initialValue: .zero)
    private let _plusFrame = State<CGRect>(initialValue: .zero)
    private let _clicks = State<Any?>(initialValue: nil)

    /// Like Arc's, a new Space's form leaves only the Spaces' icons.
    private var drafting: Bool { model.draftSpace != nil }

    var body: some View {
        HStack(spacing: 0) {
            IconButton(symbol: "books.vertical", label: "Library") {
                model.openLibrary(nil)
            }
            .opacity(drafting ? 0 : 1)
            .disabled(drafting)
            SpaceIcons(model: model)
                .padding(.horizontal, 5)
            // Like Arc's, the "+" opens its own menu, turning into a "×" while it shows.
            PlusButton(shown: menuShown, active: model.active) { setMenu(!menuShown) }
                .padding(.trailing, 2)
                .help(menuShown ? "" : "New")
                .opacity(drafting ? 0 : 1)
                .disabled(drafting)
                .background(Frame(frame: _plusFrame.projectedValue))
                .popover(isPresented: $model.profilesShown, arrowEdge: .top) {
                    ProfilesView(model: model)
                }
        }
        .frame(height: 40)
        .foregroundStyle(model.buttonInk)
        .overlay(alignment: .bottomTrailing) {
            if menuShown {
                PlusMenu(model: model) { setMenu(false) }
                    .background(Frame(frame: _menuFrame.projectedValue))
                    .padding(.trailing, 2 + 14 - plusMenuArrowTrailing)
                    // The arrow's tip over the "+"'s center, 20 points over the footer's bottom.
                    .padding(.bottom, 20 + plusMenuAbovePlus)
                    .fixedSize()
                    .transition(
                        .asymmetric(
                            insertion: .scale(scale: 0.95, anchor: .bottomTrailing)
                                .combined(with: .opacity)
                                .animation(.easeOut(duration: 0.05)),
                            removal: .scale(scale: 0.95, anchor: .bottomTrailing)
                                .combined(with: .opacity)
                                .animation(.easeIn(duration: 0.1))))
            }
        }
        // Measured on Arc's: its icons' centers 25 points over the window's bottom.
        .padding(.bottom, 5)
        .onDisappear { setMenu(false) }
    }

    /// Shows or hides the "+" menu, which a click anywhere else closes.
    private func setMenu(_ shown: Bool) {
        if let monitor = _clicks.wrappedValue {
            NSEvent.removeMonitor(monitor)
            _clicks.wrappedValue = nil
        }
        withAnimation { menuShown = shown }
        guard shown else { return }
        _clicks.wrappedValue = NSEvent.addLocalMonitorForEvents(
            matching: [.leftMouseDown, .rightMouseDown]
        ) { event in
            let inside: Bool = {
                guard let content = event.window?.contentView else { return false }
                // SwiftUI's frames are from the window's top left corner.
                let point = NSPoint(
                    x: event.locationInWindow.x,
                    y: content.bounds.height - event.locationInWindow.y)
                return _menuFrame.wrappedValue.contains(point) || _plusFrame.wrappedValue.contains(point)
            }()
            if !inside { setMenu(false) }
            return event
        }
    }
}

/// The footer's "+", turned into a "×" while its menu shows.
struct PlusButton: View {
    let shown: Bool
    let active: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            // At Arc's size, measured; like Arc's, it turns a quarter of a half turn into a "×".
            Image(systemName: "plus")
                .font(.system(size: 17, weight: .regular))
                .rotationEffect(.degrees(shown ? 45 : 0))
                .animation(.easeInOut(duration: 0.2), value: shown)
                .frame(width: 28, height: 28)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(active || shown ? 1 : 0.3)
    }
}

/// Keeps a view's frame from the window's top left corner.
struct Frame: View {
    @Binding var frame: CGRect

    var body: some View {
        GeometryReader { geometry in
            Color.clear
                .onAppear { frame = geometry.frame(in: .global) }
                .onChange(of: geometry.frame(in: .global)) { _, new in frame = new }
        }
    }
}

/// The Spaces' icons, measured on Arc's: alone, a Space shows none; otherwise each takes up to
/// 34 points, centered, and when they don't fit they share the room, the displayed one keeping
/// at least 22.5 points.
struct SpaceIcons: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let spaces = model.state?.spaces ?? []
        GeometryReader { geometry in
            let count = CGFloat(spaces.count)
            let room = geometry.size.width
            let roomy = count * 34 <= room
            let even = min(34, room / max(count, 1))
            let displayed = roomy ? even : max(even, 22.5)
            let other = roomy ? even : (room - displayed) / max(count - 1, 1)
            HStack(spacing: 0) {
                ForEach(spaces) { space in
                    let shown = space.id == model.state?.space
                    SpaceIcon(model: model, space: space, displayed: shown, slot: shown ? displayed : other)
                        .transition(.scale.combined(with: .opacity))
                }
            }
            .frame(width: room, height: geometry.size.height)
            // Like Arc's, a new Space's icon grows in as the others make room, and they resize
            // as the displayed one changes.
            .animation(.easeInOut(duration: spaceSlide), value: spaces.map(\.id))
            .animation(.easeInOut(duration: spaceSlide), value: model.state?.space)
        }
        .frame(height: 24)
        .opacity(spaces.count > 1 ? 1 : 0)
    }
}

/// A Space's emoji, or a dot without one: a click displays the Space, a right click opens its
/// menu, like Arc's.
struct SpaceIcon: View {
    @ObservedObject var model: AppModel
    let space: Space
    let displayed: Bool
    /// The room it takes along the footer.
    let slot: CGFloat
    private let _hovered = State<Bool>(initialValue: false)
    private var hovered: Bool {
        get { _hovered.wrappedValue }
        nonmutating set { _hovered.wrappedValue = newValue }
    }

    var body: some View {
        Button {
            model.slide(to: space.id)
        } label: {
            Group {
                if let emoji = space.emoji {
                    SpaceGlyph(icon: emoji, size: 12).opacity(displayed ? 1 : 0.5)
                } else {
                    // Measured on Arc's: 8 points with room, 4 without, dimmed while the
                    // window is inactive.
                    let size: CGFloat = slot >= 22.5 ? 8 : 4
                    Circle()
                        .fill(.primary.opacity((displayed ? 0.5 : 0.18) * (model.active ? 1 : 0.5)))
                        .frame(width: size, height: size)
                }
            }
            .frame(width: min(24, slot), height: 24)
            .background(
                RoundedRectangle(cornerRadius: 6).fill(.primary.opacity(hovered ? 0.1 : 0)))
            .frame(width: slot)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(space.title)
        .onHover { hovered = $0 }
        .rightClick { model.run("menu/space/\(space.id)") }
        // Like Arc's, a drag resting on a Space's icon slides to that Space, to drop there.
        .onDrop(of: [.plainText], delegate: SpaceIconDrop(model: model, space: space.id))
        // Like Arc's, Change Space Icon… opens its picker above the Space's icon.
        .popover(
            isPresented: Binding(
                get: { model.editingSpace == SpaceEdit(space: space.id, field: "emoji") },
                set: { if !$0 { model.editingSpace = nil } }),
            arrowEdge: .top
        ) {
            EmojiPicker(model: model, space: space.id)
        }
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

/// Where the sidebar's toggle's symbol starts beside the window's buttons: past the last one, as
/// far as they stand apart.
private let togglePlace: CGFloat = windowButtonsLeading + 3 * windowButtonsSpacing

/// The room behind views' buttons, which moves the window like Arc's, as its titlebar would:
/// SwiftUI hands it the clicks they do not take. In the tab list's empty room, a double-click
/// does not zoom it; behind the toolbar, it does.
struct WindowDragArea: NSViewRepresentable {
    var doubleClicks = false

    func makeNSView(context: Context) -> WindowDrag {
        let view = WindowDrag()
        view.doubleClicks = doubleClicks
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
    // In the sidebar, a view of the main window, from the sidebar's top left corner.
    if let from = NSApp.currentEvent?.window, from.parent == nil,
        let origin = sidebarOrigins[ObjectIdentifier(from)]
    {
        rect = frame.offsetBy(dx: origin.x, dy: origin.y)
    } else if let from = NSApp.currentEvent?.window, let main = from.parent,
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
