// The library (ui/src/Library.tsx), which the sidebar shows instead of a Space's page: a rail of
// sections, the one picked last kept until the app quits, and the picked section under a field
// searching it, but for the Spaces, which fill the window.

import AppKit
import SwiftUI

enum LibrarySection: String, CaseIterable, Identifiable {
    // Arc's, in its order: its Easels and Boosts have no counterpart here.
    case media, downloads, spaces, archive

    var id: String { rawValue }

    var label: String {
        switch self {
        case .media: "Media"
        case .downloads: "Downloads"
        case .spaces: "Spaces"
        case .archive: "Archived Tabs"
        }
    }

    var symbol: String {
        switch self {
        case .media: "photo.on.rectangle.angled"
        case .downloads: "arrow.down.circle"
        case .spaces: "rectangle.on.rectangle.angled"
        case .archive: "archivebox"
        }
    }

    /// The symbol of the section shown, filled, like Arc's.
    var filledSymbol: String {
        switch self {
        case .media: "photo.fill.on.rectangle.fill"
        case .downloads: "arrow.down.circle.fill"
        case .spaces: "rectangle.fill.on.rectangle.angled.fill"
        case .archive: "archivebox.fill"
        }
    }

    /// What the section's search field says it searches, like Arc's "Search Archive...".
    var searchName: String { self == .archive ? "Archive" : label }

    /// The Spaces fill the window, the page hidden; the other sections rest beside it.
    var wide: Bool { self == .spaces }
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
            // Like Arc's, measured: the sections in a rail 111 points wide, a little lighter than
            // the window, centered on it, each a large icon over its name, 100 points apart; the
            // one shown filled and tinted.
            VStack(spacing: 21) {
                ForEach(LibrarySection.allCases) { section in
                    let shown = model.section == section
                    Button {
                        model.pick(section)
                    } label: {
                        VStack(spacing: 9) {
                            Image(systemName: shown ? section.filledSymbol : section.symbol)
                                .font(.system(size: 22, weight: .medium))
                                .foregroundStyle(shown ? model.accent : Ink.secondary)
                                .frame(height: 26)
                            Text(section.label).font(.system(size: 11, weight: .bold))
                                .foregroundStyle(shown ? Ink.primary : Ink.secondary)
                                .multilineTextAlignment(.center)
                        }
                        .frame(width: 97, height: 79)
                        .background(
                            RoundedRectangle(cornerRadius: 10).fill(shown ? Ink.item : .clear))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .frame(maxHeight: .infinity)
            .overlay(alignment: .bottomLeading) {
                IconButton(symbol: "arrow.left", label: "Back to the Space") { model.closeLibrary() }
                    .padding(.leading, 9)
                    .padding(.bottom, 10)
            }
            .frame(width: 111)
            .background(Color.ink((0x000000, 0.03), (0xFFFFFF, 0.035)))
            .overlay(alignment: .trailing) {
                Rectangle().fill(Color.ink((0x000000, 0.08), (0xFFFFFF, 0.06))).frame(width: 1)
                    .offset(x: 1)
            }
            .padding(.trailing, 1)
            VStack(alignment: .leading, spacing: 10) {
                if !model.section.wide {
                    LibrarySearch(model: model)
                }
                if model.section == .spaces {
                    ManageSpacesView(model: model)
                } else {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            switch model.section {
                            case .spaces: EmptyView()
                            case .archive: ArchivedTabs(model: model)
                            case .media: MediaGrid(model: model)
                            case .downloads: DownloadsList(model: model)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .scrollIndicators(.never)
                }
            }
            // Every section's top on the page's card's, 10 points from the window's.
            .padding(.top, 10)
            .padding(.leading, model.section == .spaces ? 0 : 7)
            .padding(.trailing, 8)
        }
    }
}

/// The library's search field, like Arc's, measured: a 36-point pill, a magnifier, and "Search
/// <section>...".
struct LibrarySearch: View {
    @ObservedObject var model: AppModel

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass").font(.system(size: 13, weight: .medium))
                .foregroundStyle(Ink.secondary)
                .frame(width: 20)
            TextField("Search \(model.section.searchName)...", text: model.queryBinding)
                .textFieldStyle(.plain)
                .font(.system(size: 14))
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        .background(RoundedRectangle(cornerRadius: 10).fill(Ink.item))
    }
}

/// Items grouped under the day they belong to, like Arc's library: "Today", "Yesterday", then how
/// long ago, as the system says it.
struct DayGroups<Item, Row: View>: View {
    let items: [Item]
    let time: (Item) -> Double
    @ViewBuilder let row: (Item) -> Row

    var body: some View {
        let groups = items.reduce(into: [(String, [Item])]()) { groups, item in
            let label = dayLabel(time(item))
            if groups.last?.0 == label { groups[groups.count - 1].1.append(item) } else {
                groups.append((label, [item]))
            }
        }
        ForEach(Array(groups.enumerated()), id: \.offset) { _, group in
            Text(group.0).font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Ink.secondary)
                .padding(.leading, 3)
                .padding(.top, 12)
                .padding(.bottom, 4)
            ForEach(Array(group.1.enumerated()), id: \.offset) { _, item in row(item) }
        }
    }
}

/// The day a time, in milliseconds since the epoch, belongs to, in words.
func dayLabel(_ time: Double) -> String {
    let date = Date(timeIntervalSince1970: time / 1000)
    let calendar = Calendar.current
    if calendar.isDateInToday(date) { return "Today" }
    if calendar.isDateInYesterday(date) { return "Yesterday" }
    let days = calendar.dateComponents(
        [.day], from: calendar.startOfDay(for: date), to: calendar.startOfDay(for: Date())
    ).day ?? 0
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .full
    return formatter.localizedString(
        from: days < 7 ? DateComponents(day: -days) : DateComponents(weekOfYear: -(days / 7)))
}

/// An address as the library shows it under a title: without scheme nor "www.".
func shortAddress(_ url: String) -> String {
    let address = url.replacingOccurrences(
        of: "^[a-z][a-z0-9+.-]*://(www\\.)?", with: "", options: [.regularExpression, .caseInsensitive])
    return address.hasSuffix("/") ? String(address.dropLast()) : address
}

/// A row opening something, like Arc's library's, measured: 56 points tall, an icon, a title
/// over its address.
struct LibraryRow: View {
    let favicon: String?
    let title: String
    let detail: String
    var help = ""
    let action: () -> Void
    private let hovered = State(initialValue: false)

    var body: some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Favicon(url: favicon, size: 16)
                VStack(alignment: .leading, spacing: 3) {
                    Text(title).font(.system(size: 14)).foregroundStyle(Ink.primary).lineLimit(1)
                    Text(detail).font(.system(size: 12)).foregroundStyle(Ink.secondary).lineLimit(1)
                }
                Spacer(minLength: 6)
            }
            .padding(.horizontal, 14)
            .frame(height: 56)
            .background(
                RoundedRectangle(cornerRadius: 10).fill(Ink.item.opacity(hovered.wrappedValue ? 1 : 0)))
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
        DayGroups(items: found, time: { $0.element.archivedAt }) { item in
            let (rank, tab) = item
            LibraryRow(
                favicon: tab.favicon, title: tab.title.isEmpty ? tab.url : tab.title,
                detail: shortAddress(tab.url), help: tab.space.map { "\(tab.url)\nin \($0)" } ?? tab.url
            ) {
                model.run("archive/restore/\(rank)")
                model.closeLibrary()
            }
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
        if media.isEmpty {
            // Like Arc's, a picture's outline over what found nothing, in the middle.
            VStack(spacing: 18) {
                Image(systemName: "photo.on.rectangle").font(.system(size: 40))
                    .foregroundStyle(Ink.secondary.opacity(0.6))
                Text(
                    model.query.isEmpty
                        ? "No images or videos downloaded" : "No results found for “\(model.query)”"
                )
                .font(.system(size: 13.5))
                .foregroundStyle(Ink.secondary)
            }
            .frame(maxWidth: .infinity)
            .padding(.top, 180)
        }
        // Like Arc's, measured: two columns of 132-point tiles, 16 points apart, 8 between rows.
        LazyVGrid(
            columns: Array(repeating: GridItem(.fixed(132), spacing: 16), count: 2), spacing: 8
        ) {
            ForEach(media) { download in
                Button {
                    model.run("download/open/\(download.id)")
                } label: {
                    MediaTile(
                        image: uiImage("arbalete://ui/media/\(download.id)"),
                        video: ["mp4", "m4v", "webm", "mov"].contains(
                            (download.name as NSString).pathExtension.lowercased()))
                }
                .buttonStyle(.plain)
                .help(download.name)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 4)
    }
}

/// A picture or a video downloaded, like Arc's: fitted in a tile, edged in white, a video with a
/// play glyph.
struct MediaTile: View {
    let image: NSImage?
    let video: Bool

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 10).fill(Ink.item)
            if let image {
                Image(nsImage: image).resizable().aspectRatio(contentMode: .fit)
                    .padding(4)
                    .background(Color.white)
                    .frame(maxWidth: 110, maxHeight: 110)
            } else {
                Image(systemName: video ? "film" : "photo").font(.system(size: 24))
                    .foregroundStyle(Ink.secondary)
            }
            if video {
                Image(systemName: "play.fill").font(.system(size: 22)).foregroundStyle(.white)
                    .shadow(color: .black.opacity(0.4), radius: 3)
            }
        }
        .frame(width: 132, height: 132)
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
            .padding(.horizontal, 24)
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
