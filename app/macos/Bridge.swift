// The native macOS interface's side of the bridge with the app (app/src/native.rs): the app
// attaches it to its window, gives it the events the web UI gets, and runs the commands it sends
// back, the same as the web UI's (app/src/command.rs).

import AppKit
import SwiftUI

/// Runs a command from a window, given as its path, like `select/12`.
public typealias Send = @convention(c) (UInt32, UnsafePointer<CChar>) -> Void

/// What the interface knows of the app, from its events.
@MainActor
final class AppModel: ObservableObject {
    @Published var state: AppState?
    /// The folder or the tab whose name is being edited.
    @Published var renaming: Int?
    /// The Space whose name or icon ("name" or "emoji") is edited in its header.
    @Published var editingSpace: SpaceEdit?
    /// The new Space whose form the sidebar shows (SpaceDraft.swift).
    @Published var draftSpace: Int?
    /// The Spaces there were when one was asked for, until it shows.
    var awaitingDraft: Set<Int>?
    @Published var downloads: [Download] = []
    @Published var archive: [ArchivedTab] = []
    @Published var history: [Visit] = []
    /// The sidebar shows the library, on a section, under what its field searches.
    @Published var libraryShown = false
    /// The list of profiles shows above the footer.
    @Published var profilesShown = false
    /// The node a drag in the sidebar carries, whether a tab, and where it would land
    /// (SidebarDrop.swift); once the drag ends, nowhere shows where it would land.
    @Published var dragged: Int? {
        didSet { if dragged == nil { dropTarget = nil } }
    }
    var draggedTab = false
    /// The tabs a drag carries, several when they were selected at once.
    var draggedTabs: [Int] = []
    /// What the drag carries, as it was when it started, which a drag switching Spaces leaves.
    var draggedItem: Item?
    /// A drag resting on a Space's icon, until it slides there.
    var spaceWait: DispatchWorkItem?
    /// The tabs selected at once with ⌘ or ⇧, besides the one shown, like Arc's; none otherwise.
    @Published var marked: [Int] = []
    /// Where the card is, from the window's top left corner, which the window is shaded around and
    /// washed under while it shows no page (Backdrop); nil while a split shows.
    @Published var card: NSRect?
    /// Whether the card shows no page.
    @Published var cardEmpty = false
    /// Whether the app is in front, which shows the window's blur (Backdrop).
    @Published var active = NSApp.isActive
    /// The Space switch a drag resting at the sidebar's edge waits for, and where it rested.
    var edgeWait: DispatchWorkItem?
    var edgeAt: NSPoint?
    @Published var dropTarget: DropTarget?
    /// Where in the favorites a drag would land.
    @Published var favoriteSlot: Int?
    /// Whether a drag is over the empty favorites.
    @Published var favoritesTargeted = false
    /// The tab just pinned, which says how to rename it.
    @Published var renameHint: Int?
    private var renameHintWait: DispatchWorkItem?
    /// Option is held: a split's tabs offer to leave it rather than close.
    @Published var optionHeld = false
    private var flags: Any?
    private var appearance: NSKeyValueObservation?
    /// A split a tab resting on another's middle will offer, once it rests long enough.
    var pendingSplit: DropTarget?
    var splitWait: DispatchWorkItem?
    /// How far a two-finger swipe has moved the displayed Space's page, in points.
    @Published var swipe: CGFloat = 0
    /// The width of a Space's page in the sidebar, which a swipe slides by.
    var pageWidth: CGFloat = 0
    /// The Space a finished swipe slid to, until the app shows it.
    private var swipedTo: Int?
    /// The Spaces a click carries the page across, left to right, while it does, like Arc's
    /// carousel, and how far along it is, from 0 at the left to 1 at the right.
    @Published var carousel: [(id: Int, shown: (SpacePage, Space)?)]?
    @Published var carouselShare: CGFloat = 0
    /// The last page the app sent of each Space, which a carousel shows passing by.
    private var seenPages: [Int: SpacePage] = [:]
    @Published var section = LibrarySection.spaces
    @Published var query = "" {
        didSet { if section == .history { searchHistory() } }
    }
    /// The window the model is the interface of, which its commands come from.
    let window: UInt32
    /// Whether the sidebar floats above the page, which then shows the window's buttons.
    @Published var sidebarFloating = false
    /// Whether the window shows in full screen, without its buttons.
    @Published var fullScreen = false
    /// Whether the sidebar is docked, which keeps its toggle from the bar above the page.
    @Published var sidebarDocked = true
    /// The split's pages shown, from the card's top left corner, each loading above its own.
    @Published var panes: [Pane] = []
    let send: Send

    init(window: UInt32, send: @escaping Send) {
        self.window = window
        self.send = send
        flags = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { [weak self] event in
            self?.optionHeld = event.modifierFlags.contains(.option)
            return event
        }
        // A theme in automatic appearance follows the system's: the views read it again.
        appearance = NSApp.observe(\.effectiveAppearance) { [weak self] _, _ in
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.state = self.state
                self.paintWindow()
            }
        }
    }

    deinit {
        if let flags { NSEvent.removeMonitor(flags) }
    }

    func run(_ path: String) {
        path.withCString { send(window, $0) }
    }

    /// The displayed Space's page.
    var page: SpacePage? {
        guard let state else { return nil }
        return state.pages.first { $0.space == state.space }
    }

    var space: Space? {
        guard let state else { return nil }
        return state.spaces.first { $0.id == state.space }
    }

    /// The displayed Space's selected tab, a favorite, a pinned tab or one of today's.
    var activeTab: Tab? {
        guard let page else { return nil }
        let all = (page.favorites + page.pinned + page.today).flatMap(\.tabs)
        return all.first { $0.id == page.selected }
    }

    /// The window's colors last sent, so as to send them only when they change.
    private var colors = ""

    /// An event dispatched to the web UI's sidebar.
    func handle(_ name: String, _ detail: Data) {
        switch name {
        case "arbalete:state":
            do {
                state = try JSONDecoder().decode(AppState.self, from: detail)
            } catch {
                NSLog("arbalete: cannot read the state: \(error)")
            }
            paintWindow()
            followDraft()
            for page in state?.pages ?? [] { seenPages[page.space] = page }
            // The page slid to is now the displayed one, in the middle.
            if let swipedTo, state?.space == swipedTo {
                self.swipedTo = nil
                var still = Transaction()
                still.disablesAnimations = true
                withTransaction(still) { swipe = 0 }
            }
        case "arbalete:rename":
            renaming = try? JSONDecoder().decode(Int.self, from: detail)
        case "arbalete:new-space":
            newSpace(moving: try? JSONDecoder().decode(Int.self, from: detail))
        case "arbalete:edit-space":
            editingSpace = try? JSONDecoder().decode(SpaceEdit.self, from: detail)
        case "arbalete:downloads":
            downloads = (try? JSONDecoder().decode([Download].self, from: detail)) ?? downloads
        case "arbalete:archive":
            archive = (try? JSONDecoder().decode([ArchivedTab].self, from: detail)) ?? archive
        case "arbalete:history":
            struct Found: Decodable {
                var text: String
                var entries: [Visit]
            }
            if let found = try? JSONDecoder().decode(Found.self, from: detail), found.text == query {
                history = found.entries
            }
        case "arbalete:swipe":
            struct Gesture: Decodable {
                var phase: String
                var dx: Double
            }
            if let gesture = try? JSONDecoder().decode(Gesture.self, from: detail) {
                swiped(gesture.phase, gesture.dx)
            }
        case "arbalete:rename-profile":
            profilesShown = true
        case "arbalete:library":
            openLibrary(.spaces)
        case "arbalete:open-library":
            openLibrary(nil)
        case "arbalete:show-downloads":
            openLibrary(.downloads)
        case "arbalete:show-history":
            openLibrary(.history)
        case "arbalete:show-extensions":
            openLibrary(.extensions)
        default:
            break
        }
    }

    var queryBinding: Binding<String> {
        Binding(get: { self.query }, set: { self.query = $0 })
    }

    /// A swipe pages between the Spaces, like Arc's: past a third of the sidebar, it shows the
    /// next or the previous one; before the first, the library.
    private func swiped(_ phase: String, _ dx: Double) {
        // An incognito window has no other Space to swipe to.
        guard state?.incognito != true else { return }
        switch phase {
        case "begin":
            swipe = 0
        case "move":
            swipe += dx
        default:
            let threshold = swipeThreshold
            // Like Arc's, a new Space's form only goes back, rightward, which drops the Space:
            // only its button creates it.
            if let draft = draftSpace {
                if swipe > threshold {
                    draftSpace = nil
                    run("space/delete/\(draft)")
                }
                withAnimation(.spring(duration: 0.3)) { swipe = 0 }
                return
            }
            guard let state, let index = state.spaces.firstIndex(where: { $0.id == state.space })
            else { return }
            let (previous, next) = (index > 0, index < state.spaces.count - 1)
            if (swipe > threshold && previous) || (swipe < -threshold && next) {
                // Like Arc's, the page carries on to its neighbor, which the app then shows.
                let forward = swipe < 0
                swipedTo = state.spaces[index + (forward ? 1 : -1)].id
                withAnimation(.easeOut(duration: 0.25)) {
                    swipe = forward ? -pageWidth : pageWidth
                } completion: {
                    self.run(forward ? "space/next" : "space/previous")
                    // Should the app not show it, the page comes back.
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                        guard self.swipedTo != nil else { return }
                        self.swipedTo = nil
                        withAnimation(.spring(duration: 0.3)) { self.swipe = 0 }
                    }
                }
            } else {
                if swipe > threshold, index == 0 { openLibrary(nil) }
                // Like Arc's, past the last Space, the swipe creates one.
                if swipe < -threshold, !next { newSpace() }
                withAnimation(.spring(duration: 0.3)) { swipe = 0 }
            }
        }
    }

    /// The page the app last sent of a Space, with the Space, if it sent one.
    func livePage(_ space: Int) -> (SpacePage, Space)? {
        guard let state, let page = state.pages.first(where: { $0.space == space }),
            let space = state.spaces.first(where: { $0.id == space })
        else { return nil }
        return (page, space)
    }

    /// Displays a Space, like Arc's: the page slides across the Spaces between, a carousel.
    func slide(to space: Int) {
        guard let state, space != state.space,
            let from = state.spaces.firstIndex(where: { $0.id == state.space }),
            let to = state.spaces.firstIndex(where: { $0.id == space })
        else { return run("space/\(space)") }
        let forward = to > from
        carousel = state.spaces[min(from, to)...max(from, to)].map { space in
            (space.id, seenPages[space.id].map { ($0, space) })
        }
        var still = Transaction()
        still.disablesAnimations = true
        withTransaction(still) { carouselShare = forward ? 0 : 1 }
        run("space/\(space)")
        // Longer the more Spaces it passes, a little.
        let duration = min(0.5, spaceSlide + 0.05 * Double(abs(to - from) - 1))
        withAnimation(.easeInOut(duration: duration)) {
            carouselShare = forward ? 1 : 0
        } completion: {
            self.carousel = nil
        }
    }

    /// How far a swipe goes before it pages, or opens the library or a new Space.
    var swipeThreshold: CGFloat { 80 }

    /// Whether a swipe goes past the first Space, rightward, or the last one, leftward.
    private var swipePastEdge: Bool {
        guard let state, let index = state.spaces.firstIndex(where: { $0.id == state.space })
        else { return false }
        return (swipe > 0 && index == 0) || (swipe < 0 && index == state.spaces.count - 1)
    }

    /// How far the pages slide: with the fingers, or a fifth of that past the first or the last
    /// Space, where they resist like Arc's.
    var pagerShift: CGFloat { swipePastEdge ? swipe / 5 : swipe }

    /// How far a swipe past the first or the last Space moves the whole sidebar's content.
    var edgeShift: CGFloat { swipePastEdge ? pagerShift : 0 }

    /// How far a swipe past the last Space has gone toward creating one, from 0 to 1.
    var creatingShare: CGFloat {
        // A new Space's form creates nothing more, like Arc's.
        draftSpace == nil && swipe < 0 && swipePastEdge ? min(1, -swipe / swipeThreshold) : 0
    }

    /// The theme of the Space a swipe brings in, and how far it has come, from 0 to 1.
    func swipeNeighbor() -> (Theme?, Double) {
        guard swipe != 0, pageWidth > 0, let state,
            let index = state.spaces.firstIndex(where: { $0.id == state.space })
        else { return (nil, 0) }
        let at = index + (swipe < 0 ? 1 : -1)
        guard state.spaces.indices.contains(at) else { return (nil, 0) }
        return (state.spaces[at].theme, min(1, abs(swipe) / pageWidth))
    }

    /// The pages of the displayed Space's previous neighbor, itself and its next neighbor, if
    /// the app sent them.
    /// Each is known by its Space, so that the page slid to stays in place once displayed; a
    /// missing one by its side.
    func neighbors() -> [(id: Int, shown: (SpacePage, Space)?)] {
        guard let state, let index = state.spaces.firstIndex(where: { $0.id == state.space })
        else { return [] }
        return (-1...1).map { step in
            let at = index + step
            guard state.spaces.indices.contains(at) else { return (Int.min + 1 + step, nil) }
            let space = state.spaces[at]
            return (space.id, state.pages.first { $0.space == space.id }.map { ($0, space) })
        }
    }

    /// A favorite, tab, folder or split of the displayed Space's page.
    func item(_ id: Int) -> Item? {
        guard let page else { return nil }
        func find(_ items: [Item]) -> Item? {
            for item in items {
                if item.id == id { return item }
                if case .folder(_, _, _, let children) = item, let found = find(children) {
                    return found
                }
                if case .split(_, let tabs) = item, let tab = tabs.first(where: { $0.id == id }) {
                    return .tab(tab)
                }
            }
            return nil
        }
        // What a drag carries from another Space, which it switched from.
        return find(page.favorites + page.pinned + page.today)
            ?? draggedItem.flatMap { item in
                item.id == id ? item : item.tabs.first { $0.id == id }.map { .tab($0) }
            }
    }

    /// Where a node is in the displayed Space's page: its parent and its index among the
    /// parent's children.
    func place(of id: Int) -> (parent: Int, index: Int)? {
        guard let page else { return nil }
        if let index = page.favorites.firstIndex(where: { $0.id == id }) {
            return (page.roots.favorites, index)
        }
        func find(_ items: [Item], _ parent: Int) -> (Int, Int)? {
            for (index, item) in items.enumerated() {
                if item.id == id { return (parent, index) }
                if case .folder(let folder, _, _, let children) = item,
                    let found = find(children, folder)
                {
                    return found
                }
            }
            return nil
        }
        return find(page.pinned, page.roots.pinned) ?? find(page.today, page.roots.today)
    }

    /// Moves a dragged node before the child `index` of `parent`, or after its last one: the app
    /// counts the index once the node has left its place.
    func move(_ id: Int, to parent: Int, _ index: Int) {
        guard id != parent else { return }
        // From another Space, which a drag switched from.
        guard let from = place(of: id) else {
            run("move/\(id)/\(parent)/\(index)")
            return
        }
        let to = from.parent == parent && from.index < index ? index - 1 : index
        if from.parent == parent && to == from.index { return }
        let pinning = page.map { page in
            page.today.flatMap(\.tabs).contains { $0.id == id } && pins(parent, in: page)
        } ?? false
        run("move/\(id)/\(parent)/\(to)")
        if pinning { hintRename(id) }
    }

    /// Whether a container is the page's pinned tabs or a folder among them.
    private func pins(_ container: Int, in page: SpacePage) -> Bool {
        func holds(_ items: [Item]) -> Bool {
            items.contains { item in
                guard case .folder(let id, _, _, let children) = item else { return false }
                return id == container || holds(children)
            }
        }
        return container == page.roots.pinned || holds(page.pinned)
    }

    /// Like Arc's, a tab just pinned says, a moment after it lands and for 3 seconds, that a
    /// double-click renames it.
    private func hintRename(_ tab: Int) {
        renameHintWait?.cancel()
        let show = DispatchWorkItem { [weak self] in
            guard let self else { return }
            renameHint = tab
            let end = DispatchWorkItem { [weak self] in
                if self?.renameHint == tab { self?.renameHint = nil }
            }
            renameHintWait = end
            DispatchQueue.main.asyncAfter(deadline: .now() + 3, execute: end)
        }
        renameHintWait = show
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: show)
    }

    /// Puts a tab beside another in a split, after it or `before` it, or a copy of it with
    /// Option held, like Arc's.
    func split(_ id: Int, onto target: Int, before: Bool = false) {
        guard id != target else { return }
        let command = NSEvent.modifierFlags.contains(.option) ? "split-duplicate" : "split"
        run("\(command)/\(id)/\(target)\(before ? "/before" : "")")
    }

    /// Shows the library, on a section or the one it showed last.
    func openLibrary(_ section: LibrarySection?) {
        libraryShown = true
        run("library/show")
        pick(section ?? self.section)
    }

    func pick(_ section: LibrarySection) {
        self.section = section
        query = ""
        run(section.wide ? "library/wide" : "library/narrow")
        // Their page may have changed them.
        switch section {
        case .extensions: run("extension/reload")
        case .archive: run("archive/list")
        case .history: searchHistory()
        default: break
        }
    }

    /// Back to the displayed Space, the card resting beside the sidebar again.
    func closeLibrary() {
        libraryShown = false
        run("card/offset/0")
    }

    private func searchHistory() {
        run("history/search/200/\(percentEncoded(query))")
    }

    /// Tells the app the card's color, which an empty Space shows, a little tinted by the
    /// displayed Space's leading color (WindowColors in app/src/command.rs).
    /// The card's color, which an empty Space shows and a split's pages' bars take.
    var cardColor: String {
        let theme = space?.theme
        let panel = isDark(theme) ? "#1e1a28" : "#f8f6fb"
        return theme?.colors.first.map { mix($0, panel, 0.08) } ?? panel
    }

    private func paintWindow() {
        let theme = space?.theme
        let panel = isDark(theme) ? "#1e1a28" : "#f8f6fb"
        let lead = theme?.colors.first
        let card = cardColor
        let command = "window/colors/\(card.dropFirst())/0/\((lead ?? panel).dropFirst())"
        if command != colors {
            colors = command
            run(command)
        }
    }
}

/// What the interface puts in one of the app's windows.
/// How far below the window's top its buttons and the sidebar's toolbar are centered, like the
/// system's own windows, as System Settings.
let windowButtonsCenter: CGFloat = 25.5

/// Where the window's buttons' circles start from its left edge, how wide and how far apart they
/// are, like the system's own windows, measured on System Settings.
let windowButtonsLeading: CGFloat = 18.5
let windowButtonsCircle: CGFloat = 14
let windowButtonsSpacing: CGFloat = 23
/// Where the room past them starts: 12 points after the last circle, like Arc's.
let windowButtonsEnd: CGFloat = windowButtonsLeading + 2 * windowButtonsSpacing + windowButtonsCircle + 12
/// The three circles' room.
let windowLightsSize = NSSize(
    width: windowButtonsCircle + 2 * windowButtonsSpacing, height: windowButtonsCircle)

/// Places the window's buttons like Arc's: centered `windowButtonsCenter` below its top, their
/// titlebar grown to twice that, the first circle `windowButtonsLeading` from the left edge.
@MainActor
func centerWindowButtons(_ window: NSWindow) {
    guard let close = window.standardWindowButton(.closeButton), let bar = close.superview,
        let container = bar.superview
    else { return }
    let height = windowButtonsCenter * 2
    container.frame = NSRect(
        x: container.frame.minX, y: window.frame.height - height, width: container.frame.width,
        height: height)
    bar.frame = container.bounds
    for (index, kind) in [NSWindow.ButtonType.closeButton, .miniaturizeButton, .zoomButton]
        .enumerated()
    {
        guard let button = window.standardWindowButton(kind) else { continue }
        // Drawn at Arc's circles' size, from AppKit's own.
        if button.frame.width != windowButtonsCircle {
            let natural = button.frame.size
            button.setFrameSize(NSSize(width: windowButtonsCircle, height: windowButtonsCircle))
            button.setBoundsSize(natural)
        }
        let x = windowButtonsLeading + CGFloat(index) * windowButtonsSpacing
        button.setFrameOrigin(NSPoint(x: x, y: (height - windowButtonsCircle) / 2))
    }
}

@MainActor
final class NativeWindow {
    let model: AppModel
    let backdrop: Backdrop
    let sidebar: SidebarHost
    let commandBar: CommandBar
    let findBar: FindBar
    let siteControls: SiteControls
    let themePicker: ThemePickerModel
    let importModel: ImportModel
    let splitHandles: SplitHandles
    let pages: PagePlacer
    let pageDrop: PageDrop
    let peek: PeekChrome
    let capture: CaptureChrome
    /// The panels above the page, by the name of the web view they stand for, like "overlay".
    var surfaces: [String: Surface] = [:]
    private var observers: [NSObjectProtocol] = []
    /// The main window it stands for.
    private(set) weak var window: NSWindow?
    /// Closes the Command Bar on a click out of it (closeCommandBar).
    private var clicks: Any?

    init(window: NSWindow, content: NSView, app: AppModel) {
        model = app
        self.window = window
        window.isOpaque = false
        window.backgroundColor = .clear
        window.titlebarAppearsTransparent = true
        centerWindowButtons(window)
        // AppKit lays the buttons out again as the window changes.
        observers = [
            NSWindow.didResizeNotification, NSWindow.didEndLiveResizeNotification,
            NSWindow.didExitFullScreenNotification, NSWindow.didBecomeKeyNotification,
        ].map {
            NotificationCenter.default.addObserver(forName: $0, object: window, queue: .main) {
                [weak window] _ in
                MainActor.assumeIsolated { window.map(centerWindowButtons) }
            }
        }
        // As it starts going into full screen or out of it, the sidebar's toggle moving with it.
        for (name, full) in [
            (NSWindow.willEnterFullScreenNotification, true),
            (NSWindow.willExitFullScreenNotification, false),
        ] {
            observers.append(
                NotificationCenter.default.addObserver(forName: name, object: window, queue: .main) {
                    _ in MainActor.assumeIsolated { app.fullScreen = full }
                })
        }
        backdrop = Backdrop(content: content, model: app)
        sidebar = SidebarHost(window: window, model: app)
        let bar = CommandBar(app: app)
        commandBar = bar
        let overlay = Surface(in: window, radius: 14) { CommandBarView(bar: bar) }
        overlay.takesKeyboard = true
        overlay.shown = { [weak bar] in
            guard let field = bar?.field, field.currentEditor() == nil else { return }
            field.window?.makeFirstResponder(field)
        }
        surfaces["overlay"] = overlay
        clicks = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) {
            [weak window, weak overlay, weak bar] event in
            if let window, let overlay, let bar {
                closeCommandBar(bar, overlay, window, event)
            }
            return event
        }
        let find = FindBar(app: app)
        findBar = find
        let findSurface = Surface(in: window, radius: 10) { FindView(bar: find) }
        findSurface.takesKeyboard = true
        findSurface.shown = { [weak find] in
            guard let field = find?.field else { return }
            field.window?.makeFirstResponder(field)
            field.selectText(nil)
        }
        surfaces["find"] = findSurface
        let site = SiteControls(app: app)
        siteControls = site
        let siteSurface = Surface(in: window, radius: 12) { SiteView(controls: site) }
        siteSurface.takesKeyboard = true
        siteSurface.resigned = { app.run("site/hide") }
        surfaces["site"] = siteSurface
        let picker = ThemePickerModel(app: app)
        themePicker = picker
        let themeSurface = Surface(in: window, radius: 20, material: .hudWindow) {
            ThemePickerView(picker: picker)
        }
        themeSurface.takesKeyboard = true
        themeSurface.resigned = { app.run("theme/hide") }
        surfaces["theme"] = themeSurface
        let importer = ImportModel(app: app)
        importModel = importer
        let importSurface = Surface(in: window, radius: 16) { ImportView(model: importer) }
        importSurface.takesKeyboard = true
        surfaces["import"] = importSurface
        let pages = PagePlacer(in: window, model: app)
        sidebar.parent = { [weak pages] in pages?.overlay() }
        self.pages = pages
        let handles = SplitHandles(pages: pages, model: app)
        splitHandles = handles
        pages.didPlace = { [weak handles] in handles?.arrange() }
        pageDrop = PageDrop(in: window, model: app)
        peek = PeekChrome(in: window, model: app)
        peek.fade = { [weak pages] in pages?.fade($0) }
        capture = CaptureChrome(in: window, model: app)
    }

    /// Closes its panels, the window they belong to closing.
    func close() {
        observers.forEach(NotificationCenter.default.removeObserver)
        if let clicks { NSEvent.removeMonitor(clicks) }
        sidebar.close()
        surfaces.values.forEach { $0.close() }
        splitHandles.close()
        pages.close()
        pageDrop.close()
        peek.close()
        capture.close()
    }

    /// Gives one of its views an event, by the name of the web view it stands for.
    func handle(_ surface: String, _ name: String, _ detail: Data) {
        switch surface {
        case "sidebar": model.handle(name, detail)
        case "overlay": commandBar.handle(name, detail)
        case "find": findBar.handle(name, detail)
        case "site": siteControls.handle(name, detail)
        case "theme": themePicker.handle(name, detail)
        case "import": importModel.handle(name, detail)
        default: break
        }
    }
}

/// The interface of each of the app's windows, by the id the app gives it.
@MainActor var nativeWindows: [UInt32: NativeWindow] = [:]

/// Puts the interface in the window `handle` is, or is in, which the app knows as `id`: the blur
/// behind every view, and the sidebar above them.
@_cdecl("arbalete_ui_attach")
public func attach(_ id: UInt32, _ handle: UnsafeMutableRawPointer, _ send: Send) {
    MainActor.assumeIsolated {
        let object = Unmanaged<AnyObject>.fromOpaque(handle).takeUnretainedValue()
        guard let window = (object as? NSWindow) ?? (object as? NSView)?.window,
            let content = window.contentView
        else { return }
        let app = AppModel(window: id, send: send)
        nativeWindows[id] = NativeWindow(window: window, content: content, app: app)
        if settingsModel == nil { settingsModel = SettingsModel(app: app) }
    }
}

/// Takes the interface out of the window `id`, which closed: its panels close with it.
@_cdecl("arbalete_ui_detach")
public func detach(_ id: UInt32) {
    MainActor.assumeIsolated {
        nativeWindows.removeValue(forKey: id)?.close()
    }
}

/// An event dispatched to a web surface the interface stands in for, like "sidebar", in the
/// window `id`.
@_cdecl("arbalete_ui_event")
public func event(
    _ id: UInt32, _ surface: UnsafePointer<CChar>, _ name: UnsafePointer<CChar>,
    _ detail: UnsafePointer<CChar>
) {
    MainActor.assumeIsolated {
        let (surface, name) = (String(cString: surface), String(cString: name))
        let detail = Data(String(cString: detail).utf8)
        switch surface {
        case "settings":
            settingsModel?.handle(name, detail)
        case let surface where surface.hasPrefix("little/"):
            if let id = Int(surface.dropFirst("little/".count)), let bar = littles[id],
                name == "arbalete:little"
            {
                bar.page = try? JSONDecoder().decode(LittleDetail.self, from: detail)
            }
        default:
            nativeWindows[id]?.handle(surface, name, detail)
        }
    }
}

/// Shows the sidebar of the window `id` at a frame in its points from its top left corner, or
/// hides it.
@_cdecl("arbalete_ui_place_sidebar")
public func placeSidebar(
    _ id: UInt32, _ visible: Bool, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ floating: Bool, _ revealed: Bool
) {
    MainActor.assumeIsolated {
        nativeWindows[id]?.sidebar.place(
            visible ? NSRect(x: x, y: y, width: width, height: height) : nil,
            floating: floating, revealed: revealed)
        // Floating too: the drop over the page starts past it, or it would take the drags the
        // sidebar's rows wait for.
        nativeWindows[id]?.pageDrop.sidebar =
            visible ? NSRect(x: x, y: y, width: width, height: height) : nil
    }
}

/// The image behind a url of the app's UI, like a tab's cached icon, read once.
@MainActor func uiImage(_ url: String?) -> NSImage? {
    guard let url else { return nil }
    if let image = images[url] { return image }
    let path = url.withCString { arbalete_ui_file($0) }
    defer { arbalete_free_string(path) }
    let image = path.flatMap { NSImage(contentsOfFile: String(cString: $0)) }
    if image != nil { images[url] = image }
    return image
}

@MainActor private var images: [String: NSImage] = [:]
