// The import assistant, shown above the page by the app on the first launch
// and from the menu (show_import in app/src/shell.rs), to import other browsers' data. Arc is
// picked by default: Arbalete users most likely come from it. While the app reads the browsers'
// data, it shows the import running; the app hides it once done.

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
    let app: AppModel

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        guard name == "arbalete:sources",
            let sources = try? JSONDecoder().decode(Sources.self, from: detail)
        else { return }
        self.sources = sources
        importing = false
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
        guard !picked.isEmpty,
            let json = (try? JSONEncoder().encode(picked)).flatMap({ String(data: $0, encoding: .utf8) })
        else { return }
        importing = true
        app.run("import/run/\(percentEncoded(json))")
    }
}

struct ImportView: View {
    @ObservedObject var model: ImportModel

    var body: some View {
        if let sources = model.sources {
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
                                            Button("Open System Settings") { model.app.run("import/full-disk-access") }
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
                    Button("Not now") { model.app.run("import/close") }
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
