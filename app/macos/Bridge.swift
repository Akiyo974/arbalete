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
    @Published var downloads: [Download] = []
    @Published var archive: [ArchivedTab] = []
    @Published var history: [Visit] = []
    /// The sidebar shows the library, on a section, under what its field searches.
    @Published var libraryShown = false
    /// The list of profiles shows above the footer.
    @Published var profilesShown = false
    /// How far a two-finger swipe has moved the displayed Space's page, in points.
    @Published var swipe: CGFloat = 0
    @Published var section = LibrarySection.spaces
    @Published var query = "" {
        didSet { if section == .history { searchHistory() } }
    }
    /// The window the model is the interface of, which its commands come from.
    let window: UInt32
    let send: Send

    init(window: UInt32, send: @escaping Send) {
        self.window = window
        self.send = send
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
        let all = page.favorites + (page.pinned + page.today).flatMap(\.tabs)
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
        case "arbalete:rename":
            renaming = try? JSONDecoder().decode(Int.self, from: detail)
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
        switch phase {
        case "begin":
            swipe = 0
        case "move":
            swipe += dx
        default:
            let threshold: CGFloat = 80
            if let state, let index = state.spaces.firstIndex(where: { $0.id == state.space }) {
                if swipe > threshold {
                    if index == 0 { openLibrary(nil) } else { run("space/previous") }
                } else if swipe < -threshold, index < state.spaces.count - 1 {
                    run("space/next")
                }
            }
            withAnimation(.spring(duration: 0.3)) { swipe = 0 }
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
        guard id != parent, let from = place(of: id) else { return }
        let to = from.parent == parent && from.index < index ? index - 1 : index
        if from.parent == parent && to == from.index { return }
        run("move/\(id)/\(parent)/\(to)")
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
    private func paintWindow() {
        let theme = space?.theme
        let dark = isDark(theme)
        let panel = dark ? "#1e1a28" : "#f8f6fb"
        let lead = theme?.colors.first
        let card = lead.map { mix($0, panel, 0.08) } ?? panel
        let command = "window/colors/\(card.dropFirst())/0/\((lead ?? panel).dropFirst())"
        if command != colors {
            colors = command
            run(command)
        }
    }
}

/// What the interface puts in one of the app's windows.
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
    /// The panels above the page, by the name of the web view they stand for, like "overlay".
    var surfaces: [String: Surface] = [:]

    init(window: NSWindow, content: NSView, app: AppModel) {
        model = app
        window.isOpaque = false
        window.backgroundColor = .clear
        window.titlebarAppearsTransparent = true
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
        surfaces["toolbar"] = Surface(in: window, radius: 0, material: .titlebar) {
            PageToolbar(model: app)
        }
        let site = SiteControls(app: app)
        siteControls = site
        let siteSurface = Surface(in: window, radius: 12) { SiteView(controls: site) }
        siteSurface.takesKeyboard = true
        siteSurface.resigned = { app.run("site/hide") }
        surfaces["site"] = siteSurface
        let picker = ThemePickerModel(app: app)
        themePicker = picker
        let themeSurface = Surface(in: window, radius: 20) { ThemePickerView(picker: picker) }
        themeSurface.takesKeyboard = true
        themeSurface.resigned = { app.run("theme/hide") }
        surfaces["theme"] = themeSurface
        let importer = ImportModel(app: app)
        importModel = importer
        let importSurface = Surface(in: window, radius: 16) { ImportView(model: importer) }
        importSurface.takesKeyboard = true
        surfaces["import"] = importSurface
        splitHandles = SplitHandles(in: window, model: app)
    }

    /// Closes its panels, the window they belong to closing.
    func close() {
        sidebar.close()
        surfaces.values.forEach { $0.close() }
        splitHandles.close()
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
