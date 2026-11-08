// The import assistant, shown above the page by the app on the first launch and from the menu
// (show_import in app/src/shell/import_ui.rs), to import other browsers' data. Arc is picked by
// default: Arbalete users most likely come from it. While the app reads the browsers' data, it
// shows the import running; the app hides it once done.

import AppKit
import SwiftUI

struct Sources: Decodable {
    struct Browser: Decodable {
        struct BrowserProfile: Decodable {
            var name: String
            var needsFullDiskAccess: Bool
        }
        var name: String
        /// Arc's sidebar is imported as Spaces: it has no bookmarks bar to place.
        var arc: Bool
        var profiles: [BrowserProfile]
    }
    struct Target: Decodable, Identifiable {
        var id: Int
        var name: String
    }
    var browsers: [Browser]
    /// The existing profiles to merge into, none on the first launch.
    var profiles: [Target]
}

/// A Chrome Web Store extension an import found in another browser's profile, to add to the
/// profile it went into (offered_extensions in app/src/shell/import_ui.rs).
struct FoundExtension: Codable, Hashable {
    var profile: Int
    var id: String
    var name: String
    /// Its icon, in the other browser's folder.
    var icon: String?
}

/// An extension's icon, from its file, or a puzzle piece.
struct FoundExtensionIcon: View {
    let path: String?

    var body: some View {
        if let path, let image = NSImage(contentsOfFile: path) {
            Image(nsImage: image).resizable().interpolation(.high)
        } else {
            Image(systemName: "puzzlepiece.extension").resizable().scaledToFit().opacity(0.7)
        }
    }
}

/// What to do with a browser profile.
struct ImportChoice: Encodable {
    var browser: Int
    var profile: Int
    var checked: Bool
    var into: Int?
    var bar: String

    private enum Keys: String, CodingKey {
        case browser, profile, into, bar
    }

    func encode(to encoder: Encoder) throws {
        var keys = encoder.container(keyedBy: Keys.self)
        try keys.encode(browser, forKey: .browser)
        try keys.encode(profile, forKey: .profile)
        try keys.encode(into, forKey: .into)
        try keys.encode(bar, forKey: .bar)
    }
}

@MainActor
final class ImportModel: ObservableObject {
    @Published var sources: Sources?
    @Published var choices: [ImportChoice] = []
    @Published var importing = false
    /// Once imported, the extensions found, and those to add.
    @Published var found: [FoundExtension] = []
    @Published var adding: Set<FoundExtension> = []
    let app: AppModel

    init(app: AppModel) {
        self.app = app
    }

    private struct Found: Decodable {
        var extensions: [FoundExtension]
    }

    func handle(_ name: String, _ detail: Data) {
        if name == "arbalete:extensions", let found = try? JSONDecoder().decode(Found.self, from: detail) {
            self.found = found.extensions
            adding = Set(found.extensions)
            importing = false
            return
        }
        guard name == "arbalete:sources",
            let sources = try? JSONDecoder().decode(Sources.self, from: detail)
        else { return }
        self.sources = sources
        (importing, found, adding) = (false, [], [])
        choices = sources.browsers.enumerated().flatMap { b, browser in
            browser.profiles.enumerated().map { p, profile in
                ImportChoice(
                    browser: b, profile: p, checked: browser.arc && !profile.needsFullDiskAccess,
                    into: nil, bar: "favorites")
            }
        }
    }

    func index(_ browser: Int, _ profile: Int) -> Int? {
        choices.firstIndex { $0.browser == browser && $0.profile == profile }
    }

    func run() {
        let picked = choices.filter(\.checked)
        guard !picked.isEmpty else { return }
        importing = true
        app.run("import", jsonObject(picked))
    }

    /// Adds the extensions picked, then closes: Chromium asks to confirm each one.
    func addExtensions() {
        let picked = found.filter(adding.contains)
        if !picked.isEmpty { app.run("install-extensions", jsonObject(picked)) }
        app.run("close-import")
    }
}

struct ImportView: View {
    @ObservedObject var model: ImportModel

    var body: some View {
        if !model.found.isEmpty {
            ImportExtensionsView(model: model)
        } else if let sources = model.sources {
            VStack(alignment: .leading, spacing: 14) {
                Text("Import from another browser").font(.title2.bold())
                Text("From Arc: its Spaces, favorites, tabs and history. From other browsers: bookmarks and history.")
                    .foregroundStyle(.secondary)
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        ForEach(Array(sources.browsers.enumerated()), id: \.offset) { b, browser in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(browser.name).font(.headline)
                                ForEach(Array(browser.profiles.enumerated()), id: \.offset) { p, profile in
                                    if profile.needsFullDiskAccess {
                                        HStack {
                                            Text("macOS keeps \(browser.name)'s data private. To import it, give Arbalete Full Disk Access in System Settings, then open the import again.")
                                                .font(.system(size: 12))
                                            Button("Open System Settings") { model.app.run("open-full-disk-access") }
                                        }
                                    } else if let i = model.index(b, p) {
                                        ImportRow(model: model, index: i, name: profile.name, arc: browser.arc, targets: sources.profiles)
                                    }
                                }
                            }
                            .disabled(model.importing)
                        }
                    }
                }
                HStack {
                    Spacer()
                    Button("Not now") { model.app.run("close-import") }
                        .disabled(model.importing)
                        .keyboardShortcut(.cancelAction)
                    Button(model.importing ? "Importing…" : "Import") { model.run() }
                        .disabled(model.importing || !model.choices.contains(where: \.checked))
                        .keyboardShortcut(.defaultAction)
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
    }
}

struct ImportRow: View {
    @ObservedObject var model: ImportModel
    let index: Int
    let name: String
    let arc: Bool
    let targets: [Sources.Target]

    var body: some View {
        HStack {
            Toggle(name, isOn: $model.choices[index].checked)
            Spacer()
            if !targets.isEmpty {
                Picker("", selection: $model.choices[index].into) {
                    Text("New profile").tag(Int?.none)
                    ForEach(targets) { Text("Into \($0.name)").tag(Int?.some($0.id)) }
                }
                .labelsHidden()
                .fixedSize()
            }
            if !arc {
                Picker("", selection: $model.choices[index].bar) {
                    Text("Bar to Favorites").tag("favorites")
                    Text("Bar to a pinned folder").tag("pinnedFolder")
                }
                .labelsHidden()
                .fixedSize()
            }
        }
    }
}

/// After an import, the extensions found, to add from the Chrome Web Store (decision 0008).
struct ImportExtensionsView: View {
    @ObservedObject var model: ImportModel

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Add your extensions").font(.title2.bold())
            Text("Arbalete adds them from the Chrome Web Store, one at a time: confirm each one as it asks.")
                .foregroundStyle(.secondary)
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(model.found, id: \.self) { found in
                        Toggle(isOn: Binding(
                            get: { model.adding.contains(found) },
                            set: { if $0 { model.adding.insert(found) } else { model.adding.remove(found) } })
                        ) {
                            HStack(spacing: 8) {
                                FoundExtensionIcon(path: found.icon).frame(width: 16, height: 16)
                                Text(found.name)
                            }
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack {
                Spacer()
                Button("Not now") { model.app.run("close-import") }
                    .keyboardShortcut(.cancelAction)
                Button(model.adding.count > 1 ? "Add \(model.adding.count)" : "Add") { model.addExtensions() }
                    .disabled(model.adding.isEmpty)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}
