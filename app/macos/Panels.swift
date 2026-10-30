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
        HStack(spacing: 4) {
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
            }
            IconButton(symbol: "chevron.up", label: "Previous match", size: 24) {
                bar.find(forward: false)
            }
            IconButton(symbol: "chevron.down", label: "Next match", size: 24) {
                bar.find(forward: true)
            }
            IconButton(symbol: "xmark", label: "Close", size: 24) { bar.hide() }
        }
        .padding(.horizontal, 10)
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

    var body: some View {
        let tab = model.activeTab
        ZStack {
            HStack(spacing: 2) {
                NavButtons(model: model)
                Spacer()
                if tab != nil {
                    ForEach(model.state?.extensions ?? []) { ext in
                        PinnedExtension(model: model, extension: ext, view: "toolbar")
                    }
                    SiteControlsButton(model: model, view: "toolbar")
                }
            }
            if let tab {
                Button {
                    model.run("overlay/edit")
                } label: {
                    Label(domain(tab.url), systemImage: "link")
                        .lineLimit(1)
                        .padding(.horizontal, 10)
                        .frame(height: 26)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(tab.url)
            }
        }
        .padding(.horizontal, 6)
        .frame(maxHeight: .infinity)
        .overlay(alignment: .bottom) {
            if let page = model.state?.page, page.loading {
                ProgressView(value: page.progress).progressViewStyle(.linear).controlSize(.mini)
            }
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
