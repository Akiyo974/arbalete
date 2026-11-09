// The Command Bar, shown above the page by the app (open_overlay in app/src/shell/overlay.rs):
// empty, it opens a tab; holding the selected tab's address (⌘L), it loads another in that tab.
// Below what is typed, the address it opens or a search, the app's actions it names, the search
// engine's suggestions, then the open tabs and the history pages matching it; the arrows pick one,
// Enter or a click opens it, Escape closes the bar. Typing forward completes the field with the
// best result's address, selected: typing on replaces it, deleting drops it, → or End keeps it.
// While ⌃⇥ is held, it shows the tabs selected last instead, side by side (TabSwitcher).

import AppKit
import ImageIO
import SwiftUI

/// The search row's height and a result's, and the space around the results, measured on Arc's:
/// the app fits the Command Bar to them (place_overlay in app/src/shell/overlay.rs).
private let searchRow: CGFloat = 64
private let resultRow: CGFloat = 50
private let resultsPadding: CGFloat = 8
private let resultsTop: CGFloat = 3
private let resultsBottom: CGFloat = 11.5
/// The banner telling where suggestions come from, under the results.
private let bannerRow: CGFloat = 56
/// How many results show at once, like Arc's: the others scroll.
private let visibleRows = 5
/// How many of the search engine's suggestions show.
private let suggestionCount = 4

/// The app's actions the Command Bar runs, by their names, and the commands they send.
let actions: [(title: String, command: String)] = [
    ("New Folder", command("new-folder")),
    ("New Easel", command("new-easel")),
    ("New Little Arbalete Window", command("new-little")),
    ("New Boost", command("new-boost")),
    ("New Profile", command("new-profile")),
    ("Close Tab", command("close")),
    ("Reopen Closed Tab", command("reopen", 0)),
    ("Pin or Unpin Tab", command("pin")),
    ("Copy URL", command("copy-link")),
    ("Clear Today", command("clear-today")),
    ("Next Space", command("next-space")),
    ("Previous Space", command("previous-space")),
    ("Toggle Sidebar", command("toggle-sidebar")),
    ("Toggle Toolbar", command("toggle-toolbar")),
    ("Open Library", command("show-library")),
    ("Show History", command("show-history")),
    ("Show Downloads", command("show-downloads")),
    ("Manage Extensions", command("manage-extensions")),
    ("Add Extensions", command("show-store")),
    ("Import Browser Data", command("show-import")),
    ("Show Welcome", command("show-onboarding")),
    ("Settings", command("show-settings")),
    ("View Page Source", command("view-source")),
    ("Developer Tools", command("dev-tools")),
]

/// The actions whose names start a word with each word typed, like "new fo" for New Folder: from
/// two letters on, as one matches too many; three at most.
func matchingActions(_ text: String) -> [(title: String, command: String)] {
    let words = text.lowercased().split(whereSeparator: \.isWhitespace)
    guard words.joined().count >= 2 else { return [] }
    return Array(
        actions.filter { action in
            let names = action.title.lowercased().split(separator: " ")
            return words.allSatisfy { word in names.contains { $0.hasPrefix(word) } }
        }.prefix(3))
}

struct SearchEngine: Decodable {
    var name: String
    var url: String
}

struct Results: Decodable {
    struct Page: Decodable {
        var url: String
        var title: String
        var favicon: String?
    }
    var text: String
    var tabs: [Tab]
    var pages: [Page]
    var suggestions: [String]?
}

struct Switching: Decodable {
    struct Card: Decodable, Identifiable {
        var id: Int
        var url: String
        var title: String
        var favicon: String?
        /// The last picture of its page, a JPEG in base64, sent as the switcher opens.
        var snapshot: String?

        var label: String { title.isEmpty ? url : title }
    }
    var tabs: [Card]
    var picked: Int
}

enum CommandRow: Identifiable {
    case go(url: String, title: String, search: Bool)
    case action(command: String, title: String)
    case tab(Tab)
    case page(url: String, title: String, favicon: String?)

    var id: String {
        switch self {
        case .go(let url, _, _): "go:\(url)"
        case .action(let command, _): "action:\(command)"
        case .tab(let tab): "tab:\(tab.id)"
        case .page(let url, _, _): "page:\(url)"
        }
    }

    /// A history page's row, titled with its address while it has no title.
    init(_ page: Results.Page) {
        self = .page(url: page.url, title: page.title.isEmpty ? page.url : page.title, favicon: page.favicon)
    }

    var title: String {
        switch self {
        case .go(_, let title, _), .action(_, let title), .page(_, let title, _): title
        case .tab(let tab): tab.label
        }
    }
}

/// What completes typed text: the address of the first result, history pages then open tabs,
/// that starts with it, without scheme nor "www.": up to the end of its host, or all of it once
/// a "/" is typed. It opens that site's home, or that page.
func complete(_ text: String, _ results: Results?) -> (suffix: String, url: String)? {
    let typed = text.lowercased()
    guard !typed.isEmpty, !typed.contains(where: \.isWhitespace), let results else { return nil }
    for url in results.pages.map(\.url) + results.tabs.map(\.url) {
        var address = url.replacingOccurrences(
            of: "^[a-z][a-z0-9+.-]*://", with: "", options: [.regularExpression, .caseInsensitive])
        address = address.replacingOccurrences(
            of: "^www\\.", with: "", options: [.regularExpression, .caseInsensitive])
        guard address.lowercased().hasPrefix(typed) else { continue }
        let page = typed.contains("/")
        var whole = page ? address : String(address.split(separator: "/").first ?? "")
        if page && whole.hasSuffix("/") { whole.removeLast() }
        if whole.count > typed.count {
            let suffix = String(whole.dropFirst(typed.count))
            guard page else {
                guard let components = URLComponents(string: url), let scheme = components.scheme,
                    let host = components.host
                else { continue }
                let port = components.port.map { ":\($0)" } ?? ""
                return (suffix, "\(scheme)://\(host)\(port)/")
            }
            return (suffix, url)
        }
    }
    return nil
}

/// The address typed text stands for, or nil when it is a search: a URL with a scheme stays as
/// it is, a host with a dot, localhost or an IP address opens over https.
func toURL(_ text: String) -> String? {
    func matches(_ pattern: String) -> Bool {
        text.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil
    }
    if matches("^(https?|file|ftp|chrome|arbalete)://") || matches("^(about|data|mailto|view-source):") {
        return text
    }
    if text.contains(where: \.isWhitespace) { return nil }
    if matches("^localhost(:[0-9]+)?(/|$)") { return "http://\(text)" }
    return matches("^[^/]+\\.[a-z0-9-]{2,}(:[0-9]+)?(/|$)")
        || matches("^[0-9]+(\\.[0-9]+){3}(:[0-9]+)?(/|$)")
        ? "https://\(text)" : nil
}

@MainActor
final class CommandBar: ObservableObject {
    @Published var text = ""
    @Published var editing = false
    @Published var favicon: String?
    @Published var search = SearchEngine(name: "Google", url: "https://www.google.com/search?q=")
    @Published var results: Results?
    @Published var picked = 0
    /// The row under the pointer, which Arc tints without picking it.
    @Published var hovered: Int?
    @Published var completing = false
    @Published var switching: Switching?
    /// The pictures of the switcher's tabs, by tab, with the JPEG each was made from.
    private(set) var snapshots: [Int: (jpeg: String, image: NSImage)] = [:]
    /// Told whether it is the tab switcher, which its panel rounds more, like Arc's.
    var switched: (Bool) -> Void = { _ in }
    /// Whether the banner telling where suggestions come from shows, like Arc's under its ⓘ.
    @Published var informing = false {
        didSet { if informing != oldValue { sendHeight() } }
    }
    /// The field, to focus it once the bar shows.
    weak var field: NSTextField?
    private let app: AppModel
    /// The Little Arbalete window it opens what is picked in, while it is a Little Arbalete
    /// window's, like Little Arc's: a new one, without an id, or that window's page. Without,
    /// it is its window's, opening a tab.
    let little: Int??
    /// Fits a Little Arbalete window's bar to its height; the app fits its window's.
    var fit: (CGFloat) -> Void = { _ in }
    /// Takes a Little Arbalete window's bar away.
    var dismiss: () -> Void = {}
    /// Whether a Little Arbalete window's bar shows.
    var littleShown = false

    /// The tab switcher's picked tile on a light panel and on a dark one, from its window's
    /// Space's colors.
    var tiles: (light: Color, dark: Color) {
        (app.switcherTile(dark: false), app.switcherTile(dark: true))
    }

    /// Its panel shows above its window's page.
    var isShown: Bool {
        little == nil
            ? nativeWindows[app.window]?.surfaces["overlay"]?.isShown == true : littleShown
    }
    private var height: CGFloat = 0

    init(app: AppModel, little: Int?? = nil) {
        self.app = app
        self.little = little
    }

    func handle(_ name: String, _ detail: Data) {
        let decoder = JSONDecoder()
        switch name {
        case "arbalete:show":
            struct Show: Decodable {
                var url: String?
                var favicon: String?
                var search: SearchEngine?
            }
            let show = try? decoder.decode(Show?.self, from: detail)
            text = show?.url ?? ""
            editing = show?.url != nil
            favicon = show?.favicon
            if let engine = show?.search { search = engine }
            results = nil
            picked = 0
            completing = false
            switching = nil
            informing = false
            // The app placed it again, one row tall: it is told its height anew.
            height = 0
            field?.stringValue = text
            // Selected all once it shows, ready to be typed over.
            field?.currentEditor()?.selectAll(nil)
        case "arbalete:results":
            // Only the answer to what is typed now counts.
            guard var detail = try? decoder.decode(Results.self, from: detail), detail.text == text
            else { return }
            if results?.text == text { detail.suggestions = results?.suggestions }
            results = detail
            showCompletion()
        case "arbalete:suggestions":
            struct Suggestions: Decodable {
                var text: String
                var suggestions: [String]
            }
            guard let detail = try? decoder.decode(Suggestions.self, from: detail),
                detail.text == text
            else { return }
            var updated = results ?? Results(text: text, tabs: [], pages: [])
            updated.suggestions = detail.suggestions
            results = updated
        case "arbalete:switch":
            let switching = try? decoder.decode(Switching.self, from: detail)
            let ids = Set(switching?.tabs.map(\.id) ?? [])
            snapshots = snapshots.filter { ids.contains($0.key) }
            for tab in switching?.tabs ?? [] {
                guard let jpeg = tab.snapshot, snapshots[tab.id]?.jpeg != jpeg,
                    let image = thumbnail(jpeg)
                else { continue }
                snapshots[tab.id] = (jpeg, image)
            }
            self.switching = switching
        default:
            return
        }
        switched(switching != nil)
        sendHeight()
    }

    var completion: (suffix: String, url: String)? {
        completing ? complete(text, results) : nil
    }

    /// What is typed, opened as an address, completed or not, or searched, then the app's
    /// actions it names, the search engine's suggestions, and the matching open tabs and history
    /// pages.
    var rows: [CommandRow] {
        let typed = text.trimmingCharacters(in: .whitespaces)
        // Nothing typed, like Arc's: the sites visited most.
        guard !typed.isEmpty else {
            return (results?.pages ?? []).map(CommandRow.init)
        }
        let go: CommandRow
        if let completion {
            go = .go(url: completion.url, title: typed + completion.suffix, search: false)
        } else if let url = toURL(typed) {
            go = .go(url: url, title: typed, search: false)
        } else {
            // Like Arc's, the text alone.
            go = .go(url: search.url + percentEncoded(typed), title: typed, search: true)
        }
        let suggestions = (results?.suggestions ?? [])
            .filter { $0.lowercased() != typed.lowercased() }
            .prefix(suggestionCount)
            .map { CommandRow.go(url: search.url + percentEncoded($0), title: $0, search: true) }
        // A Little Arbalete window's opens pages only, like Little Arc's.
        let actions = little == nil ? matchingActions(typed) : []
        let tabs = little == nil ? results?.tabs ?? [] : []
        return [go]
            + actions.map { .action(command: $0.command, title: $0.title) }
            + suggestions
            + tabs.map { .tab($0) }
            + (results?.pages ?? []).map(CommandRow.init)
    }

    /// Tells the app the bar's height, which it fits it to; the app sizes the tab switcher itself
    /// (place_switcher in app/src/shell/overlay.rs).
    func sendHeight() {
        guard switching == nil else { return }
        let shown = min(rows.count, visibleRows)
        let height =
            if shown > 0 {
                searchRow + CGFloat(shown) * resultRow + resultsTop + resultsBottom
                    + (informing ? bannerRow : 0)
            } else {
                searchRow + (informing ? bannerRow : 0)
            }
        if height != self.height {
            self.height = height
            if little == nil { app.run("overlay-height", Int(height)) } else { fit(height) }
        }
    }

    func type(_ typed: String, deleting: Bool) {
        completing = !deleting && typed.count > text.count
        text = typed
        picked = 0
        app.run(little == nil ? "search-overlay" : "search-little", typed)
        showCompletion()
        sendHeight()
    }

    /// Shows the completion after what is typed, selected.
    func showCompletion() {
        guard let field, let editor = field.currentEditor() else { return }
        let suffix = completion?.suffix ?? ""
        if field.stringValue != text + suffix {
            field.stringValue = text + suffix
            editor.selectedRange = NSRange(location: text.utf16.count, length: suffix.utf16.count)
        }
    }

    /// Keeps the completion, as → or End do.
    func acceptCompletion() -> Bool {
        guard let completion else { return false }
        text += completion.suffix
        completing = false
        return false
    }

    func move(_ step: Int) {
        let count = rows.count
        guard count > 0 else { return }
        picked = (picked + step + count) % count
    }

    func open(_ row: CommandRow) {
        switch row {
        case .tab(let tab):
            app.run("select", tab.id)
            app.run("hide-overlay")
        case .action(let command, _):
            app.run("hide-overlay")
            app.run(command: command)
        case .go(let url, _, _), .page(let url, _, _):
            switch little {
            case .some(.some(let id)):
                app.run("little", id, ["navigate": url])
                hide()
            case .some(.none):
                app.run("new-little", url)
                hide()
            case .none:
                app.run(editing ? "navigate" : "open", url)
            }
        }
    }

    func submit() {
        let rows = rows
        if picked < rows.count { open(rows[picked]) }
    }

    func hide() {
        if little == nil { app.run("hide-overlay") } else { dismiss() }
    }

    /// Opens the settings at the profiles', where each one's search engine is chosen.
    func showSearchSettings() {
        hide()
        settingsModel?.choose("Profiles")
        app.run("show-settings")
    }

    /// Shows a Little Arbalete window's bar holding `url`, or empty, like the app shows its
    /// window's: with the sites visited most until something is typed.
    func showLittle(url: String?, search engine: SearchEngine) {
        var show: [String: Any] = ["search": ["name": engine.name, "url": engine.url]]
        if let url { show["url"] = url }
        handle("arbalete:show", (try? JSONSerialization.data(withJSONObject: show)) ?? Data())
        app.run("search-little", text)
    }
}

struct CommandBarView: View {
    @ObservedObject var bar: CommandBar

    var body: some View {
        if let switching = bar.switching {
            TabSwitcher(
                switching: switching, snapshots: bar.snapshots.mapValues(\.image), tiles: bar.tiles)
                .background(lightVeil)
        } else {
            let rows = bar.rows
            VStack(spacing: 0) {
                HStack(spacing: 8) {
                    // Like Arc's, the picked row's icon once something is typed; a magnifier
                    // before.
                    Group {
                        if bar.editing, rows.isEmpty, let favicon = bar.favicon {
                            Favicon(url: favicon, size: 16)
                        } else if !bar.text.isEmpty, bar.picked < rows.count {
                            RowIcon(row: rows[bar.picked], picked: false, field: true)
                        } else {
                            Image(systemName: "magnifyingglass").font(.system(size: 12, weight: .medium))
                        }
                    }
                    .frame(width: 24, height: 24)
                    CommandField(bar: bar)
                    // Like Arc's, it tells where suggestions come from as the pointer reaches it.
                    Button { bar.informing = true } label: {
                        Image(systemName: "info").font(.system(size: 11, weight: .bold))
                            .foregroundStyle(Color.ink((0x000000, 0.5), (0xFFFFFF, 0.6)))
                            .frame(width: 22, height: 22)
                            .background(Circle().fill(Color.ink((0x000000, 0.06), (0xFFFFFF, 0.1))))
                            .contentShape(Circle())
                    }
                    .buttonStyle(.plain)
                    .onHover { if $0 { bar.informing = true } }
                }
                .padding(.horizontal, 20)
                .frame(height: searchRow)
                if !rows.isEmpty {
                    // A hairline under the field while results show, like Arc's.
                    Rectangle().fill(Color.ink((0x000000, 0.14), (0xFFFFFF, 0.08)))
                        .frame(height: 0.5)
                        .padding(.horizontal, 12)
                    ScrollViewReader { scroll in
                        ScrollView {
                            VStack(spacing: 0) {
                                ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                                    ResultRow(
                                        row: row, picked: index == bar.picked,
                                        hovered: index == bar.hovered, label: label(row)
                                    )
                                    .id(index)
                                    // Like Arc's, the pointer tints a row without picking it.
                                    .onHover { over in
                                        if over {
                                            bar.hovered = index
                                        } else if bar.hovered == index {
                                            bar.hovered = nil
                                        }
                                    }
                                    .onTapGesture { bar.open(row) }
                                }
                            }
                            .padding(.horizontal, resultsPadding)
                            .padding(.top, resultsTop - 0.5)
                            .padding(.bottom, resultsBottom)
                        }
                        .scrollIndicators(.never)
                        .onChange(of: bar.picked) { _, picked in scroll.scrollTo(picked) }
                    }
                }
                if bar.informing { SuggestionsBanner(bar: bar) }
            }
            .frame(maxHeight: .infinity, alignment: .top)
            .background(lightVeil)
        }
    }

    /// Measured on Arc's light theme: a near-white panel, #F8F8F8, that still lets the page
    /// through; the dark one keeps its material as it is.
    private var lightVeil: Color { Color.ink((0xFFFFFF, 0.85), (0x000000, 0)) }

    private func label(_ row: CommandRow) -> String? {
        switch row {
        case .tab: "Switch to Tab"
        case .action: "Action"
        default: nil
        }
    }
}

/// Where the search engine's suggestions come from, as the bar's last row, like Arc's: what it
/// says on the left, the search settings and Dismiss on the right.
struct SuggestionsBanner: View {
    @ObservedObject var bar: CommandBar

    var body: some View {
        VStack(spacing: 0) {
            Rectangle().fill(Color.ink((0x000000, 0.14), (0xFFFFFF, 0.08)))
                .frame(height: 0.5)
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Arbalete asks your search engine for suggestions as you type.")
                        .foregroundStyle(Color.ink((0x000000, 0.8), (0xFFFFFF, 0.8)))
                    Text("You can turn them off in each profile's search settings.")
                        .foregroundStyle(Color.ink((0x000000, 0.42), (0xFFFFFF, 0.4)))
                }
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
                Spacer(minLength: 8)
                BannerButton(title: "Search Settings…", prominent: true) { bar.showSearchSettings() }
                BannerButton(title: "Dismiss", prominent: false) { bar.informing = false }
            }
            .padding(.horizontal, 20)
            .frame(maxHeight: .infinity)
        }
        .frame(height: bannerRow)
        .background(Color.ink((0x000000, 0.03), (0x000000, 0.2)))
    }
}

/// The tab switcher's tabs, side by side, padded 16 like Arc's: the picked one stays in sight
/// while the others scroll (place_switcher in app/src/shell/overlay.rs sizes the panel to them).
struct TabSwitcher: View {
    let switching: Switching
    let snapshots: [Int: NSImage]
    let tiles: (light: Color, dark: Color)

    var body: some View {
        ScrollViewReader { scroll in
            ScrollView(.horizontal) {
                HStack(spacing: 0) {
                    ForEach(Array(switching.tabs.enumerated()), id: \.element.id) { index, tab in
                        SwitcherTab(
                            tab: tab, snapshot: snapshots[tab.id], picked: index == switching.picked, tiles: tiles)
                            .id(index)
                    }
                }
                .padding(16)
            }
            .scrollIndicators(.never)
            .onAppear { scroll.scrollTo(switching.picked, anchor: .center) }
            .onChange(of: switching.picked) { _, picked in
                withAnimation(.easeOut(duration: 0.1)) { scroll.scrollTo(picked, anchor: .center) }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// A tab of the switcher, like Arc's: its page's last picture above its icon and title, 15
/// points semibold, padded 12; the picked one on a tile rounded 8, in the Space's colors.
struct SwitcherTab: View {
    let tab: Switching.Card
    let snapshot: NSImage?
    let picked: Bool
    let tiles: (light: Color, dark: Color)
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(spacing: 12) {
            Group {
                if let snapshot {
                    Image(nsImage: snapshot).resizable().interpolation(.high).scaledToFill()
                } else {
                    // A page never pictured: its icon, larger.
                    Color.ink((0x000000, 0.06), (0xFFFFFF, 0.06))
                        .overlay { Favicon(url: tab.favicon, size: 32) }
                }
            }
            .frame(width: 166, height: 104, alignment: .top)
            .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .strokeBorder(Color.ink((0x000000, 0.1), (0xFFFFFF, 0.1)), lineWidth: 1)
            )
            // Arc's: black at 30%, 2 points around, half a point down.
            .shadow(color: .black.opacity(0.3), radius: 2, y: 0.5)
            HStack(spacing: 6) {
                if tab.favicon != nil { Favicon(url: tab.favicon, size: 16) }
                Text(tab.label)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Color.ink((0x000000, 0.85), (0xFFFFFF, 0.9)))
                    .lineLimit(1)
            }
            .frame(width: 166, height: 18)
        }
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill((scheme == .dark ? tiles.dark : tiles.light).opacity(picked ? 1 : 0)))
    }
}

/// A page's picture, a JPEG in base64, made small enough for the switcher.
func thumbnail(_ jpeg: String) -> NSImage? {
    let options = [
        kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 600,
    ] as CFDictionary
    guard let data = Data(base64Encoded: jpeg),
        let source = CGImageSourceCreateWithData(data as CFData, nil),
        let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options)
    else { return nil }
    return NSImage(cgImage: image, size: .zero)
}

/// One of the banner's buttons, small and rounded, the prominent one filled with the accent color.
private struct BannerButton: View {
    let title: String
    let prominent: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(prominent ? Color.white : Color.primary)
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(prominent ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.primary.opacity(0.1))))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// A result's icon, like Arc's: a search's magnifier, an address's globe or an action's bolt, 12
/// points; a page's site icon, on a white tile while it is picked; or, in the field, the picked
/// row's.
struct RowIcon: View {
    let row: CommandRow
    let picked: Bool
    var field = false

    var body: some View {
        let glyph: String? =
            switch row {
            case .action: "bolt"
            case .go(_, _, true): "magnifyingglass"
            case .go(_, _, false): "globe"
            default: nil
            }
        if let glyph {
            Image(systemName: glyph).font(.system(size: 12, weight: .medium))
                .foregroundStyle(picked ? Color.white : field ? Color.primary : Color.ink((0x000000, 0.65), (0xFFFFFF, 0.65)))
        } else {
            let favicon: String? =
                switch row {
                case .tab(let tab): tab.favicon
                case .page(_, _, let favicon): favicon
                default: nil
                }
            if let favicon {
                Favicon(url: favicon, size: 16)
                    .frame(width: 24, height: 24)
                    .background(
                        RoundedRectangle(cornerRadius: 5).fill(.white.opacity(picked && !field ? 0.99 : 0)))
            }
        }
    }
}

struct ResultRow: View {
    let row: CommandRow
    let picked: Bool
    let hovered: Bool
    let label: String?

    /// A page's site, as Arc shows it after its title: its address without scheme nor "www.".
    private var site: String? {
        guard case .page(let url, let title, _) = row, title != url else { return nil }
        let address = url.replacingOccurrences(
            of: "^[a-z][a-z0-9+.-]*://(www\\.)?", with: "", options: [.regularExpression, .caseInsensitive])
        return address.hasSuffix("/") ? String(address.dropLast()) : address
    }

    var body: some View {
        HStack(spacing: 8) {
            RowIcon(row: row, picked: picked)
                .frame(width: 24, height: 24)
            // Like Arc's, a page's site after its title, periwinkle while it is picked.
            (Text(row.title).foregroundStyle(picked ? Color.white : Color.ink((0x000000, 0.8), (0xFFFFFF, 0.8)))
                + Text(site.map { " — \($0)" } ?? "")
                .foregroundStyle(picked ? Color.ink((0xFFFFFF, 1), (0x979BFF, 1)) : Color.ink((0x000000, 0.42), (0xFFFFFF, 0.31))))
                .lineLimit(1)
                .font(.system(size: 13, weight: .medium))
            Spacer(minLength: 8)
            if let label {
                Text(label).font(.system(size: 13, weight: .medium))
                    .foregroundStyle(picked ? Color.white : Color.ink((0x000000, 0.27), (0xFFFFFF, 0.27)))
                // An arrow on a tile, like Arc's "Switch to Tab".
                Image(systemName: "arrow.right").font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(picked ? color("#6066fa") : Color(white: 0.67))
                    .frame(width: 24, height: 24)
                    .background(
                        RoundedRectangle(cornerRadius: 5).fill(
                            picked ? Color.white.opacity(0.99) : Color.ink((0x000000, 0.08), (0xFFFFFF, 0.04))))
                    .padding(.leading, 1)
            }
        }
        .padding(.leading, 12)
        .padding(.trailing, 15)
        .frame(height: resultRow)
        // Inset 2 points within the row, rounded 6, in Arc's indigo while picked; tinted under
        // the pointer, measured.
        .background(
            RoundedRectangle(cornerRadius: 6)
                // Measured on Arc's light theme too: periwinkle while picked, 6% black hovered.
                .fill(picked ? Color.ink((0x858AFF, 1), (0x373DD0, 1)) : Color.ink((0x000000, 0.06), (0xFFFFFF, 0.035)).opacity(hovered ? 1 : 0))
                .padding(2))
        .contentShape(Rectangle())
    }
}

/// The Command Bar's field: AppKit's, for its selection and its keys.
/// Whether the click being handled is on `window`'s edge, which resizes it.
@MainActor func onEdge(_ window: NSWindow) -> Bool {
    guard NSApp.currentEvent?.type == .leftMouseDown else { return false }
    let point = NSEvent.mouseLocation
    return window.frame.insetBy(dx: -6, dy: -6).contains(point)
        && !window.frame.insetBy(dx: 6, dy: 6).contains(point)
}

/// Like Arc's, a click out of the Command Bar closes it, but on the window's edge, which resizes
/// the window and keeps the bar: in the sidebar too, which does not take the keyboard from it.
@MainActor func closeCommandBar(
    _ bar: CommandBar, _ overlay: Surface, _ window: NSWindow, _ event: NSEvent
) {
    guard overlay.isShown, !overlay.contains(event), !onEdge(window) else { return }
    bar.hide()
}

struct CommandField: NSViewRepresentable {
    @ObservedObject var bar: CommandBar

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = .systemFont(ofSize: 18, weight: .medium)
        field.placeholderString = "Search or Enter URL…"
        field.delegate = context.coordinator
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        bar.field = field
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(bar: bar) }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        let bar: CommandBar
        private var deleting = false

        init(bar: CommandBar) {
            self.bar = bar
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            bar.type(field.stringValue, deleting: deleting)
            deleting = false
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            // Like Arc's, resizing the window keeps the bar, which takes the keyboard back once
            // the resize ends; a click anywhere else closes it.
            if let field = notification.object as? NSTextField, let main = field.window,
                onEdge(main)
            {
                var observer: NSObjectProtocol?
                observer = NotificationCenter.default.addObserver(
                    forName: NSWindow.didEndLiveResizeNotification, object: main, queue: .main
                ) { [weak field] _ in
                    if let observer { NotificationCenter.default.removeObserver(observer) }
                    MainActor.assumeIsolated {
                        field?.window?.makeKey()
                        field?.window?.makeFirstResponder(field)
                    }
                }
                return
            }
            if bar.isShown { bar.hide() }
        }

        func control(
            _ control: NSControl, textView: NSTextView, doCommandBy selector: Selector
        ) -> Bool {
            switch selector {
            case #selector(NSResponder.moveDown(_:)):
                bar.move(1)
            case #selector(NSResponder.moveUp(_:)):
                bar.move(-1)
            case #selector(NSResponder.insertNewline(_:)):
                bar.submit()
            case #selector(NSResponder.cancelOperation(_:)):
                bar.hide()
            case #selector(NSResponder.moveRight(_:)), #selector(NSResponder.moveToEndOfLine(_:)),
                #selector(NSResponder.moveToEndOfDocument(_:)):
                return bar.acceptCompletion()
            case #selector(NSResponder.deleteBackward(_:)), #selector(NSResponder.deleteForward(_:)):
                deleting = true
                return false
            default:
                return false
            }
            return true
        }
    }
}
