// The library (ui/src/Library.tsx), which the sidebar shows instead of a Space's page: a rail of
// sections, the one picked last kept until the app quits, and the picked section under a field
// searching it, but for the Spaces and the extensions, which fill the window.

import AppKit
import SwiftUI

enum LibrarySection: String, CaseIterable, Identifiable {
    case spaces, closed, archive, history, media, downloads, extensions

    var id: String { rawValue }

    var label: String {
        switch self {
        case .spaces: "Spaces"
        case .closed: "Recently Closed"
        case .archive: "Archived Tabs"
        case .history: "History"
        case .media: "Media"
        case .downloads: "Downloads"
        case .extensions: "Extensions"
        }
    }

    var symbol: String {
        switch self {
        case .spaces: "rectangle.on.rectangle"
        case .closed: "clock.arrow.circlepath"
        case .archive: "archivebox"
        case .history: "clock"
        case .media: "photo"
        case .downloads: "arrow.down.circle"
        case .extensions: "puzzlepiece.extension"
        }
    }

    /// The Spaces and the extensions fill the window, the page hidden; the other sections rest
    /// beside it.
    var wide: Bool { self == .spaces || self == .extensions }
}

struct ClosedTab: Decodable {
    var url: String
    var title: String
    var favicon: String?
    var closedAt: Double
}

struct ArchivedTab: Decodable {
    var url: String
    var title: String
    var favicon: String?
    var space: String?
    var archivedAt: Double
}

struct Visit: Decodable {
    var url: String
    var title: String
    var lastVisit: Double
}

struct Download: Decodable, Identifiable {
    var id: Int
    var name: String
    var url: String
    var received: Double
    var total: Double
    var speed: Double
    var state: String
    var startedAt: Double
    var missing: Bool
}

struct InstalledExtension: Decodable, Identifiable {
    var id: String
    var name: String
    var icon: String?
    var popup: Bool
    var enabled: Bool
    var pinned: Bool
    var options: Bool
    var description: String
    var version: String
}

/// How long ago a time was, both in milliseconds since the epoch, in a few words: "2 min ago".
func ago(_ time: Double, now: Double = Date().timeIntervalSince1970 * 1000) -> String {
    let minutes = Int((now - time) / 60_000)
    if minutes < 1 { return "Just now" }
    if minutes < 60 { return "\(minutes) min ago" }
    let hours = minutes / 60
    if hours < 24 { return "\(hours) h ago" }
    return "\(hours / 24) d ago"
}

/// Whether any of `texts` holds what is typed in a section's search field, whatever the case.
func matches(_ query: String, _ texts: String...) -> Bool {
    let query = query.trimmingCharacters(in: .whitespaces).lowercased()
    return query.isEmpty || texts.contains { $0.lowercased().contains(query) }
}

/// "12.3 / 45.6 MB · 8 s left" while downloading, then what became of it.
func downloadDetail(_ download: Download) -> String {
    func unit(_ bytes: Double) -> (String, Double) {
        bytes >= 1e9 ? ("GB", 1e9) : bytes >= 1e6 ? ("MB", 1e6) : bytes >= 1e3 ? ("KB", 1e3) : ("B", 1)
    }
    func amount(_ bytes: Double, _ divisor: Double) -> String {
        divisor == 1 ? String(Int(bytes)) : String(format: "%.1f", bytes / divisor)
    }
    func size(_ bytes: Double) -> String {
        let (name, divisor) = unit(bytes)
        return "\(amount(bytes, divisor)) \(name)"
    }
    switch download.state {
    case "complete": return download.missing ? "Deleted" : size(download.received)
    case "canceled": return "Canceled"
    case "failed": return "Failed"
    default: break
    }
    guard download.total > 0 else { return size(download.received) }
    let (name, divisor) = unit(download.total)
    let progress = "\(amount(download.received, divisor)) / \(amount(download.total, divisor)) \(name)"
    guard download.speed > 0 else { return progress }
    let left = Int(((download.total - download.received) / download.speed).rounded(.up))
    let time = left < 60 ? "\(left) s" : left < 3600 ? "\(left / 60) min" : "\(left / 3600) h"
    return "\(progress) · \(time) left"
}

struct LibraryView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        HStack(spacing: 0) {
            // Like Arc's, measured: the sections in a column of their own, a little lighter than
            // the window, centered, each a large icon over its name; the one shown tinted.
            VStack(spacing: 21) {
                Spacer(minLength: 40)
                ForEach(LibrarySection.allCases) { section in
                    let shown = model.section == section
                    Button {
                        model.pick(section)
                    } label: {
                        VStack(spacing: 8) {
                            Image(systemName: section.symbol).font(.system(size: 22))
                                .foregroundStyle(shown ? model.accent : Ink.secondary)
                                .frame(height: 26)
                            Text(section.label).font(.system(size: 12, weight: .semibold))
                                .foregroundStyle(shown ? Ink.primary : Ink.secondary)
                                .multilineTextAlignment(.center)
                        }
                        .frame(width: 100, height: 82)
                        .background(
                            RoundedRectangle(cornerRadius: 10).fill(shown ? Ink.item : .clear))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                Spacer(minLength: 0)
                HStack {
                    IconButton(symbol: "arrow.left", label: "Back to the Space") { model.closeLibrary() }
                    Spacer()
                }
                .padding(.bottom, 10)
            }
            .padding(.horizontal, 6)
            .frame(width: 113)
            .background(Color.ink((0x000000, 0.03), (0xFFFFFF, 0.035)))
            .overlay(alignment: .trailing) {
                Rectangle().fill(Color.ink((0x000000, 0.08), (0xFFFFFF, 0.06))).frame(width: 1)
            }
            VStack(alignment: .leading, spacing: 8) {
                Spacer().frame(height: 36)
                if !model.section.wide {
                    TextField("Search \(model.section.label)…", text: model.queryBinding)
                        .textFieldStyle(.roundedBorder)
                }
                if model.section == .spaces {
                    ManageSpacesView(model: model)
                } else {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 2) {
                            switch model.section {
                            case .spaces: EmptyView()
                            case .closed: ClosedTabs(model: model)
                            case .archive: ArchivedTabs(model: model)
                            case .history: HistoryList(model: model)
                            case .media: MediaGrid(model: model)
                            case .downloads: DownloadsList(model: model)
                            case .extensions: ExtensionsList(model: model)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .padding(.leading, model.section == .spaces ? 0 : 10)
            .padding(.trailing, 10)
        }
    }
}

/// A row opening something: an icon, a title and a detail at its right.
struct LibraryRow: View {
    let favicon: String?
    let title: String
    let detail: String
    var help = ""
    let action: () -> Void
    private let hovered = State(initialValue: false)

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Favicon(url: favicon, size: 16)
                Text(title).lineLimit(1)
                Spacer(minLength: 6)
                Text(detail).font(.system(size: 11)).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 8)
            .frame(height: 32)
            .background(
                RoundedRectangle(cornerRadius: 7).fill(.primary.opacity(hovered.wrappedValue ? 0.08 : 0)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered.wrappedValue = $0 }
        .help(help)
    }
}

struct Empty: View {
    let text: String

    var body: some View {
        Text(text).foregroundStyle(.secondary).padding(.top, 20).frame(maxWidth: .infinity)
    }
}

/// The tabs closed since launch, the last closed first, which reopen in the displayed Space.
struct ClosedTabs: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let closed = Array((model.state?.closed ?? []).enumerated())
            .filter { matches(model.query, $0.element.title, $0.element.url) }
        if model.state?.closed.isEmpty ?? true {
            Empty(text: "No tab closed yet")
        } else if closed.isEmpty {
            Empty(text: "No closed tab matches")
        }
        ForEach(closed, id: \.offset) { rank, tab in
            LibraryRow(
                favicon: tab.favicon, title: tab.title.isEmpty ? tab.url : tab.title,
                detail: ago(tab.closedAt), help: tab.url
            ) {
                model.run("reopen/\(rank)")
                model.closeLibrary()
            }
        }
    }
}

/// The tabs archived, the most recent first: a click opens one again in its Space.
struct ArchivedTabs: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let found = Array(model.archive.enumerated())
            .filter { matches(model.query, $0.element.title, $0.element.url) }
        if model.archive.isEmpty {
            Empty(text: "No archived tabs yet")
        } else if found.isEmpty {
            Empty(text: "No archived tab matches")
        }
        ForEach(found, id: \.offset) { rank, tab in
            LibraryRow(
                favicon: tab.favicon, title: tab.title.isEmpty ? tab.url : tab.title,
                detail: ago(tab.archivedAt), help: tab.space.map { "\(tab.url)\nin \($0)" } ?? tab.url
            ) {
                model.run("archive/restore/\(rank)")
                model.closeLibrary()
            }
        }
    }
}

/// The displayed Space's profile history, the latest visit first, which the app searches.
struct HistoryList: View {
    @ObservedObject var model: AppModel

    var body: some View {
        if model.history.isEmpty {
            Empty(text: model.query.isEmpty ? "No history yet" : "No page matches")
        }
        ForEach(Array(model.history.enumerated()), id: \.offset) { _, visit in
            LibraryRow(
                favicon: nil, title: visit.title.isEmpty ? visit.url : visit.title,
                detail: ago(visit.lastVisit), help: visit.url
            ) {
                model.run("open/\(percentEncoded(visit.url))")
                model.closeLibrary()
            }
            .rightClick { model.run("history/remove/\(percentEncoded(visit.url))") }
        }
    }
}

/// The images and videos downloaded and still there, as a grid of their previews.
struct MediaGrid: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let media = model.downloads.reversed().filter { download in
            let ext = (download.name as NSString).pathExtension.lowercased()
            return download.state == "complete" && !download.missing
                && ["png", "jpg", "jpeg", "gif", "webp", "avif", "mp4", "m4v", "webm", "mov"].contains(ext)
                && matches(model.query, download.name)
        }
        if media.isEmpty { Empty(text: "No images or videos downloaded") }
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 90), spacing: 6)], spacing: 6) {
            ForEach(media) { download in
                Button {
                    model.run("download/open/\(download.id)")
                } label: {
                    Group {
                        if let image = uiImage("arbalete://ui/media/\(download.id)") {
                            Image(nsImage: image).resizable().scaledToFill()
                        } else {
                            Image(systemName: "film").font(.system(size: 24))
                        }
                    }
                    .frame(width: 90, height: 90)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                .buttonStyle(.plain)
                .help(download.name)
            }
        }
    }
}

/// Every download, the most recent first.
struct DownloadsList: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let found = model.downloads.reversed().filter { matches(model.query, $0.name, $0.url) }
        if found.isEmpty { Empty(text: model.downloads.isEmpty ? "No downloads yet" : "No download matches") }
        ForEach(found) { download in
            DownloadRow(model: model, download: download)
        }
    }
}

/// A finished download opens on a click, and shows in the Finder from its button; the × cancels
/// one in progress, or takes another off the list.
struct DownloadRow: View {
    @ObservedObject var model: AppModel
    let download: Download

    var body: some View {
        let running = download.state == "inProgress"
        HStack(spacing: 8) {
            Image(systemName: "doc").frame(width: 16)
            VStack(alignment: .leading, spacing: 1) {
                Text(download.name).lineLimit(1)
                Text(downloadDetail(download)).font(.system(size: 11)).foregroundStyle(.secondary)
            }
            Spacer(minLength: 4)
            if download.state == "complete" && !download.missing {
                IconButton(symbol: "magnifyingglass", label: "Show in Finder", size: 22) {
                    model.run("download/show/\(download.id)")
                }
            }
            IconButton(symbol: "xmark", label: running ? "Cancel" : "Remove", size: 22) {
                model.run("download/\(running ? "cancel" : "remove")/\(download.id)")
            }
        }
        .padding(.horizontal, 8)
        .frame(height: 40)
        .contentShape(Rectangle())
        .onTapGesture { if download.state == "complete" { model.run("download/open/\(download.id)") } }
        .help(download.url)
    }
}

/// The Spaces side by side, like Arc's: a column each, in sidebar order, in its theme's colors,
/// with its name, its tabs, a brush for its theme, a handle to move it and its menu; a click
/// shows it. The "+" after them creates a Space.
struct ManageSpacesView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 24) {
                ForEach(Array((model.state?.spaces ?? []).enumerated()), id: \.element.id) {
                    index, space in
                    SpaceColumn(model: model, space: space, index: index)
                }
                Button {
                    model.newSpace()
                } label: {
                    Image(systemName: "plus").font(.system(size: 12, weight: .bold))
                        .foregroundStyle(.white)
                        .frame(width: 22, height: 22)
                        .background(Circle().fill(color("#333ce8")))
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .help("New Space")
                .padding(.horizontal, 12)
            }
            .padding(.horizontal, 22)
            .padding(.bottom, 16)
        }
        .scrollIndicators(.never)
    }
}

/// A Space's column among the Spaces, as Arc's, measured: 240 points wide; a Space with colors
/// shows them at three quarters over a light gray, its titles dark, one without colors nothing
/// under its titles; the name 33 points down, the tabs in 40-point rows from 70 points down,
/// under the line that clears today's tabs of the displayed Space.
struct SpaceColumn: View {
    @ObservedObject var model: AppModel
    let space: Space
    let index: Int

    private var colored: Bool { !space.theme.colors.isEmpty }
    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: 9) }

    var body: some View {
        let page = model.state?.pages.first { $0.space == space.id }
        let tabs = page.map { ($0.pinned + $0.today).flatMap(\.tabs) } ?? []
        let ink: Color = colored ? .black.opacity(0.4) : Ink.primary
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 9) {
                if let emoji = space.emoji { SpaceGlyph(icon: emoji, size: 16) }
                Text(space.title).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                Spacer()
                IconButton(symbol: "paintbrush.pointed.fill", label: "Edit Theme Color", size: 24) {
                    model.run("theme/show/\(space.id)")
                }
                .opacity(0.75)
            }
            .padding(.leading, 17)
            .padding(.trailing, 14)
            .frame(height: 24)
            .padding(.top, 21)
            if !tabs.isEmpty {
                HStack(spacing: 6) {
                    Rectangle().fill(ink.opacity(0.15)).frame(height: 1)
                    if space.id == model.state?.space {
                        Button("↓ Clear") { model.run("today/clear") }
                            .buttonStyle(.plain)
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(Ink.secondary)
                    }
                }
                .padding(.horizontal, 17)
                .frame(height: 10)
                .padding(.top, 10)
                .padding(.bottom, 5)
            }
            ForEach(tabs) { tab in
                HStack(spacing: 10) {
                    Favicon(url: tab.favicon, size: 16)
                    Text(tab.label).font(.system(size: 14)).lineLimit(1)
                }
                .padding(.horizontal, 17)
                .frame(height: 40)
            }
            Spacer(minLength: 0)
            HStack {
                Image(systemName: "arrow.up.and.down.and.arrow.left.and.right")
                    .font(.system(size: 12, weight: .medium))
                    .frame(width: 24, height: 24)
                    .help("Drag to Move")
                Spacer()
                IconButton(symbol: "ellipsis", label: "Space options", size: 24) {
                    model.run("menu/space/\(space.id)")
                }
            }
            .padding(.horizontal, 10)
            .padding(.bottom, 10)
        }
        .foregroundStyle(ink)
        .frame(width: 240)
        .frame(maxHeight: .infinity)
        .background {
            if colored {
                ZStack {
                    Color(white: 0.765)
                    Tint.colors(space.theme.colors).opacity(0.75)
                }
                .clipShape(shape)
            }
        }
        .contentShape(shape)
        .onTapGesture {
            model.run("space/\(space.id)")
            model.closeLibrary()
        }
        .rightClick { model.run("menu/space/\(space.id)") }
        // Dragged onto another, it takes that one's place.
        .draggable("space:\(space.id)")
        .dropDestination(for: String.self) { ids, _ in
            guard let id = ids.first?.split(separator: ":").last.flatMap({ Int($0) }),
                ids.first?.hasPrefix("space:") == true, id != space.id
            else { return false }
            model.run("space/move/\(id)/\(index)")
            return true
        }
    }
}

/// The displayed Space's profile's extensions: each with its switch, pin, options and removal;
/// the Chrome Web Store to add more, a folder to load one from, and updates.
struct ExtensionsList: View {
    @ObservedObject var model: AppModel

    var body: some View {
        HStack {
            Button("Add Extensions") { model.run("site/store") }
            Button("Load Unpacked…") { model.run("extension/load-unpacked") }
            Button("Update") { model.run("extension/update") }
        }
        .padding(.bottom, 8)
        let installed = model.state?.installed ?? []
        if installed.isEmpty { Empty(text: "No extensions yet") }
        ForEach(installed) { ext in
            // Too narrow for the controls beside the text, they go under it.
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 10) {
                    ExtensionSummary(ext: ext)
                    Spacer(minLength: 0)
                    ExtensionControls(model: model, ext: ext)
                }
                VStack(alignment: .leading, spacing: 8) {
                    ExtensionSummary(ext: ext)
                    ExtensionControls(model: model, ext: ext)
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 10).fill(.primary.opacity(0.06)))
        }
    }
}

/// An installed extension's icon, name, description and version.
private struct ExtensionSummary: View {
    let ext: InstalledExtension

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Favicon(url: ext.icon, size: 24)
            VStack(alignment: .leading, spacing: 2) {
                Text(ext.name).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                Text(ext.description).font(.system(size: 11)).foregroundStyle(.secondary).lineLimit(2)
                Text(ext.version).font(.system(size: 10)).foregroundStyle(.tertiary)
            }
            .frame(minWidth: 140, alignment: .leading)
        }
    }
}

/// An installed extension's pin, options, removal and switch.
private struct ExtensionControls: View {
    @ObservedObject var model: AppModel
    let ext: InstalledExtension

    var body: some View {
        HStack(spacing: 10) {
            IconButton(symbol: ext.pinned ? "pin.fill" : "pin", label: ext.pinned ? "Unpin" : "Pin", size: 24) {
                model.run("extension/\(ext.pinned ? "unpin" : "pin")/\(ext.id)")
            }
            if ext.options {
                IconButton(symbol: "gearshape", label: "Options", size: 24) {
                    model.run("extension/options/\(ext.id)")
                }
            }
            IconButton(symbol: "trash", label: "Remove", size: 24) {
                model.run("extension/remove/\(ext.id)")
            }
            Toggle("", isOn: Binding(
                get: { ext.enabled },
                set: { _ in model.run("extension/\(ext.enabled ? "off" : "on")/\(ext.id)") }))
                .toggleStyle(.switch)
                .labelsHidden()
                .controlSize(.small)
        }
        .fixedSize()
    }
}

/// The downloads in progress and the ones of the last day at the bottom of the sidebar, three at
/// most; the library lists them all.
struct RecentDownloads: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let now = Date().timeIntervalSince1970 * 1000
        let recent = model.downloads.reversed()
            .filter { $0.state == "inProgress" || now - $0.startedAt < 24 * 3600 * 1000 }
            .prefix(3)
        VStack(spacing: 0) {
            ForEach(Array(recent)) { download in
                DownloadRow(model: model, download: download)
            }
        }
    }
}

/// The list of profiles, from the New menu: to create one, rename one or pick its color. It
/// opens by itself when the app asks to rename the profile it has just created.
struct ProfilesView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(model.state?.profiles ?? []) { profile in
                ProfileRow(model: model, profile: profile)
            }
            Button("+ New profile") { model.run("profile/new") }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
        }
        .padding(12)
        .frame(width: 260)
    }
}

struct ProfileRow: View {
    @ObservedObject var model: AppModel
    let profile: Profile
    private let name = State(initialValue: "")

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            TextField("Profile name", text: name.projectedValue)
                .textFieldStyle(.roundedBorder)
                .onAppear { name.wrappedValue = profile.name }
                .onSubmit {
                    let value = name.wrappedValue.trimmingCharacters(in: .whitespaces)
                    if !value.isEmpty && value != profile.name {
                        model.run("profile/rename/\(profile.id)/\(percentEncoded(value))")
                    }
                }
            HStack(spacing: 6) {
                ForEach(
                    ["purple", "blue", "green", "yellow", "orange", "red", "pink", "gray"], id: \.self
                ) { option in
                    Button {
                        model.run("profile/color/\(profile.id)/\(option)")
                    } label: {
                        Circle().fill(color(profileColors[option] ?? "#8b8d98"))
                            .frame(width: 16, height: 16)
                            .overlay(Circle().strokeBorder(.primary, lineWidth: profile.color == option ? 2 : 0))
                    }
                    .buttonStyle(.plain)
                    .help(option.capitalized)
                }
            }
        }
    }
}
