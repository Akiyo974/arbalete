// The Command Bar (ui/src/Overlay.tsx), shown above the page by the app (open_overlay in
// app/src/shell.rs): empty, it opens a tab; holding the selected tab's address (⌘L), it loads
// another in that tab. Below what is typed, the address it opens or a search, the app's actions
// it names, the search engine's suggestions, then the open tabs and the history pages matching
// it; the arrows pick one, Enter or a click opens it, Escape closes the bar. Typing forward
// completes the field with the best result's address, selected: typing on replaces it, deleting
// drops it, → or End keeps it. While ⌃⇥ is held, it lists the tabs selected last instead.

import AppKit
import SwiftUI

/// The search row's height and a result's, and the space around the results, like Arc's: the
/// app fits the Command Bar to them (place_overlay in app/src/shell.rs).
private let searchRow: CGFloat = 50
private let resultRow: CGFloat = 50
private let resultsPadding: CGFloat = 8
private let resultsInset: CGFloat = 10
/// How many results show at once, like Arc's: the others scroll.
private let visibleRows = 5
/// How many of the search engine's suggestions show.
private let suggestionCount = 4

/// The app's actions the Command Bar runs, by their names, and the commands they send.
let actions: [(title: String, command: String)] = [
    ("New Folder", "folder/new"),
    ("New Little Arbalete Window", "little/new"),
    ("New Profile", "profile/new"),
    ("Close Tab", "close"),
    ("Reopen Closed Tab", "reopen"),
    ("Pin or Unpin Tab", "pin"),
    ("Copy URL", "link/copy"),
    ("Clear Today", "today/clear"),
    ("Next Space", "space/next"),
    ("Previous Space", "space/previous"),
    ("Toggle Sidebar", "sidebar/toggle"),
    ("Toggle Toolbar", "toolbar/toggle"),
    ("Open Library", "library/show"),
    ("Show History", "history/show"),
    ("Show Downloads", "downloads/show"),
    ("Manage Extensions", "extension/manage"),
    ("Add Extensions", "site/store"),
    ("Import Browser Data", "import/show"),
    ("Settings", "settings/show"),
    ("View Page Source", "page/source"),
    ("Developer Tools", "page/devtools"),
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
    var tabs: [Tab]
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
    if matches("^(https?|file|ftp|chrome)://") || matches("^(about|data|mailto|view-source):") {
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
    @Published var completing = false
    @Published var switching: Switching?
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
            switching = try? decoder.decode(Switching.self, from: detail)
        default:
            return
        }
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
            go = .go(
                url: search.url + percentEncoded(typed),
                title: "\(typed) — Search \(search.name)", search: true)
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

    /// Tells the app the bar's height, which it fits it to.
    func sendHeight() {
        let shown = min(rows.count, visibleRows)
        let height =
            if let switching {
                CGFloat(switching.tabs.count) * resultRow + 2 * resultsInset
            } else if shown > 0 {
                searchRow + CGFloat(shown) * resultRow + 2 * resultsInset
            } else {
                searchRow
            }
        if height != self.height {
            self.height = height
            if little == nil { app.run("overlay/height/\(Int(height))") } else { fit(height) }
        }
    }

    func type(_ typed: String, deleting: Bool) {
        completing = !deleting && typed.count > text.count
        text = typed
        picked = 0
        app.run("\(little == nil ? "overlay" : "little")/search/\(percentEncoded(typed))")
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
            app.run("select/\(tab.id)")
            app.run("overlay/hide")
        case .action(let command, _):
            app.run("overlay/hide")
            app.run(command)
        case .go(let url, _, _), .page(let url, _, _):
            switch little {
            case .some(.some(let id)):
                app.run("little/\(id)/navigate/\(percentEncoded(url))")
                hide()
            case .some(.none):
                app.run("little/new/\(percentEncoded(url))")
                hide()
            case .none:
                app.run("\(editing ? "navigate" : "open")/\(percentEncoded(url))")
            }
        }
    }

    func submit() {
        let rows = rows
        if picked < rows.count { open(rows[picked]) }
    }

    func hide() {
        if little == nil { app.run("overlay/hide") } else { dismiss() }
    }

    /// Shows a Little Arbalete window's bar holding `url`, or empty, like the app shows its
    /// window's: with the sites visited most until something is typed.
    func showLittle(url: String?, search engine: SearchEngine) {
        var show: [String: Any] = ["search": ["name": engine.name, "url": engine.url]]
        if let url { show["url"] = url }
        handle("arbalete:show", (try? JSONSerialization.data(withJSONObject: show)) ?? Data())
        app.run("little/search/\(percentEncoded(text))")
    }
}

struct CommandBarView: View {
    @ObservedObject var bar: CommandBar

    var body: some View {
        if let switching = bar.switching {
            VStack(spacing: 0) {
                ForEach(Array(switching.tabs.enumerated()), id: \.element.id) { index, tab in
                    ResultRow(row: .tab(tab), picked: index == switching.picked, label: nil)
                }
            }
            .padding(.horizontal, resultsPadding)
            .padding(.vertical, resultsInset)
            .frame(maxHeight: .infinity, alignment: .top)
        } else {
            VStack(spacing: 0) {
                HStack(spacing: 12) {
                    if bar.editing, let favicon = bar.favicon {
                        Favicon(url: favicon, size: 18)
                    } else {
                        Image(systemName: "magnifyingglass").font(.system(size: 17))
                            .foregroundStyle(.secondary)
                    }
                    CommandField(bar: bar)
                }
                .padding(.horizontal, 20)
                .frame(height: searchRow)
                let rows = bar.rows
                if !rows.isEmpty {
                    // A hairline under the field while results show, like Arc's.
                    Rectangle().fill(Color.ink((0x000000, 0.10), (0xFFFFFF, 0.10)))
                        .frame(height: 1)
                        .padding(.horizontal, 12)
                    ScrollViewReader { scroll in
                        ScrollView {
                            VStack(spacing: 0) {
                                ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                                    ResultRow(
                                        row: row, picked: index == bar.picked,
                                        label: label(row)
                                    )
                                    .id(index)
                                    .onHover { if $0 { bar.picked = index } }
                                    .onTapGesture { bar.open(row) }
                                }
                            }
                            .padding(.horizontal, resultsPadding)
                            .padding(.vertical, resultsInset)
                        }
                        .scrollIndicators(.never)
                        .onChange(of: bar.picked) { _, picked in scroll.scrollTo(picked) }
                    }
                }
            }
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    private func label(_ row: CommandRow) -> String? {
        switch row {
        case .tab: "Switch to Tab →"
        case .action: "Action"
        default: nil
        }
    }
}

struct ResultRow: View {
    let row: CommandRow
    let picked: Bool
    let label: String?

    /// A page's site, as Arc shows it after its title.
    private var site: String? {
        guard case .page(let url, let title, _) = row, title != url else { return nil }
        return domain(url)
    }

    var body: some View {
        HStack(spacing: 8) {
            Group {
                switch row {
                case .action: Image(systemName: "bolt")
                case .tab(let tab) where tab.favicon != nil: Favicon(url: tab.favicon, size: 16)
                case .page(_, _, let favicon?): Favicon(url: favicon, size: 16)
                case .go(_, _, true): Image(systemName: "magnifyingglass")
                default: Image(systemName: "globe")
                }
            }
            .frame(width: 24, height: 24)
            .clipShape(RoundedRectangle(cornerRadius: 4))
            // Like Arc's, a page's site after its title.
            (Text(row.title).foregroundStyle(picked ? Color.white : Color.ink((0x000000, 0.80), (0xFFFFFF, 0.80)))
                + Text(site.map { " — \($0)" } ?? "")
                .foregroundStyle(picked ? Color.white.opacity(0.6) : Color.ink((0x000000, 0.4), (0xFFFFFF, 0.4))))
                .lineLimit(1)
                .font(.system(size: 13, weight: .medium))
            Spacer(minLength: 8)
            if let label {
                Text(label).font(.system(size: 13, weight: .medium))
                    .foregroundStyle(picked ? Color.white : Color.ink((0x000000, 0.33), (0xFFFFFF, 0.33)))
            }
        }
        .padding(.leading, 12)
        .padding(.trailing, label == nil ? 12 : 20)
        .frame(height: resultRow)
        // Inset 2 points within the row, rounded 6, in Arc's indigo, measured.
        .background(
            RoundedRectangle(cornerRadius: 6).fill(color("#4549d3").opacity(picked ? 1 : 0))
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
        field.font = .systemFont(ofSize: 18)
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
