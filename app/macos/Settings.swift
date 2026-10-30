// The settings window (ui/src/Settings.tsx), which the app opens from Arbalete › Settings…
// (show_settings in app/src/shell.rs): the app's own settings, each profile's search engine and
// archiving, and every shortcut. Escape closes it, like its close button.

import AppKit
import SwiftUI

struct SettingsDetail: Decodable {
    struct General: Decodable {
        var warnBeforeQuit: Bool
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
    var settings: General
    var engines: [Choice]
    var delays: [Choice]
    var profiles: [ProfileSettings]
}

/// The palette of profiles and Spaces (Color in crates/arbalete-core/src/sidebar.rs).
let profileColors = [
    "purple": "#6e56cf", "blue": "#3e7bfa", "green": "#30a46c", "yellow": "#e5b800",
    "orange": "#f76b15", "red": "#e5484d", "pink": "#d6409f", "gray": "#8b8d98",
]

/// Every shortcut, as the app's menus and keyboard monitor define them (app/src/mac.rs).
let shortcuts: [(String, [(String, String)])] = [
    ("Tabs", [
        ("New Tab", "⌘T"), ("Edit Address", "⌘L"), ("Close Tab", "⌘W"),
        ("Reopen Closed Tab", "⇧⌘T"), ("Pin or Unpin Tab", "⌘D"), ("Copy URL", "⇧⌘C"),
        ("Next Tab", "⌥⌘↓ or ⌘}"), ("Previous Tab", "⌥⌘↑ or ⌘{"),
        ("Switch Between Recent Tabs", "⌃⇥, ⌃⇧⇥"), ("Favorite 1 to 8", "⌘1 … ⌘8"),
        ("Last Favorite", "⌘9"), ("Clear Today", "⇧⌘K"),
    ]),
    ("Spaces", [("Next Space", "⌥⌘→"), ("Previous Space", "⌥⌘←"), ("Space 1 to 9", "⌃1 … ⌃9")]),
    ("Page", [
        ("Back", "⌘["), ("Forward", "⌘]"), ("Reload", "⌘R"), ("Force Reload", "⇧⌘R"),
        ("Stop Loading", "⌘."), ("Find", "⌘F"), ("Find Next", "⌘G"), ("Find Previous", "⇧⌘G"),
        ("Actual Size", "⌘0"), ("Zoom In", "⌘="), ("Zoom Out", "⌘-"), ("Print", "⌘P"),
    ]),
    ("Developer", [("View Source", "⌥⌘U"), ("Developer Tools", "⌥⌘I or F12")]),
    ("Window", [
        ("Hide or Show Sidebar", "⌘S"), ("Show or Hide Toolbar", "⇧⌘D"),
        ("New Little Arbalete Window", "⌥⌘N"), ("Open Little Arbalete in Space", "⌘O"),
        ("Library", "⇧⌘L"), ("History", "⌘Y"), ("Downloads", "⇧⌘J"), ("Settings", "⌘,"),
        ("Minimize", "⌘M"), ("Quit", "⌘Q"),
    ]),
]

@MainActor
final class SettingsModel: ObservableObject {
    @Published var detail: SettingsDetail?
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
            contentRect: NSRect(x: 0, y: 0, width: 640, height: 480),
            styleMask: [.titled, .closable, .fullSizeContentView], backing: .buffered, defer: false)
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.title = "Settings"
        window.contentView = NSHostingView(rootView: SettingsView(settings: self))
        window.center()
        window.closed = { [weak self] in self?.window = nil }
        window.escaped = { [weak self] in self?.app.run("settings/hide") }
        window.makeKeyAndOrderFront(nil)
        self.window = window
    }

    var isKey: Bool { window?.isKeyWindow ?? false }
}

final class SettingsWindow: NSWindow, NSWindowDelegate {
    var closed: () -> Void = {}
    var escaped: () -> Void = {}

    override init(
        contentRect: NSRect, styleMask: NSWindow.StyleMask, backing: NSWindow.BackingStoreType,
        defer flag: Bool
    ) {
        super.init(contentRect: contentRect, styleMask: styleMask, backing: backing, defer: flag)
        delegate = self
    }

    override func cancelOperation(_ sender: Any?) { escaped() }

    func windowWillClose(_ notification: Notification) { closed() }
}

struct SettingsView: View {
    @ObservedObject var settings: SettingsModel

    var body: some View {
        TabView {
            General(settings: settings).tabItem { Text("General") }
            ProfilesSettings(settings: settings)
                .tabItem { Text("Profiles") }
            ShortcutsList().tabItem { Text("Shortcuts") }
        }
        .padding(.top, 28)
        .padding(16)
        .frame(minWidth: 600, minHeight: 440)
    }
}

private struct General: View {
    @ObservedObject var settings: SettingsModel

    var body: some View {
        if let general = settings.detail?.settings {
            Form {
                Toggle(isOn: binding(general.warnBeforeQuit, "warn-before-quit")) {
                    Text("Warn before quitting")
                    Text("⌘Q asks before closing Arbalete.")
                }
                Toggle(isOn: binding(general.restoreSession, "restore-session")) {
                    Text("Restore the previous session")
                    Text("Reopens the “Today” tabs left open at the last quit. Pinned tabs and favorites always stay.")
                }
                Toggle(isOn: binding(general.suspendTabs, "suspend-tabs")) {
                    Text("Save memory with tabs left aside")
                    Text("A tab hidden for 5 minutes is paused, then unloaded after an hour. Tabs playing media, using the camera or the microphone, and the sites kept awake never are.")
                }
                if general.suspendTabs && !general.awakeSites.isEmpty {
                    Section("Sites kept awake") {
                        ForEach(general.awakeSites, id: \.self) { site in
                            HStack {
                                Text(site)
                                Spacer()
                                Button {
                                    settings.app.run("awake/off/\(percentEncoded(site))")
                                } label: {
                                    Image(systemName: "xmark")
                                }
                                .buttonStyle(.plain)
                                .foregroundStyle(.secondary)
                                .help("Let it sleep")
                                .accessibilityLabel("Let \(site) sleep")
                            }
                        }
                    }
                }
            }
            .formStyle(.grouped)
        }
    }

    private func binding(_ value: Bool, _ key: String) -> Binding<Bool> {
        Binding(get: { value }, set: { settings.app.run("settings/\(key)/\($0)") })
    }
}

private struct ProfilesSettings: View {
    @ObservedObject var settings: SettingsModel

    var body: some View {
        if let detail = settings.detail {
            Form {
                ForEach(detail.profiles) { profile in
                    Section {
                        Picker("Search engine", selection: Binding(
                            get: { profile.searchEngine },
                            set: { settings.app.run("profile/search/\(profile.id)/\($0)") }
                        )) {
                            ForEach(detail.engines) { Text($0.name).tag($0.id) }
                        }
                        Toggle("Search suggestions", isOn: Binding(
                            get: { profile.searchSuggestions },
                            set: { settings.app.run("profile/suggestions/\(profile.id)/\($0 ? "on" : "off")") }
                        ))
                        Picker("Archive tabs after", selection: Binding(
                            get: { profile.archiveAfter },
                            set: { settings.app.run("profile/archive/\(profile.id)/\($0)") }
                        )) {
                            ForEach(detail.delays) { Text($0.name).tag($0.id) }
                        }
                    } header: {
                        HStack {
                            Circle().fill(color(profileColors[profile.color] ?? "#8b8d98"))
                                .frame(width: 10, height: 10)
                            Text(profile.name)
                        }
                    }
                }
            }
            .formStyle(.grouped)
        }
    }
}

private struct ShortcutsList: View {
    var body: some View {
        Form {
            ForEach(shortcuts, id: \.0) { group, keys in
                Section(group) {
                    ForEach(keys, id: \.0) { action, key in
                        LabeledContent(action) { Text(key).monospaced() }
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}

@MainActor var settingsModel: SettingsModel?

@_cdecl("arbalete_ui_settings")
public func showSettings(_ shown: Bool) {
    MainActor.assumeIsolated {
        settingsModel?.show(shown)
    }
}

@_cdecl("arbalete_ui_settings_focused")
public func settingsFocused() -> Bool {
    MainActor.assumeIsolated {
        settingsModel?.isKey ?? false
    }
}
