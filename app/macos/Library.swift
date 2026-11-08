// The library, which the sidebar shows instead of a Space's page: a rail of
// sections, the one picked last kept until the app quits, and the picked section under a field
// searching it, but for the Spaces, which fill the window.

import AppKit
import QuickLookThumbnailing
import SwiftUI
import UniformTypeIdentifiers

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
    /// Closed by the user rather than archived for being left aside.
    var manual: Bool?
}

struct Download: Decodable, Identifiable {
    var id: Int
    var name: String
    var path: String
    var url: String
    var profile: Int?
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
            // the window, centered on it, each a large icon over its name, 100 points apart, in
            // black inks the theme tints; the one shown darker, its icon filled with white.
            VStack(spacing: 21) {
                ForEach(LibrarySection.allCases) { section in
                    let shown = model.section == section
                    Button {
                        model.pick(section)
                    } label: {
                        VStack(spacing: 6) {
                            ZStack {
                                if shown {
                                    Image(systemName: section.filledSymbol)
                                        .foregroundStyle(Color.ink((0xFFFFFF, 0.7), (0xFFFFFF, 0.25)))
                                }
                                Image(systemName: section.symbol)
                                    .foregroundStyle(
                                        shown
                                            ? Color.ink((0x000000, 0.75), (0xFFFFFF, 0.85))
                                            : Color.ink((0x000000, 0.5), (0xFFFFFF, 0.45)))
                            }
                            .font(.system(size: 22, weight: .medium))
                            .frame(height: 26)
                            Text(section.label).font(.system(size: 11, weight: .bold))
                                .foregroundStyle(
                                    shown
                                        ? Color.ink((0x000000, 0.8), (0xFFFFFF, 0.85))
                                        : Color.ink((0x000000, 0.65), (0xFFFFFF, 0.55)))
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
                    LibraryHeader(model: model)
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
            // Every section's top on the page's card's, 10 points from the window's; the Spaces
            // keep their own margins.
            .padding(.top, model.section == .spaces ? 0 : 10)
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
            // Like Arc's, a filter on makes the field search what it leaves.
            TextField(
                model.filters.isEmpty ? "Search \(model.section.searchName)..." : "Search...",
                text: model.queryBinding)
                .textFieldStyle(.plain)
                .font(.system(size: 14))
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        .background(RoundedRectangle(cornerRadius: 10).fill(Ink.item))
    }
}

/// A chip of the library's filter: a profile for the downloads, a Space for the tabs archived.
enum FilterChip: Hashable {
    case profile(Int)
    case space(String)
    /// How a tab was archived: closed by the user, or left aside.
    case closed(manually: Bool)
}

extension LibrarySection {
    /// Like Arc's, the downloads and the tabs archived can be filtered.
    var filterable: Bool { self == .downloads || self == .archive }
}

extension AppModel {
    /// The chips the filter offers on the section shown, with their names, in groups.
    var filterGroups: [(title: String, chips: [(FilterChip, String)])] {
        switch section {
        case .downloads:
            let profiles = (state?.profiles ?? []).map { (FilterChip.profile($0.id), $0.name) }
            return [("Profiles", profiles)]
        case .archive:
            var names: [String] = []
            for tab in archive { if let space = tab.space, !names.contains(space) { names.append(space) } }
            return [
                (
                    "How was the tab closed?",
                    [
                        (FilterChip.closed(manually: false), "Auto-Closed"),
                        (FilterChip.closed(manually: true), "Manually Closed"),
                    ]
                ),
                ("Where was the tab?", names.map { (FilterChip.space($0), $0) }),
            ]
        default:
            return []
        }
    }

    /// The name of a chip picked.
    func filterName(_ chip: FilterChip) -> String {
        filterGroups.flatMap(\.chips).first { $0.0 == chip }?.1 ?? ""
    }

    /// Whether a download passes the filter: with no profile picked, every one does. Those
    /// saved before downloads kept their profile count as the first profile's.
    func passes(_ download: Download) -> Bool {
        let picked = filters.compactMap { if case .profile(let id) = $0 { id } else { nil } }
        let profile = download.profile ?? state?.profiles.first?.id
        return picked.isEmpty || profile.map(picked.contains) == true
    }

    /// Whether an archived tab passes the filter: each group picked narrows it, any chip of a
    /// group does.
    func passes(_ tab: ArchivedTab) -> Bool {
        let spaces = filters.compactMap { if case .space(let name) = $0 { name } else { nil } }
        let closed = filters.compactMap { if case .closed(let manually) = $0 { manually } else { nil } }
        return (spaces.isEmpty || tab.space.map(spaces.contains) == true)
            && (closed.isEmpty || closed.contains(tab.manual ?? false))
    }
}

/// The top of a narrow section, like Arc's: its search field, a Filter button beside it for the
/// downloads and the tabs archived, which turns the top into the filter's panel, and the chips
/// picked under it as tokens.
struct LibraryHeader: View {
    @ObservedObject var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if model.filterShown {
                FilterPanel(model: model).transition(.opacity)
            } else {
                HStack(spacing: 3) {
                    LibrarySearch(model: model)
                    if model.section.filterable {
                        LibraryButton(label: "Filter", symbol: "line.3.horizontal.decrease.circle") {
                            model.filterShown = true
                        }
                    }
                }
                .transition(.opacity)
                if !model.filters.isEmpty {
                    FlowLayout(spacing: 6) {
                        ForEach(Array(model.filters), id: \.self) { chip in
                            Button {
                                model.filters.remove(chip)
                            } label: {
                                HStack(spacing: 6) {
                                    Image(systemName: "xmark.circle.fill")
                                    Text(model.filterName(chip))
                                }
                                .font(.system(size: 14))
                                .foregroundStyle(Ink.secondary)
                                .padding(.horizontal, 10)
                                .frame(height: 32)
                                .background(RoundedRectangle(cornerRadius: 10).fill(Ink.item))
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
            }
        }
        // Measured on Arc's: a quarter of a second to cross-fade.
        .animation(.easeInOut(duration: 0.25), value: model.filterShown)
    }
}

/// A button of the library's top, like Arc's: a light pill as tall as the search field.
struct LibraryButton: View {
    let label: String
    var symbol: String? = nil
    let action: () -> Void
    private let hovered = State(initialValue: false)

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if let symbol { Image(systemName: symbol) }
                Text(label)
            }
            .font(.system(size: 14))
            .foregroundStyle(Ink.secondary)
            .padding(.horizontal, 10)
            .frame(height: 36)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(Ink.item.opacity(hovered.wrappedValue ? 1.4 : 0.8)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered.wrappedValue = $0 }
    }
}

/// The filter's panel, like Arc's: its title and Done, then each group's chips, light when
/// picked.
struct FilterPanel: View {
    @ObservedObject var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(model.section == .downloads ? "Filter Downloads..." : "Filter Archived Tabs")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Ink.secondary)
                    .padding(.leading, 5)
                Spacer()
                LibraryButton(label: "Done") { model.filterShown = false }
            }
            ForEach(model.filterGroups, id: \.title) { group in
                VStack(alignment: .leading, spacing: 8) {
                    Text(group.title).font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(Ink.secondary)
                        .padding(.leading, 3)
                    FlowLayout(spacing: 7) {
                        ForEach(group.chips, id: \.0) { chip, name in
                            let on = model.filters.contains(chip)
                            Button {
                                if on { model.filters.remove(chip) } else { model.filters.insert(chip) }
                            } label: {
                                Text(name).font(.system(size: 14))
                                    // Like Arc's: dark grey picked in light themes, light grey
                                    // in dark ones.
                                    .foregroundStyle(on ? Color.ink((0xFFFFFF, 1), (0x19191B, 1)) : Ink.secondary)
                                    .padding(.horizontal, 11)
                                    .frame(height: 32)
                                    .background(
                                        RoundedRectangle(cornerRadius: 10)
                                            .fill(on ? Color.ink((0x585760, 1), (0x9A99A2, 1)) : Ink.item))
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
            }
            Rectangle().fill(Ink.divider).frame(height: 1)
        }
    }
}

/// Lays its views out in rows, wrapping onto the next one when a row is full.
struct FlowLayout: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        let rows = arrange(subviews, width: width)
        let height = rows.last.map { $0.y + $0.height } ?? 0
        return CGSize(width: proposal.width ?? rows.map(\.width).max() ?? 0, height: height)
    }

    func placeSubviews(
        in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()
    ) {
        for row in arrange(subviews, width: bounds.width) {
            var x = bounds.minX
            for index in row.items {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: bounds.minY + row.y), proposal: .init(size))
                x += size.width + spacing
            }
        }
    }

    private func arrange(_ subviews: Subviews, width: CGFloat)
        -> [(items: [Int], y: CGFloat, width: CGFloat, height: CGFloat)]
    {
        var rows: [(items: [Int], y: CGFloat, width: CGFloat, height: CGFloat)] = []
        var x: CGFloat = 0
        for (index, subview) in subviews.enumerated() {
            let size = subview.sizeThatFits(.unspecified)
            if rows.isEmpty || (x > 0 && x + size.width > width) {
                let y = rows.last.map { $0.y + $0.height + spacing } ?? 0
                rows.append(([], y, 0, 0))
                x = 0
            }
            rows[rows.count - 1].items.append(index)
            rows[rows.count - 1].width = x + size.width
            rows[rows.count - 1].height = max(rows[rows.count - 1].height, size.height)
            x += size.width + spacing
        }
        return rows
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
            .filter { model.passes($0.element) && matches(model.query, $0.element.title, $0.element.url) }
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
                model.run("restore-archived", rank)
                model.closeLibrary()
            }
        }
    }
}

/// The images and videos downloaded and still there, as a grid of their previews.
struct MediaGrid: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let media = libraryMedia(model).filter { matches(model.query, $0.name) }
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
                // Like Arc's, in the app's own viewer.
                Button {
                    model.viewing = download.id
                } label: {
                    FileThumbnail(path: mediaKind(download) == .image ? download.path : nil) {
                        MediaTile(image: $0, video: mediaKind(download) == .video)
                    }
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

/// Every download, the most recent first, under the day it started, like Arc's.
struct DownloadsList: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let found = model.downloads.reversed().filter {
            model.passes($0) && matches(model.query, $0.name, $0.url)
        }
        if found.isEmpty {
            // Like Arc's, an arrow's outline over what to come back for, in the middle.
            VStack(spacing: 14) {
                Image(systemName: "arrow.down.circle").font(.system(size: 44, weight: .light))
                    .foregroundStyle(Ink.secondary.opacity(0.6))
                Text("Nothing here yet!").font(.system(size: 16, weight: .bold))
                    .foregroundStyle(Ink.primary)
                Text("Come back after you've downloaded some files.").font(.system(size: 13))
                    .foregroundStyle(Ink.secondary)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 30)
            .padding(.top, 180)
        }
        DayGroups(items: Array(found), time: { $0.startedAt }) { download in
            DownloadRow(model: model, download: download)
        }
    }
}

/// A download, like Arc's: its picture or its file's icon, its name over what it is and where it
/// came from, and its menu on hover. A click shows a picture or a video in the media viewer, and
/// opens another file.
struct DownloadRow: View {
    @ObservedObject var model: AppModel
    let download: Download
    private let hovered = State(initialValue: false)

    var body: some View {
        let running = download.state == "inProgress"
        let there = download.state == "complete" && !download.missing
        HStack(spacing: 14) {
            DownloadIcon(download: download, there: there)
            VStack(alignment: .leading, spacing: 3) {
                Text(download.name).font(.system(size: 14)).foregroundStyle(Ink.primary).lineLimit(1)
                Text(there ? downloadKind(download) : downloadDetail(download))
                    .font(.system(size: 12)).foregroundStyle(Ink.secondary).lineLimit(1)
            }
            Spacer(minLength: 6)
            if hovered.wrappedValue || running {
                Menu {
                    if there {
                        Button("Show in Finder") { model.run("download", "show", download.id) }
                    }
                    if running {
                        Button("Cancel") { model.run("download", "cancel", download.id) }
                    } else {
                        Button("Remove from List") { model.run("download", "remove", download.id) }
                    }
                } label: {
                    Image(systemName: "ellipsis").font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Ink.secondary)
                        .frame(width: 26, height: 26)
                        .contentShape(Rectangle())
                }
                .menuStyle(.button)
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .fixedSize()
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 56)
        .background(
            RoundedRectangle(cornerRadius: 10).fill(Ink.item.opacity(hovered.wrappedValue ? 1 : 0)))
        .contentShape(Rectangle())
        .onHover { hovered.wrappedValue = $0 }
        .onTapGesture {
            guard there else { return }
            if mediaKind(download) != nil {
                model.viewing = download.id
            } else {
                model.run("download", "open", download.id)
            }
        }
        .help(download.url)
    }
}

/// A picture downloaded in small, or its file's icon, as the Finder shows it.
struct DownloadIcon: View {
    let download: Download
    let there: Bool

    var body: some View {
        FileThumbnail(path: there && mediaKind(download) == .image ? download.path : nil) {
            if let image = $0 {
                Image(nsImage: image).resizable().aspectRatio(contentMode: .fill)
                    .frame(width: 22, height: 26).clipped()
            } else {
                Image(nsImage: NSWorkspace.shared.icon(forFile: download.path)).resizable()
                    .frame(width: 28, height: 28)
            }
        }
        .frame(width: 28, height: 28)
    }
}

/// A file's picture in small, at most a media tile's, or nil until it is made or without one.
/// Quick Look makes it out of the process, so the browser never decodes a file a page chose; the
/// full image waits for the media viewer.
struct FileThumbnail<Content: View>: View {
    let path: String?
    @ViewBuilder let content: (NSImage?) -> Content
    private let _loaded = State<NSImage?>(initialValue: nil)
    @Environment(\.displayScale) private var scale

    var body: some View {
        content(path.flatMap { thumbnails.object(forKey: $0 as NSString) } ?? _loaded.wrappedValue)
            .task(id: path) {
                _loaded.wrappedValue = nil
                guard let path, thumbnails.object(forKey: path as NSString) == nil else { return }
                let request = QLThumbnailGenerator.Request(
                    fileAt: URL(fileURLWithPath: path), size: CGSize(width: 132, height: 132),
                    scale: scale, representationTypes: .thumbnail)
                guard
                    let made = try? await QLThumbnailGenerator.shared
                        .generateBestRepresentation(for: request)
                else { return }
                let image = NSImage(cgImage: made.cgImage, size: .zero)
                thumbnails.setObject(image, forKey: path as NSString)
                _loaded.wrappedValue = image
            }
    }
}

/// The thumbnails made, the most recent kept.
@MainActor private let thumbnails = {
    let cache = NSCache<NSString, NSImage>()
    cache.countLimit = 200
    return cache
}()

/// What a download is and where it came from, like Arc's: "Image from google.com".
func downloadKind(_ download: Download) -> String {
    let kind: String
    switch mediaKind(download) {
    case .image: kind = "Image"
    case .video: kind = "Video"
    case nil:
        kind = UTType(filenameExtension: (download.name as NSString).pathExtension)?
            .localizedDescription.map { $0.prefix(1).uppercased() + $0.dropFirst() } ?? "File"
    }
    guard let host = URL(string: download.url)?.host() else { return kind }
    let site = host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    return "\(kind) from \(site)"
}

/// The Spaces side by side, like Arc's: a column each, in sidebar order, in its theme's colors,
/// with its name, its profile, its tabs, a brush for its theme, a handle to move it and its menu;
/// like Arc's, a click on it does nothing else. The "+" after them creates a Space.
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
            // Like Arc's, measured: the columns the view's height less 96 points, centered.
            .padding(.horizontal, 24)
            .padding(.vertical, 48)
        }
        .scrollIndicators(.never)
    }
}

/// A Space's column among the Spaces, as Arc's, measured: 240 points wide; a Space with colors
/// shows them at three quarters over a light gray, or over a near black when dark, one without
/// colors nothing under its titles; the name 25 points down, then its rows, which scroll when
/// they overflow.
struct SpaceColumn: View {
    @ObservedObject var model: AppModel
    let space: Space
    let index: Int

    private var colored: Bool { !space.theme.colors.isEmpty }
    /// Like Arc's, a dark theme's column is dark, its text light.
    private var dark: Bool { colored && isDark(space.theme) }
    private var shape: RoundedRectangle { RoundedRectangle(cornerRadius: 9) }
    /// Its frame, from the top left corner of the window it shows in.
    private let _frame = State<CGRect>(initialValue: .zero)

    private var profileName: String {
        model.state?.profiles.first { $0.id == space.profile }?.name ?? "Default"
    }

    var body: some View {
        // Like Arc's, every Space's pinned tabs and folders, then today's tabs of the displayed
        // one, under the line that clears them.
        let displayed = space.id == model.state?.space
        let today = displayed ? (model.state?.pages.first { $0.space == space.id }?.today ?? []) : []
        // Like Arc's, measured: the rows in the theme's titles' color, whole; the header and the
        // controls in it at 55%.
        let titles = space.theme.colors.first.map { ThemePalette($0).titles }
        let ink = titles.map { color(dark ? $0.dark : $0.light) } ?? Ink.primary
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 9) {
                if let emoji = space.emoji { SpaceGlyph(icon: emoji, size: 16) }
                Text(space.title).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                Spacer(minLength: 0)
                // Like Arc's, the profile's name in full: the title gives way.
                ProfileChip(name: profileName, dark: dark) { model.run("menu", ["space-profile": space.id]) }
                    .fixedSize()
                IconButton(symbol: "paintbrush.pointed.fill", label: "Edit Theme Color", size: 24) {
                    // Like Arc's, beside the column.
                    let column = inMainWindow(_frame.wrappedValue)
                    model.run(
                        "show-theme-picker", space.id,
                        [Int(column.minX.rounded()), Int(column.maxX.rounded())])
                }
                .opacity(0.75)
            }
            .foregroundStyle(ink.opacity(0.55))
            .padding(.leading, 17)
            .padding(.trailing, 14)
            // Every column's at the same height, whatever its icon.
            .frame(height: 24)
            .padding(.top, 13)
            .padding(.bottom, 8)
            // Like Arc's, whose column holds the Space's sidebar: the tabs scroll in it, between
            // its header and its controls, however many they are.
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(space.pinned) { item in
                        SpaceColumnRow(
                            icon: item.favicon, title: item.title, folder: item.folder == true)
                    }
                    if !today.isEmpty {
                        HStack(spacing: 6) {
                            Rectangle().fill(ink.opacity(0.15)).frame(height: 1)
                            Button("↓ Clear") { model.run("clear-today") }
                                .buttonStyle(.plain)
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(Ink.secondary)
                        }
                        .padding(.horizontal, 17)
                        .frame(height: 10)
                        .padding(.vertical, 10)
                        ForEach(today.flatMap(\.tabs)) { tab in
                            SpaceColumnRow(icon: tab.favicon, title: tab.label, folder: false)
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollIndicators(.never)
            HStack {
                Image(systemName: "arrow.up.and.down.and.arrow.left.and.right")
                    .font(.system(size: 12, weight: .medium))
                    .frame(width: 24, height: 24)
                    .help("Drag to Move")
                Spacer()
                IconButton(symbol: "ellipsis", label: "Space options", size: 24) {
                    model.run("menu", ["space": space.id])
                }
            }
            .foregroundStyle(ink.opacity(0.55))
            .padding(.horizontal, 10)
            .padding(.bottom, 10)
        }
        .foregroundStyle(ink)
        .frame(width: 240)
        .frame(maxHeight: .infinity)
        .background {
            if colored {
                ZStack {
                    Color(white: dark ? 0.06 : 0.765)
                    Tint.colors(space.theme.colors).opacity(0.75)
                }
                .clipShape(shape)
            } else {
                // Like Arc's, every Space on a card: lighter, with a soft shadow, in light themes,
                // a little lighter in dark ones.
                shape.fill(Color.ink((0xFFFFFF, 0.5), (0xFFFFFF, 0.025)))
                    .shadow(color: Color.ink((0x000000, 0.09), (0x000000, 0)), radius: 10)
            }
        }
        .contentShape(shape)
        .background(Frame(frame: _frame.projectedValue))
        .rightClick { model.run("menu", ["space": space.id]) }
        // Dragged onto another, it takes that one's place.
        .draggable("space:\(space.id)")
        .dropDestination(for: String.self) { ids, _ in
            guard let id = ids.first?.split(separator: ":").last.flatMap({ Int($0) }),
                ids.first?.hasPrefix("space:") == true, id != space.id
            else { return false }
            model.run("move-space", id, index)
            return true
        }
    }
}

/// A row of a Space's column, like Arc's sidebar's, measured: 41 points high, a 16-point icon, or a
/// folder's, then the title, a folder's bold.
private struct SpaceColumnRow: View {
    let icon: String?
    let title: String
    let folder: Bool

    var body: some View {
        HStack(spacing: 10) {
            if folder {
                Image(systemName: "folder").font(.system(size: 14, weight: .medium))
                    .frame(width: 16, height: 16)
            } else {
                Favicon(url: icon, size: 16)
            }
            Text(title).font(.system(size: 14, weight: folder ? .semibold : .regular))
                .lineLimit(1)
        }
        .padding(.horizontal, 17)
        .frame(height: Measure.row)
    }
}

/// A Space's profile in its column's header, like Arc's chip, measured: a 14-point face and the
/// profile's name in full, 11 points, in a 22-point capsule, whose menu picks another profile or
/// creates one.
private struct ProfileChip: View {
    let name: String
    var dark = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 3) {
                Image(systemName: "face.smiling.inverse").font(.system(size: 14))
                Text(name).font(.system(size: 11, weight: .semibold)).lineLimit(1)
            }
            .padding(.leading, 4)
            .padding(.trailing, 7)
            .frame(height: 22)
            .background(Capsule().fill(.white.opacity(dark ? 0.08 : 0.35)))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help("Edit Space profile")
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
