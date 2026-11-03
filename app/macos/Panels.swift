// The small panels above the page: the find bar (ui/src/Find.tsx), the bar at the page's top in
// toolbar mode (ui/src/Toolbar.tsx), and the site controls (ui/src/Site.tsx).

import AppKit
import SwiftUI

/// Finds text in the page. Each keystroke searches; Enter goes to the next match, Shift-Enter to
/// the previous one. A new search has no active match until Enter.
@MainActor
final class FindBar: ObservableObject {
    @Published var text = ""
    @Published var found: (count: Int, active: Int)?
    weak var field: NSTextField?
    private let app: AppModel

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        switch name {
        case "arbalete:show":
            text = (try? JSONDecoder().decode(String.self, from: detail)) ?? ""
            field?.stringValue = text
            found = nil
        case "arbalete:found":
            struct Found: Decodable {
                var count: Int
                var active: Int
            }
            if let detail = try? JSONDecoder().decode(Found.self, from: detail) {
                found = (detail.count, detail.active)
            }
        default:
            break
        }
    }

    func find(forward: Bool) {
        if text.isEmpty { found = nil }
        app.run("find/\(forward ? "forward" : "backward")/\(percentEncoded(text))")
    }

    func hide() {
        app.run("find/hide")
    }
}

struct FindView: View {
    @ObservedObject var bar: FindBar

    var body: some View {
        // Like Arc's: a 15-point magnifying glass in a 26-point box, 3 points in, the field 32
        // points from the edge, and the buttons 3 points from the other.
        HStack(spacing: 0) {
            Image(systemName: "magnifyingglass").font(.system(size: 15))
                .foregroundStyle(.secondary)
                .frame(width: 26, height: 26)
                .padding(.trailing, 3)
            KeyField(
                placeholder: "Find in page", size: 13,
                attach: { bar.field = $0 },
                changed: {
                    bar.text = $0
                    bar.find(forward: true)
                },
                key: { key in
                    switch key {
                    case .enter(let shift): bar.find(forward: !shift)
                    case .escape: bar.hide()
                    default: return false
                    }
                    return true
                })
            if let found = bar.found {
                Text(found.count > 0 ? "\(found.active > 0 ? "\(found.active)" : "–")/\(found.count)" : "No results")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .fixedSize()
                    .padding(.horizontal, 6)
            }
            IconButton(symbol: "chevron.up", label: "Previous match", size: 24) {
                bar.find(forward: false)
            }
            IconButton(symbol: "chevron.down", label: "Next match", size: 24) {
                bar.find(forward: true)
            }
            IconButton(symbol: "xmark", label: "Close", size: 24) { bar.hide() }
        }
        .padding(.horizontal, 3)
        .frame(maxHeight: .infinity)
    }
}

/// A key a field passes on: Enter, with Shift or not, Escape, or the arrows.
enum FieldKey {
    case enter(shift: Bool)
    case escape
    case up
    case down
}

/// An AppKit field, for the keys SwiftUI's leaves to it.
struct KeyField: NSViewRepresentable {
    let placeholder: String
    let size: CGFloat
    let attach: (NSTextField) -> Void
    let changed: (String) -> Void
    /// Whether the field handled the key.
    let key: (FieldKey) -> Bool

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = .systemFont(ofSize: size)
        field.placeholderString = placeholder
        field.delegate = context.coordinator
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        attach(field)
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.field = self
    }

    func makeCoordinator() -> Coordinator { Coordinator(field: self) }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var field: KeyField

        init(field: KeyField) {
            self.field = field
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let control = notification.object as? NSTextField else { return }
            field.changed(control.stringValue)
        }

        func control(
            _ control: NSControl, textView: NSTextView, doCommandBy selector: Selector
        ) -> Bool {
            let shift = NSApp.currentEvent?.modifierFlags.contains(.shift) ?? false
            switch selector {
            case #selector(NSResponder.insertNewline(_:)): return field.key(.enter(shift: shift))
            case #selector(NSResponder.cancelOperation(_:)): return field.key(.escape)
            case #selector(NSResponder.moveUp(_:)): return field.key(.up)
            case #selector(NSResponder.moveDown(_:)): return field.key(.down)
            default: return false
            }
        }
    }
}

/// The bar at the page's top in toolbar mode: back, forward and reload at its left, the selected
/// tab's domain in its middle, which a click edits in the Command Bar, its site controls at its
/// right, and the page loading along its bottom.
struct PageToolbar: View {
    @ObservedObject var model: AppModel

    private func tool(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
        IconButton(symbol: symbol, label: label, size: 37, symbolSize: 14, action: action)
            .frame(height: 28)
    }

    var body: some View {
        if let tab = model.activeTab, tab.developer == true {
            DeveloperBar(model: model, url: tab.url)
        } else {
            plain
        }
    }

    /// Like Arc's, measured: buttons 37 points apart from each end, the sidebar's toggle first
    /// while it is not docked, the site controls and Split View last.
    private var plain: some View {
        let tab = model.activeTab
        let page = model.state?.page
        return ZStack {
            HStack(spacing: 0) {
                if !model.sidebarDocked {
                    tool("sidebar.left", "Toggle sidebar") { model.run("sidebar/toggle") }
                }
                tool("arrow.left", "Back") { model.run("back") }
                    .disabled(!(page?.canGoBack ?? false))
                tool("arrow.right", "Forward") { model.run("forward") }
                    .disabled(!(page?.canGoForward ?? false))
                // Like Arc's, it stops the page while it loads.
                if page?.loading == true {
                    tool("xmark", "Stop") { model.run("stop") }
                } else {
                    tool("arrow.clockwise", "Reload") { model.run("reload") }
                }
                Spacer()
                if tab != nil {
                    ForEach(model.state?.extensions ?? []) { ext in
                        PinnedExtension(model: model, extension: ext, view: "toolbar")
                    }
                    SiteControlsButton(
                        model: model, view: "toolbar", symbol: "slider.horizontal.2.square",
                        symbolSize: 15
                    )
                    .frame(width: 37)
                    tool("rectangle.split.2x1", "Add Split View") { model.run("pane/add") }
                }
            }
            if let tab {
                Button {
                    model.run("overlay/edit")
                } label: {
                    Label(domain(tab.url), systemImage: "link")
                        .font(.system(size: 13, weight: .semibold))
                        .lineLimit(1)
                        .padding(.horizontal, 10)
                        .frame(height: 26)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(tab.url)
            }
        }
        .padding(.horizontal, 2.25)
        .frame(maxHeight: .infinity)
        // A hairline under the bar, over the page, like Arc's, which shows no loading bar.
        .overlay(alignment: .bottom) {
            Rectangle().fill(Color.ink((0x000000, 0.05), (0xFFFFFF, 0.05))).frame(height: 1)
        }
    }
}

/// The bar above a page in developer mode, like Arc's, measured: in the dark, indigo with faint
/// diagonal stripes; in the light, white with its buttons in the accent. The lock, or an "i"
/// without one, and the full address in a monospaced font, then buttons to copy it, open the
/// developer tools, the site controls and Split View; in toolbar mode, the navigation buttons
/// first. On a split's page, like Arc's, the page's own, with × to close it.
struct DeveloperBar: View {
    @ObservedObject var model: AppModel
    let url: String
    /// On a split's page: it is focused before any action, and closed by ×.
    var focus: () -> Void = {}
    var close: (() -> Void)?

    private var dark: Bool { isDark(model.space?.theme) }

    var body: some View {
        HStack(spacing: 2) {
            if model.state?.toolbar == true {
                NavButtons(model: model)
            }
            Image(systemName: url.hasPrefix("https://") ? "lock.fill" : "info")
                .font(.system(size: 13, weight: .bold))
                .frame(width: 24)
            Text(url)
                .font(.system(size: 13, design: .monospaced))
                .foregroundStyle(dark ? Color.white : Ink.primary)
                .lineLimit(1)
                .truncationMode(.middle)
                .padding(.leading, 6)
            Spacer(minLength: 8)
            tool("link", "Copy URL") { model.run("menu/choose/site/any/copy-link") }
            separator
            tool("rectangle.dashed", "Capture Part of the Page") { model.run("page/capture/area") }
            tool("camera", "Capture the Page") { model.run("page/capture") }
            separator
            tool("apple.terminal", "Console") { model.run("page/devtools/console") }
            tool("network", "Network") { model.run("page/devtools/network") }
            tool("scope", "Inspect Element") { model.run("page/devtools/elements") }
            separator
            ForEach(model.state?.extensions ?? []) { ext in
                PinnedExtension(model: model, extension: ext, view: "toolbar")
            }
            SiteControlsButton(model: model, view: "toolbar")
            tool("rectangle.split.2x1", "Add Split View") { model.run("pane/add") }
            if let close {
                IconButton(symbol: "xmark", label: "Close", size: 28, symbolSize: 15, action: close)
            }
        }
        .foregroundStyle(dark ? Color.white : model.accent)
        .padding(.horizontal, 10)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background {
            if dark { stripes } else { color("#f8f8fd") }
        }
        .overlay(alignment: .bottom) {
            if !dark { Rectangle().fill(.black.opacity(0.08)).frame(height: 1) }
        }
        .environment(\.colorScheme, dark ? .dark : .light)
    }

    private var stripes: some View {
            Canvas { context, size in
                context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(color("#363ccc")))
                // Stripes 12 points wide, every 24, leaning right.
                var x: CGFloat = -size.height
                while x < size.width {
                    var stripe = Path()
                    stripe.move(to: CGPoint(x: x, y: size.height))
                    stripe.addLine(to: CGPoint(x: x + size.height, y: 0))
                    stripe.addLine(to: CGPoint(x: x + size.height + 12, y: 0))
                    stripe.addLine(to: CGPoint(x: x + 12, y: size.height))
                    context.fill(stripe, with: .color(color("#3338c9")))
                    x += 24
                }
            }
    }

    private var separator: some View {
        Rectangle().fill(dark ? Color.white.opacity(0.2) : Color.black.opacity(0.1))
            .frame(width: 1, height: 18).padding(.horizontal, 6)
    }

    private func tool(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
        IconButton(symbol: symbol, label: label, size: 28, symbolSize: 15) {
            focus()
            action()
        }
    }
}

/// The selected tab's site controls, under their button (show_site_controls in
/// app/src/shell.rs): actions on its link and what the site keeps, the profile's extensions,
/// whose popup a click opens in place of the panel, the Chrome Web Store to add more, the library
/// to manage them, and whether the connection is secure. It closes on losing the focus and on
/// Escape, and sizes itself to what it shows.
@MainActor
final class SiteControls: ObservableObject {
    struct Site: Decodable {
        struct SiteExtension: Decodable, Identifiable {
            var id: String
            var name: String
            var icon: String?
            var popup: Bool
            var enabled: Bool
            var pinned: Bool
        }
        var url: String
        /// The url's host, and whether its tabs are kept awake while tabs are suspended.
        var site: String
        var awake: Bool
        /// Its site is in developer mode.
        var developer: Bool
        var suspends: Bool
        var extensions: [SiteExtension]
    }

    @Published var site: Site?
    let app: AppModel

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        if name == "arbalete:show" {
            site = try? JSONDecoder().decode(Site.self, from: detail)
        }
    }
}

/// The site controls' actions, the items of the site menu (site_menu in app/src/menu.rs).
private let siteActions = [
    ("share", "Share", "square.and.arrow.up"),
    ("copy-link", "Copy Link", "link"),
    ("clear-cache", "Clear Cache", "trash"),
    ("clear-cookies", "Clear Cookies", "circle.dotted"),
]

struct SiteView: View {
    @ObservedObject var controls: SiteControls

    var body: some View {
        if let site = controls.site {
            let app = controls.app
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    ForEach(siteActions, id: \.0) { item, label, symbol in
                        SiteTile(label: label) {
                            app.run("site/hide")
                            app.run("menu/choose/site/any/\(item)")
                        } content: {
                            Image(systemName: symbol).font(.system(size: 15))
                        }
                    }
                }
                HStack {
                    Text("Extensions").font(.system(size: 12, weight: .semibold))
                    Spacer()
                    Button("Manage") { app.run("extension/manage") }
                        .buttonStyle(.plain)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                LazyVGrid(columns: Array(repeating: GridItem(.fixed(48), spacing: 8), count: 4),
                    alignment: .leading, spacing: 8
                ) {
                    ForEach(site.extensions) { ext in
                        SiteTile(label: ext.enabled ? ext.name : "\(ext.name) (off)") {
                            app.run("site/extension/\(ext.id)")
                        } content: {
                            Favicon(url: ext.icon, size: 18)
                        }
                        .disabled(!ext.popup || !ext.enabled)
                        .overlay(alignment: .topTrailing) {
                            Button {
                                app.run("extension/\(ext.pinned ? "unpin" : "pin")/\(ext.id)")
                            } label: {
                                Image(systemName: ext.pinned ? "pin.fill" : "pin")
                                    .font(.system(size: 9))
                                    .padding(3)
                            }
                            .buttonStyle(.plain)
                            .help(ext.pinned ? "Unpin" : "Pin")
                        }
                    }
                    SiteTile(label: "Add Extensions") {
                        app.run("site/store")
                    } content: {
                        Image(systemName: "plus").font(.system(size: 15))
                    }
                }
                Text("Settings").font(.system(size: 12, weight: .semibold))
                // Like Arc's: a row turning the site's developer mode on or off.
                Button {
                    let turn = site.developer ? "off" : "on"
                    app.run("developer/\(turn)/\(percentEncoded(site.site))")
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "hammer.fill")
                            .font(.system(size: 12))
                            .frame(width: 26, height: 26)
                            .background(Circle().fill(.primary.opacity(site.developer ? 0.2 : 0.08)))
                            .opacity(site.developer ? 1 : 0.5)
                        VStack(alignment: .leading, spacing: 1) {
                            Text("Developer Mode").font(.system(size: 12, weight: .semibold))
                            Text(site.developer ? "On" : "Off")
                                .font(.system(size: 11))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Turn \(site.developer ? "off" : "on") Developer Mode for this site")
                if site.suspends {
                    Toggle("Keep this site awake", isOn: Binding(
                        get: { site.awake },
                        set: { app.run("awake/\($0 ? "on" : "off")/\(percentEncoded(site.site))") }
                    ))
                    .font(.system(size: 12))
                }
                Divider()
                let secure = site.url.hasPrefix("https://")
                Label(secure ? "Secure" : "Not Secure", systemImage: secure ? "lock" : "lock.open")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
            .padding(14)
            .fixedSize(horizontal: false, vertical: true)
            .background(
                GeometryReader { geometry in
                    Color.clear.onAppear { report(geometry.size.height) }
                        .onChange(of: geometry.size.height) { _, height in report(height) }
                })
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    /// Tells the app the panel's height, which it fits it to.
    private func report(_ height: CGFloat) {
        controls.app.run("site/height/\(Int(height.rounded(.up)))")
    }
}

struct SiteTile<Content: View>: View {
    let label: String
    let action: () -> Void
    @ViewBuilder let content: () -> Content

    var body: some View {
        Button(action: action) {
            content()
                .frame(width: 48, height: 40)
                .background(RoundedRectangle(cornerRadius: 8).fill(.primary.opacity(0.08)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(label)
    }
}
