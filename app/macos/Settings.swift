// The settings window, which the app opens from Arbalete › Settings… (show_settings in
// app/src/shell.rs): the app's own settings, each profile's search engine and archiving, where
// links from other apps open, and every shortcut. Escape closes it, like its close button.

import AppKit
import SwiftUI

struct SettingsDetail: Decodable {
    struct General: Decodable {
        var warnBeforeQuit: Bool
        var littleHotkey: Bool
        var shortcuts: [String: ShortcutOverride]
        var restoreSession: Bool
        var suspendTabs: Bool
        var awakeSites: [String]
    }
    struct Choice: Decodable, Identifiable, Hashable {
        var id: String
        var name: String
    }
    struct ProfileSettings: Decodable, Identifiable {
        var id: Int
        var name: String
        var color: String
        var searchEngine: String
        var searchSuggestions: Bool
        var archiveAfter: String
    }
    struct SpaceChoice: Decodable, Identifiable {
        var id: Int
        var title: String
        var emoji: String?
        var profile: String?
    }
    var settings: General
    /// ⌥⌘N is on but another app, like Arc, has it.
    var littleHotkeyTaken: Bool
    /// Each changeable shortcut's keys as the menus were built (app/macos/Shortcuts.swift).
    var shortcutDefaults: [String: Keys]
    /// Who takes each shortcut first unless changed: "websites", "both" or "app".
    var shortcutPriorities: [String: String]
    var engines: [Choice]
    var delays: [Choice]
    var profiles: [ProfileSettings]
    var spaces: [SpaceChoice]
    var urlRouting: UrlRouting
}

/// Where the links other apps open go (crates/arbalete-core/src/url_routing.rs).
struct UrlRouting: Codable {
    struct Route: Codable {
        var matching: String
        var text: String
        var destination: Destination
    }
    var routes: [Route]
    var `default`: Destination
}

/// The Space shown last, a Little Arbalete window or a given Space.
enum Destination: Codable, Hashable {
    case mostRecentSpace
    case little
    case space(Int)

    private enum Key: String, CodingKey { case space }

    init(from decoder: Decoder) throws {
        if let name = try? decoder.singleValueContainer().decode(String.self) {
            self = name == "little" ? .little : .mostRecentSpace
        } else {
            self = .space(try decoder.container(keyedBy: Key.self).decode(Int.self, forKey: .space))
        }
    }

    func encode(to encoder: Encoder) throws {
        switch self {
        case .mostRecentSpace:
            var container = encoder.singleValueContainer()
            try container.encode("mostRecentSpace")
        case .little:
            var container = encoder.singleValueContainer()
            try container.encode("little")
        case .space(let id):
            var container = encoder.container(keyedBy: Key.self)
            try container.encode(id, forKey: .space)
        }
    }
}

@MainActor
final class SettingsModel: ObservableObject {
    @Published var detail: SettingsDetail?
    @Published var pane = panes[0].title
    let app: AppModel
    private var window: NSWindow?

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        if name == "arbalete:settings" {
            self.detail = try? JSONDecoder().decode(SettingsDetail.self, from: detail)
        }
    }

    func show(_ shown: Bool) {
        guard shown else {
            window?.close()
            window = nil
            return
        }
        if let window {
            window.makeKeyAndOrderFront(nil)
            return
        }
        let window = SettingsWindow(
            // Like Arc's, measured: 650 points tall in all, whatever the pane, which scrolls.
            contentRect: NSRect(x: 0, y: 0, width: 684, height: 562),
            styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.title = pane
        window.select = { [weak self, weak window] pane in
            self?.pane = pane
            window?.title = pane
        }
        window.showPanes(selected: pane)
        window.contentView = NSHostingView(rootView: SettingsView(settings: self))
        window.center()
        window.closed = { [weak self] in self?.window = nil }
        window.escaped = { [weak self] in self?.app.run("settings/hide") }
        window.makeKeyAndOrderFront(nil)
        self.window = window
    }

    var isKey: Bool { window?.isKeyWindow ?? false }

    /// Shows another pane, as its toolbar item would.
    func choose(_ pane: String) {
        self.pane = pane
        window?.title = pane
        window?.toolbar?.selectedItemIdentifier = NSToolbarItem.Identifier(pane)
    }
}

/// The settings' panes, in the toolbar like Arc's: a symbol above each title.
private let panes = [
    (title: "General", symbol: "gearshape"),
    (title: "Profiles", symbol: "person"),
    (title: "Links", symbol: "macwindow.on.rectangle"),
    (title: "Shortcuts", symbol: "keyboard"),
    (title: "Advanced", symbol: "slider.horizontal.3"),
]

final class SettingsWindow: NSWindow, NSWindowDelegate, NSToolbarDelegate {
    var closed: () -> Void = {}
    var escaped: () -> Void = {}
    var select: (String) -> Void = { _ in }

    override init(
        contentRect: NSRect, styleMask: NSWindow.StyleMask, backing: NSWindow.BackingStoreType,
        defer flag: Bool
    ) {
        super.init(contentRect: contentRect, styleMask: styleMask, backing: backing, defer: flag)
        delegate = self
    }

    override func cancelOperation(_ sender: Any?) { escaped() }

    func windowWillClose(_ notification: Notification) { closed() }

    func showPanes(selected: String) {
        let toolbar = NSToolbar(identifier: "settings")
        toolbar.delegate = self
        toolbar.selectedItemIdentifier = NSToolbarItem.Identifier(selected)
        toolbarStyle = .preference
        self.toolbar = toolbar
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        panes.map { NSToolbarItem.Identifier($0.title) }
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbarSelectableItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbar(
        _ toolbar: NSToolbar, itemForItemIdentifier identifier: NSToolbarItem.Identifier,
        willBeInsertedIntoToolbar flag: Bool
    ) -> NSToolbarItem? {
        guard let pane = panes.first(where: { $0.title == identifier.rawValue }) else { return nil }
        let item = NSToolbarItem(itemIdentifier: identifier)
        item.label = pane.title
        // 18 points, medium, like Arc's.
        item.image = NSImage(systemSymbolName: pane.symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 18, weight: .medium))
        item.target = self
        item.action = #selector(choose(_:))
        return item
    }

    @objc private func choose(_ item: NSToolbarItem) {
        toolbar?.selectedItemIdentifier = item.itemIdentifier
        select(item.itemIdentifier.rawValue)
    }
}

struct SettingsView: View {
    @ObservedObject var settings: SettingsModel

    var body: some View {
        Group {
            switch settings.pane {
            case "Profiles": ProfilesSettings(settings: settings)
            case "Links": LinksSettings(settings: settings)
            case "Shortcuts": ShortcutsSettings(settings: settings)
            case "Advanced": AdvancedSettings(settings: settings)
            default: GeneralSettings(settings: settings)
            }
        }
        // Like Arc's: every pane 684 points wide, its content 562 tall, changing at once.
        .frame(width: 684, height: 562, alignment: .top)
    }
}

// MARK: Arc's settings parts, measured

/// A pane that scrolls, its cards 556 points wide between 64-point margins, the first 20 points
/// under the toolbar, like Arc's.
struct SettingsPage<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 8) { content }
                .frame(width: 556, alignment: .leading)
                .padding(.top, 20)
                .padding(.bottom, 32)
                .frame(maxWidth: .infinity)
        }
    }
}

/// A rounded card of rows, like Arc's: a light fill, a hairline edge; SettingsLine between rows.
struct SettingsCard<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(spacing: 0) { content }
        .background(RoundedRectangle(cornerRadius: 8).fill(settingsFill))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(settingsEdge))
    }
}

/// The line between a card's rows, 10 points inside its edges, like Arc's.
struct SettingsLine: View {
    var body: some View {
        Rectangle().fill(settingsLine).frame(height: 1).padding(.horizontal, 10)
    }
}

private let settingsFill = Color.ink((0x000000, 0.025), (0xFFFFFF, 0.015))
private let settingsEdge = Color.ink((0x000000, 0.1), (0xFFFFFF, 0.1))
private let settingsLine = Color.ink((0x000000, 0.07), (0xFFFFFF, 0.08))

/// A row of a card: its label, and its control at the right, 10 points inside, like Arc's.
struct SettingsRow<Control: View>: View {
    let label: String
    var note: String? = nil
    var height: CGFloat = 38
    @ViewBuilder let control: Control

    var body: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
                Text(label).font(.system(size: 13))
                if let note {
                    Text(note).font(.system(size: 11)).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 10)
            control
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .frame(minHeight: height)
    }
}

/// A switch row, Arc's small switch at the right.
struct SettingsSwitch: View {
    let label: String
    var note: String? = nil
    let isOn: Binding<Bool>

    var body: some View {
        SettingsRow(label: label, note: note) {
            Toggle("", isOn: isOn).labelsHidden().toggleStyle(.switch).controlSize(.small)
        }
    }
}

/// A paragraph between cards, like Arc's.
struct SettingsText: View {
    let text: String

    var body: some View {
        Text(text).font(.system(size: 13)).foregroundStyle(.secondary)
            .lineSpacing(3)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 10)
            .padding(.vertical, 10)
    }
}

// MARK: Panes

/// Like Arc's account pane's banner, the browser not being the default one, then the app's own
/// settings.
private struct GeneralSettings: View {
    @ObservedObject var settings: SettingsModel
    private let isDefault = State(initialValue: isDefaultBrowser())

    var body: some View {
        SettingsPage {
            if !isDefault.wrappedValue {
                HStack(spacing: 16) {
                    Text("Arbalete isn’t your default browser.").font(.system(size: 13))
                    Button {
                        makeDefaultBrowser()
                        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                            isDefault.wrappedValue = isDefaultBrowser()
                        }
                    } label: {
                        Label("Make Arbalete your default", systemImage: "heart.fill")
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                }
                // Like Arc's banner, centered, as wide as it needs.
                .padding(.horizontal, 12)
                .frame(height: 36)
                .fixedSize()
                .background(RoundedRectangle(cornerRadius: 6).fill(Color.ink((0x000000, 0.08), (0xFFFFFF, 0.25))))
                .frame(maxWidth: .infinity)
                .padding(.bottom, 12)
            }
            if let general = settings.detail?.settings {
                SettingsCard {
                    SettingsSwitch(
                        label: "Warn before quitting",
                        isOn: binding(general.warnBeforeQuit, "warn-before-quit"))
                }
                SettingsCard {
                    SettingsRow(label: "Manage your default search engine and Auto-Archive in Profile Settings.") {
                        Button("Profile Settings…") { settings.choose("Profiles") }
                    }
                }
            }
        }
    }

    private func binding(_ value: Bool, _ key: String) -> Binding<Bool> {
        Binding(get: { value }, set: { settings.app.run("settings/\(key)/\($0)") })
    }
}

/// Like Arc's Advanced pane: rows with a colored tile, each a switch.
private struct AdvancedSettings: View {
    @ObservedObject var settings: SettingsModel

    var body: some View {
        SettingsPage {
            if let general = settings.detail?.settings {
                SettingsCard {
                    TileSwitch(
                        tile: ("macwindow", Color(red: 0.16, green: 0.79, blue: 0.29)),
                        label: "When opening Arbalete, restore windows from previous session",
                        isOn: binding(general.restoreSession, "restore-session"))
                }
                SettingsCard {
                    TileSwitch(
                        tile: ("leaf", Color(red: 0.0, green: 0.52, blue: 0.99)),
                        label: "Save memory with tabs left aside",
                        note: "A tab hidden for 5 minutes is paused, then unloaded after an hour. Tabs playing media, using the camera or the microphone, and the sites kept awake never are.",
                        isOn: binding(general.suspendTabs, "suspend-tabs"))
                    if general.suspendTabs {
                        ForEach(general.awakeSites, id: \.self) { site in
                            SettingsLine()
                            SettingsRow(label: site) {
                                Button("Let It Sleep") {
                                    settings.app.run("awake/off/\(percentEncoded(site))")
                                }
                            }
                            .padding(.leading, 28)
                        }
                    }
                }
            }
        }
    }

    private func binding(_ value: Bool, _ key: String) -> Binding<Bool> {
        Binding(get: { value }, set: { settings.app.run("settings/\(key)/\($0)") })
    }
}

/// A row of Arc's Advanced pane: a colored tile with a white symbol, the label, a switch.
private struct TileSwitch: View {
    let tile: (String, Color)
    let label: String
    var note: String? = nil
    let isOn: Binding<Bool>

    var body: some View {
        HStack(spacing: 8.5) {
            SettingsTile(symbol: tile.0, color: tile.1, size: 20)
            SettingsSwitch(label: label, note: note, isOn: isOn).padding(.leading, -10)
        }
        .padding(.leading, 10)
    }
}

/// A rounded square of a color with a white symbol, like the icons of Arc's settings rows.
struct SettingsTile: View {
    let symbol: String
    let color: Color
    var size: CGFloat = 24

    var body: some View {
        Image(systemName: symbol).font(.system(size: size * 0.5, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(RoundedRectangle(cornerRadius: size * 0.22).fill(color))
    }
}

/// Like Arc's Links pane: a paragraph, then a card leading to Air Traffic Control, its sheet of
/// routes for the links other apps open.
private struct LinksSettings: View {
    @ObservedObject var settings: SettingsModel
    private let shown = State(initialValue: false)

    var body: some View {
        SettingsPage {
            SettingsText(text: "Little Arbalete is a smaller, simpler Arbalete window. It’s perfect for quick lookups, and for the links other apps open, where Air Traffic Control sends them.")
            if let detail = settings.detail {
                SettingsCard {
                    SettingsSwitch(
                        label: "Open Little Arbalete when I press ⌥⌘N in any app",
                        note: detail.littleHotkeyTaken
                            ? "Another app, like Arc, already uses ⌥⌘N: quit it or turn its shortcut off." : nil,
                        isOn: Binding(
                            get: { detail.settings.littleHotkey },
                            set: { settings.app.run("settings/little-hotkey/\($0)") }))
                }
            }
            SettingsCard {
                SettingsRow(label: "Choose where links from other apps open.", height: 44) {
                    Button("Air Traffic Control…") { shown.wrappedValue = true }
                }
            }
        }
        .sheet(isPresented: shown.projectedValue) {
            if let detail = settings.detail {
                AirTrafficControl(settings: settings, detail: detail) { shown.wrappedValue = false }
            }
        }
    }
}

/// URL Routing, like Arc's Air Traffic Control: each route sends the links whose url contains a
/// text, or is it, to a Space, the Space shown last or a Little Arbalete window; the default
/// takes the others.
private struct AirTrafficControl: View {
    @ObservedObject var settings: SettingsModel
    let detail: SettingsDetail
    let close: () -> Void

    var body: some View {
        let routing = settings.detail?.urlRouting ?? detail.urlRouting
        VStack(spacing: 0) {
            HStack {
                Text("Air Traffic Control").font(.system(size: 15, weight: .semibold))
                Spacer()
                Button("New Route") {
                    var changed = routing
                    changed.routes.append(
                        .init(matching: "contains", text: "", destination: .mostRecentSpace))
                    save(changed)
                }
                Button(action: close) {
                    Image(systemName: "xmark").font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .keyboardShortcut(.cancelAction)
                .padding(.leading, 20)
            }
            .padding(.horizontal, 24)
            .frame(height: 84)
            Divider().padding(.horizontal, 24)
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    ForEach(routing.routes.indices, id: \.self) { index in
                        RouteRow(
                            route: routing.routes[index], spaces: detail.spaces,
                            change: { route in
                                var changed = routing
                                changed.routes[index] = route
                                save(changed)
                            },
                            remove: {
                                var changed = routing
                                changed.routes.remove(at: index)
                                save(changed)
                            })
                    }
                    if routing.routes.isEmpty {
                        Text("No routes yet: links from other apps open where the default says.")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            Divider().padding(.horizontal, 24)
            HStack(spacing: 16) {
                Text("Default").frame(width: 72, alignment: .leading)
                Picker("", selection: Binding(
                    get: { routing.default },
                    set: { destination in
                        var changed = routing
                        changed.default = destination
                        save(changed)
                    }
                )) {
                    DestinationChoices(spaces: detail.spaces)
                }
                .labelsHidden()
                .frame(width: 197)
                Spacer()
            }
            .padding(.horizontal, 36)
            .frame(height: 74)
        }
        .frame(width: 508, height: 500)
    }

    private func save(_ routing: UrlRouting) {
        guard let json = try? JSONEncoder().encode(routing),
            let text = String(data: json, encoding: .utf8)
        else { return }
        settings.app.run("routing/\(percentEncoded(text))")
    }
}

/// A route: how and what it matches in a url, and where those links open.
private struct RouteRow: View {
    let route: UrlRouting.Route
    let spaces: [SettingsDetail.SpaceChoice]
    let change: (UrlRouting.Route) -> Void
    let remove: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Label("URL", systemImage: "link").foregroundStyle(.secondary).frame(width: 56, alignment: .leading)
                // Like Arc's, "Is equal to" first.
                Picker("", selection: binding(\.matching)) {
                    Text("Is equal to").tag("isEqualTo")
                    Text("Contains").tag("contains")
                }
                .labelsHidden()
                .frame(width: 109)
                TextField("Text", text: binding(\.text), prompt: Text("example.com"))
                    .labelsHidden()
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 210)
                Button(action: remove) {
                    Image(systemName: "minus.circle.fill")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help("Remove the route")
                .accessibilityLabel("Remove the route")
            }
            HStack(spacing: 8) {
                Label("Open in", systemImage: "arrow.turn.down.right").foregroundStyle(.secondary)
                    .fixedSize()
                    .frame(width: 77, alignment: .leading)
                Picker("", selection: binding(\.destination)) {
                    DestinationChoices(spaces: spaces)
                }
                .labelsHidden()
                .frame(width: 197)
            }
            .padding(.leading, 48)
        }
    }

    private func binding<Value>(_ field: WritableKeyPath<UrlRouting.Route, Value>) -> Binding<Value> {
        Binding(
            get: { route[keyPath: field] },
            set: { value in
                var changed = route
                changed[keyPath: field] = value
                change(changed)
            })
    }
}

/// Where a link may open: each Space, with its profile, then the Space shown last and a Little
/// Arbalete window.
private struct DestinationChoices: View {
    let spaces: [SettingsDetail.SpaceChoice]

    var body: some View {
        ForEach(spaces) { space in
            // A symbol shows only in the sidebar.
            let emoji = space.emoji.flatMap { symbolName($0) == nil ? $0 : nil }
            let name = [emoji, space.title].compactMap { $0 }.joined(separator: " ")
            Text(space.profile.map { "\(name) (\($0))" } ?? name).tag(Destination.space(space.id))
        }
        Divider()
        Text("Most Recent Space").tag(Destination.mostRecentSpace)
        Text("Little Arbalete").tag(Destination.little)
    }
}

/// Like Arc's Profiles pane: what profiles are for, the list of profiles at the left, the
/// picked one's settings at the right, then its data in Chromium's own pages.
private struct ProfilesSettings: View {
    @ObservedObject var settings: SettingsModel
    private let picked = State<Int?>(initialValue: nil)

    var body: some View {
        if let detail = settings.detail {
            let profile = detail.profiles.first { $0.id == picked.wrappedValue } ?? detail.profiles.first
            SettingsPage {
                Text("Profiles help keep your data separate across Spaces — like history, logins, cookies, and extensions. You can use any Profile across one or more Spaces.")
                    .font(.system(size: 13))
                    .lineSpacing(3)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.bottom, 12)
                HStack(alignment: .top, spacing: 10) {
                    ProfileList(
                        detail: detail, picked: profile?.id, pick: { picked.wrappedValue = $0 },
                        create: { settings.app.newProfile(for: nil, names: detail.profiles.map(\.name)) },
                        rename: rename, delete: delete)
                        .frame(width: 205)
                    if let profile {
                        ProfileDetail(settings: settings, detail: detail, profile: profile)
                    }
                }
            }
        }
    }

    private func rename(_ profile: SettingsDetail.ProfileSettings) {
        let names = settings.detail?.profiles.map(\.name) ?? []
        showDialog { close in
            RenameProfileDialog(profile: profile, names: names, close: close) {
                settings.app.run("profile/rename/\(profile.id)/\(percentEncoded($0))")
            }
        }
    }

    private func delete(_ profile: SettingsDetail.ProfileSettings) {
        showDialog { close in
            DeleteProfileDialog(profile: profile, close: close) {
                settings.app.run("profile/delete/\(profile.id)")
            }
        }
    }
}

/// The profiles, like Arc's list: each with how many Spaces use it, renamed with a double click,
/// and under them the buttons creating one, deleting the picked one and renaming it.
private struct ProfileList: View {
    let detail: SettingsDetail
    let picked: Int?
    let pick: (Int) -> Void
    let create: () -> Void
    let rename: (SettingsDetail.ProfileSettings) -> Void
    let delete: (SettingsDetail.ProfileSettings) -> Void

    private var profile: SettingsDetail.ProfileSettings? {
        detail.profiles.first { $0.id == picked }
    }

    /// Why the picked profile cannot be deleted, like Arc's, if it cannot.
    private var undeletable: String? {
        guard let profile else { return "" }
        if profile.id == detail.profiles.first?.id { return "The Default Profile cannot be deleted." }
        if detail.spaces.contains(where: { $0.profile == profile.name }) {
            return "Only Profiles not assigned to any Spaces can be deleted."
        }
        return nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Your Profiles").font(.system(size: 12, weight: .semibold)).foregroundStyle(.secondary)
                .padding(.horizontal, 10)
                .frame(height: 28)
            Rectangle().fill(settingsLine).frame(height: 1)
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(Array(detail.profiles.enumerated()), id: \.element.id) { index, profile in
                        if index > 0 {
                            Rectangle().fill(settingsLine).frame(height: 1).padding(.horizontal, 14)
                        }
                        row(profile)
                    }
                }
                .padding(.vertical, 6)
            }
            Rectangle().fill(settingsLine).frame(height: 1)
            HStack(spacing: 0) {
                footerButton("plus", help: "Create a new Profile", action: create)
                Rectangle().fill(settingsLine).frame(width: 1, height: 16)
                footerButton("minus", help: undeletable ?? "Delete this Profile") {
                    profile.map(delete)
                }
                .disabled(undeletable != nil)
                Rectangle().fill(settingsLine).frame(width: 1, height: 16)
                footerButton("rectangle.and.pencil.and.ellipsis", help: "Rename your Profile") {
                    profile.map(rename)
                }
                .disabled(profile == nil)
                Spacer()
            }
            .frame(height: 30)
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(settingsEdge))
    }

    private func row(_ profile: SettingsDetail.ProfileSettings) -> some View {
        let used = detail.spaces.filter { $0.profile == profile.name }.count
        return Button {
            pick(profile.id)
        } label: {
            HStack {
                Text(profile.name).font(.system(size: 13))
                Spacer()
                Text(used == 0 ? "Not used" : used == 1 ? "1 Space" : "\(used) Spaces")
                    .font(.system(size: 12))
                    .foregroundStyle(used == 0 ? .tertiary : .secondary)
            }
            .padding(.horizontal, 8)
            .frame(height: 32)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(profile.id == picked ? Color.accentColor.opacity(0.18) : .clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .simultaneousGesture(TapGesture(count: 2).onEnded { rename(profile) })
        .padding(.horizontal, 6)
        .padding(.vertical, 4)
    }

    /// A button of the list's footer, like Arc's: a symbol, 30 points wide.
    private func footerButton(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 13))
                .frame(width: 30, height: 30)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help(help)
    }
}

/// A profile's settings, like Arc's: its search, its archiving, then its data and settings,
/// which Chromium's own pages show.
private struct ProfileDetail: View {
    @ObservedObject var settings: SettingsModel
    let detail: SettingsDetail
    let profile: SettingsDetail.ProfileSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            SettingsCard {
                SettingsRow(label: "Search engine", height: 43) {
                    Picker("", selection: Binding(
                        get: { profile.searchEngine },
                        set: { settings.app.run("profile/search/\(profile.id)/\($0)") }
                    )) {
                        ForEach(detail.engines) { Text($0.name).tag($0.id) }
                    }
                    .labelsHidden()
                    .frame(width: 153)
                }
                SettingsLine()
                SettingsSwitch(
                    label: "Include search engine suggestions",
                    isOn: Binding(
                        get: { profile.searchSuggestions },
                        set: { settings.app.run("profile/suggestions/\(profile.id)/\($0 ? "on" : "off")") }))
            }
            SettingsCard {
                SettingsRow(label: "Archive tabs after", height: 44) {
                    Picker("", selection: Binding(
                        get: { profile.archiveAfter },
                        set: { settings.app.run("profile/archive/\(profile.id)/\($0)") }
                    )) {
                        ForEach(detail.delays) { Text($0.name).tag($0.id) }
                    }
                    .labelsHidden()
                    .frame(width: 153)
                }
            }
            Text("Your Data and Settings").font(.system(size: 12)).foregroundStyle(.tertiary)
                .padding(.leading, 10)
                .padding(.top, 10)
            VStack(spacing: 0) {
                ForEach(dataPages, id: \.0) { label, symbol, tint, address in
                    DataRow(label: label, symbol: symbol, tint: tint) {
                        settings.app.run("open/\(percentEncoded(address))")
                    }
                }
            }
            .padding(.vertical, 9)
            .background(RoundedRectangle(cornerRadius: 8).fill(settingsFill))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(settingsEdge))
        }
    }
}

/// Like Arc's, a profile's data in Chromium's own settings pages.
private let dataPages: [(String, String, Color, String)] = [
    ("Privacy and Security", "lock.fill", Color(red: 0.07, green: 0.45, blue: 0.85), "chrome://settings/privacy"),
    ("Passwords", "key.fill", Color(red: 0.16, green: 0.67, blue: 0.27), "chrome://password-manager/passwords"),
    ("Credit Cards", "creditcard.fill", Color(red: 0.85, green: 0.69, blue: 0.09), "chrome://settings/payments"),
    ("Notifications", "bell.fill", Color(red: 0.85, green: 0.45, blue: 0.16), "chrome://settings/content/notifications"),
    ("Clear Browsing Data", "trash.fill", Color(red: 0.85, green: 0.2, blue: 0.22), "chrome://settings/clearBrowserData"),
]

/// A row leading to one of Chromium's pages, like Arc's: a colored tile, the label, an arrow.
private struct DataRow: View {
    let label: String
    let symbol: String
    let tint: Color
    let action: () -> Void
    private let hovered = State(initialValue: false)

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                SettingsTile(symbol: symbol, color: tint)
                Text(label).font(.system(size: 13, weight: .medium))
                Spacer()
                Image(systemName: "arrow.right").font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color.accentColor)
            }
            .padding(.horizontal, 6)
            .frame(height: 34)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(Color.accentColor.opacity(hovered.wrappedValue ? 0.12 : 0)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 4)
        .onHover { hovered.wrappedValue = $0 }
    }
}

@MainActor var settingsModel: SettingsModel?

@_cdecl("arbalete_ui_settings")
public func showSettings(_ shown: Bool) {
    MainActor.assumeIsolated {
        settingsModel?.show(shown)
    }
}

/// Makes this copy of Arbalete the default browser, from Arbalete › Set as Default Browser, like
/// Arc: macOS asks to confirm. Registered first, this copy is the one macOS picks among those with
/// its identifier.
@_cdecl("arbalete_ui_make_default_browser")
public func makeDefaultBrowser() {
    let app = Bundle.main.bundleURL
    LSRegisterURL(app as CFURL, true)
    NSWorkspace.shared.setDefaultApplication(at: app, toOpenURLsWithScheme: "http") { error in
        guard error == nil else { return failed(error) }
        NSWorkspace.shared.setDefaultApplication(
            at: app, toOpenURLsWithScheme: "https", completion: failed)
    }
}

/// Whether a copy of Arbalete is the default browser: macOS never picks one in a temporary folder.
@_cdecl("arbalete_ui_is_default_browser")
public func isDefaultBrowser() -> Bool {
    guard let url = URL(string: "https://example.com"),
        let app = NSWorkspace.shared.urlForApplication(toOpen: url)
    else { return false }
    return Bundle(url: app)?.bundleIdentifier == Bundle.main.bundleIdentifier
}

private func failed(_ error: Error?) {
    guard let error else { return }
    DispatchQueue.main.async { NSApp.presentError(error) }
}

@_cdecl("arbalete_ui_settings_focused")
public func settingsFocused() -> Bool {
    MainActor.assumeIsolated {
        settingsModel?.isKey ?? false
    }
}
