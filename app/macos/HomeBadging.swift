// The recent items over the library's button, like Arc's "Home badging", measured: the newest
// unread one drops onto the button as a small badge, and the button hovered unfolds the four
// newest above the sidebar's footer. They come from the downloads, the captures kept in the
// library and, when turned on from the button's menu, the Documents and Desktop folders and the
// screenshots ("Show media from:").

import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// A recent item: a download, a capture or a file of a folder.
struct RecentItem: Identifiable, Equatable {
    enum Kind: Equatable { case download, capture, file }
    var id: String
    var title: String
    var date: Date
    var path: String
    var kind: Kind
    /// The download it is, if one.
    var download: Download?

    static func == (a: RecentItem, b: RecentItem) -> Bool {
        a.id == b.id && a.download?.state == b.download?.state
            && a.download?.received == b.download?.received
    }

    var running: Bool { download?.state == "inProgress" }
}

/// The sources the button's menu turns on or off, in Arc's order, with their names.
let mediaSources: [(id: String, name: String)] = [
    ("documents", "Documents"), ("desktop", "Desktop"), ("screenshots", "Screenshots"),
    ("captures", "Arbalete Captures"), ("downloads", "Downloads"),
]

/// The recent items and which one the badge shows, kept fresh while the window lives.
@MainActor
final class RecentItems: ObservableObject {
    @Published private(set) var items: [RecentItem] = []
    /// The item the badge shows, the newest unread one.
    @Published private(set) var badge: String?
    /// Items hidden from the button, like Arc's Hide from Arc.
    private var hidden: Set<String> = []
    /// The items known already, which a new one is not.
    private var known: Set<String>?
    /// Since when items count as unread: the library opened last.
    private var seenSince = Date()
    private var timer: Timer?
    private var dismissal: DispatchWorkItem?
    /// The library's button, which a new download flies to.
    weak var button: NSView?

    /// Gathers the items again, from the downloads and the sources turned on.
    func refresh(downloads: [Download], sources: [String]) {
        var found: [RecentItem] = []
        for download in downloads where !download.missing && download.state != "canceled"
            && download.state != "failed"
        {
            let capture = download.path.contains("/Captures/")
            guard sources.contains(capture ? "captures" : "downloads") else { continue }
            found.append(
                RecentItem(
                    id: "download-\(download.id)", title: download.name,
                    date: Date(timeIntervalSince1970: download.startedAt / 1000),
                    path: download.path, kind: capture ? .capture : .download, download: download))
        }
        let home = FileManager.default.homeDirectoryForCurrentUser
        var folders: [(URL, Bool?)] = []
        if sources.contains("documents") { folders.append((home.appending(path: "Documents"), nil)) }
        if sources.contains("desktop") { folders.append((home.appending(path: "Desktop"), false)) }
        if sources.contains("screenshots") { folders.append((screenshotsFolder(home), true)) }
        for (folder, screenshots) in folders {
            found += recentFiles(in: folder, screenshots: screenshots)
        }
        var seen = Set<String>()
        let items = found.filter { !hidden.contains($0.id) && seen.insert($0.id).inserted }
            .sorted { $0.date > $1.date }
        let newest = Array(items.prefix(4))
        if newest != self.items { self.items = newest }
        // Like Arc's, a new item drops onto the button; the ones there at launch do not.
        let ids = Set(items.map(\.id))
        defer { known = (known ?? []).union(ids) }
        guard let known, let fresh = items.first(where: { !known.contains($0.id) && $0.date > seenSince })
        else { return }
        // Like Arc's, a new download first flies from the page to the button.
        if fresh.kind == .download, let button {
            flyDownload(fresh, to: button) { [weak self] in self?.show(fresh) }
        } else {
            show(fresh)
        }
    }

    private func show(_ item: RecentItem) {
        dismissal?.cancel()
        badge = item.id
        // Like Arc's, measured: gone after about five seconds, but a download while it runs.
        guard !item.running else { return }
        let dismiss = DispatchWorkItem { [weak self] in self?.dismissBadge() }
        dismissal = dismiss
        DispatchQueue.main.asyncAfter(deadline: .now() + 4.9, execute: dismiss)
    }

    func dismissBadge() {
        dismissal?.cancel()
        badge = nil
    }

    /// The library opened: what came before is read.
    func markRead() {
        seenSince = Date()
        dismissBadge()
    }

    func hide(_ item: RecentItem) {
        hidden.insert(item.id)
        items.removeAll { $0.id == item.id }
        if badge == item.id { dismissBadge() }
    }

    /// Keeps the folders' items fresh, a few seconds apart.
    func start(_ refresh: @escaping () -> Void) {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { _ in
            MainActor.assumeIsolated { refresh() }
        }
        refresh()
    }
}

/// Where macOS saves screenshots: its setting, or the Desktop.
private func screenshotsFolder(_ home: URL) -> URL {
    let location = UserDefaults(suiteName: "com.apple.screencapture")?.string(forKey: "location")
    return location.map { URL(fileURLWithPath: ($0 as NSString).expandingTildeInPath) }
        ?? home.appending(path: "Desktop")
}

/// A folder's newest files, at most four: its screenshots only, or none of them, or all.
private func recentFiles(in folder: URL, screenshots: Bool?) -> [RecentItem] {
    let keys: [URLResourceKey] = [.creationDateKey, .isRegularFileKey]
    let urls =
        (try? FileManager.default.contentsOfDirectory(
            at: folder, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles])) ?? []
    let files = urls.compactMap { url -> RecentItem? in
        guard let values = try? url.resourceValues(forKeys: Set(keys)), values.isRegularFile == true,
            let date = values.creationDate
        else { return nil }
        if let screenshots, isScreenshot(url) != screenshots { return nil }
        return RecentItem(
            id: url.path, title: url.lastPathComponent, date: date, path: url.path, kind: .file)
    }
    return Array(files.sorted { $0.date > $1.date }.prefix(4))
}

/// Whether a file is a screenshot or a screen recording macOS saved.
private func isScreenshot(_ url: URL) -> Bool {
    let meta = NSMetadataItem(url: url)?.value(forAttribute: "kMDItemIsScreenCapture") as? Bool
    return meta ?? false
}

// MARK: The button

/// The library's button at the footer's start, like Arc's: its badge, the recent items it
/// unfolds while hovered, and its menu of sources.
struct LibraryEntry: View {
    @ObservedObject var model: AppModel
    @ObservedObject var recent: RecentItems
    /// The footer's width, which the recent items take, inside the sidebar.
    let width: CGFloat
    private let hoveredButton = State(initialValue: false)
    private let hoveredFlyout = State(initialValue: false)
    private let open = State(initialValue: false)
    private let closing = State<DispatchWorkItem?>(initialValue: nil)

    var body: some View {
        let sources = model.state?.mediaSources ?? ["captures", "downloads"]
        IconButton(symbol: "books.vertical", label: "View Library ⇧⌘L") {
            recent.markRead()
            model.openLibrary(nil)
        }
        .overlay(alignment: .topTrailing) {
            if let item = recent.items.first(where: { $0.id == recent.badge }), !open.wrappedValue {
                Badge(item: item)
                    .offset(x: 9, y: -9)
                    .allowsHitTesting(false)
                    .transition(.badgeDrop)
            }
        }
        .animation(.spring(response: 0.3, dampingFraction: 0.65), value: recent.badge)
        .background(ButtonAnchor(recent: recent))
        .onHover { over in
            hoveredButton.wrappedValue = over
            hover()
        }
        // The sidebar keeps right clicks for its own menus: like the Spaces' icons, the button
        // shows its own, of the sources, like Arc's.
        .rightClick { showSourcesMenu(sources) }
        .overlay(alignment: .bottomLeading) {
            if open.wrappedValue && !recent.items.isEmpty {
                Flyout(model: model, recent: recent, width: width)
                    // Measured on Arc's: its last row 2 points over the button.
                    .padding(.bottom, 30)
                    .offset(x: -2)
                    .onHover { over in
                        hoveredFlyout.wrappedValue = over
                        hover()
                    }
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.easeOut(duration: 0.14), value: open.wrappedValue)
    }

    /// The menu of the sources, like Arc's: a header, then each source, checked when on.
    private func showSourcesMenu(_ sources: [String]) {
        let menu = NSMenu()
        menu.autoenablesItems = false
        let header = NSMenuItem(title: "Show media from:", action: nil, keyEquivalent: "")
        header.isEnabled = false
        menu.addItem(header)
        for source in mediaSources {
            let on = sources.contains(source.id)
            let action = MenuAction { [model] in
                model.run("media/source/\(source.id)/\(on ? "off" : "on")")
            }
            let item = NSMenuItem(
                title: source.name, action: #selector(MenuAction.run), keyEquivalent: "")
            item.target = action
            item.representedObject = action
            item.state = on ? .on : .off
            menu.addItem(item)
        }
        menu.popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
    }

    /// Opens the flyout a moment after the pointer comes, and closes it once it left both the
    /// button and the flyout.
    private func hover() {
        closing.wrappedValue?.cancel()
        let over = hoveredButton.wrappedValue || hoveredFlyout.wrappedValue
        let change = DispatchWorkItem { open.wrappedValue = over }
        closing.wrappedValue = change
        DispatchQueue.main.asyncAfter(deadline: .now() + (over ? 0.12 : 0.08), execute: change)
    }
}

/// Hands the button's view to the recent items, for downloads to fly to.
private struct ButtonAnchor: NSViewRepresentable {
    let recent: RecentItems

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        recent.button = view
        return view
    }

    func updateNSView(_ view: NSView, context: Context) { recent.button = view }
}

/// Like Arc's DownloadToLibrary: the download's icon flies from the page's middle to the
/// library's button, shrinking along a curve, in a clear window over the app's.
@MainActor
private func flyDownload(_ item: RecentItem, to button: NSView, done: @escaping () -> Void) {
    guard let window = button.window else { return done() }
    let page = (window.parent ?? window).frame
    let target = window.convertToScreen(button.convert(button.bounds, to: nil))
    let overlay = NSPanel(
        contentRect: page.union(target).insetBy(dx: -80, dy: -160), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
        defer: false)
    overlay.isOpaque = false
    overlay.backgroundColor = .clear
    overlay.hasShadow = false
    overlay.ignoresMouseEvents = true
    overlay.level = .floating
    overlay.isReleasedWhenClosed = false
    let root = NSView()
    root.wantsLayer = true
    overlay.contentView = root

    let type = UTType(filenameExtension: (item.title as NSString).pathExtension) ?? .data
    let icon = CALayer()
    icon.contents = NSWorkspace.shared.icon(for: type)
    icon.contentsGravity = .resizeAspect
    icon.bounds = CGRect(x: 0, y: 0, width: 72, height: 72)
    icon.shadowOpacity = 0.25
    icon.shadowRadius = 8
    icon.shadowOffset = CGSize(width: 0, height: -3)
    // Where AppKit put the window, which it may move off the menu bar.
    let area = overlay.frame
    let local = { (point: CGPoint) in CGPoint(x: point.x - area.minX, y: point.y - area.minY) }
    let start = local(CGPoint(x: page.midX, y: page.midY))
    let end = local(CGPoint(x: target.midX, y: target.midY))
    icon.position = end
    root.layer?.addSublayer(icon)

    // A curve rising first, then falling onto the button.
    let path = CGMutablePath()
    path.move(to: start)
    path.addQuadCurve(to: end, control: CGPoint(x: (start.x + end.x) / 2, y: max(start.y, end.y) + 120))
    let move = CAKeyframeAnimation(keyPath: "position")
    move.path = path
    let shrink = CABasicAnimation(keyPath: "transform.scale")
    shrink.fromValue = 1
    shrink.toValue = 0.3
    let fade = CAKeyframeAnimation(keyPath: "opacity")
    fade.values = [0, 1, 1, 0]
    fade.keyTimes = [0, 0.12, 0.85, 1]
    let flight = CAAnimationGroup()
    flight.animations = [move, shrink, fade]
    flight.duration = 0.65
    flight.timingFunction = CAMediaTimingFunction(controlPoints: 0.45, 0, 0.6, 1)
    icon.opacity = 0

    overlay.orderFront(nil)
    CATransaction.begin()
    CATransaction.setCompletionBlock {
        overlay.orderOut(nil)
        done()
    }
    icon.add(flight, forKey: "flight")
    CATransaction.commit()
}

extension AnyTransition {
    /// Like Arc's badge, measured: it fades in a little smaller and higher, and leaves tilting
    /// down into the button.
    static var badgeDrop: AnyTransition {
        .asymmetric(
            insertion: .opacity.combined(with: .scale(scale: 0.9)).combined(with: .offset(y: -4)),
            removal: .opacity.combined(with: .scale(scale: 0.75)).combined(with: .offset(x: -2, y: 8))
                .animation(.easeIn(duration: 0.14)))
    }
}

/// The badge: the item's picture, 24 points, or a download's progress ring.
private struct Badge: View {
    let item: RecentItem

    var body: some View {
        if item.running, let download = item.download {
            ProgressRing(download: download, size: 20)
        } else {
            Thumbnail(item: item, size: 24)
        }
    }
}

/// The recent items, the newest at the bottom by the button, like Arc's.
private struct Flyout: View {
    @ObservedObject var model: AppModel
    @ObservedObject var recent: RecentItems
    let width: CGFloat

    var body: some View {
        VStack(spacing: 8) {
            ForEach(recent.items.reversed()) { item in
                RecentRow(model: model, recent: recent, item: item)
            }
        }
        .frame(width: width)
    }
}

/// A recent item, like Arc's: its picture, its name, and how long ago, or a download's progress.
private struct RecentRow: View {
    @ObservedObject var model: AppModel
    @ObservedObject var recent: RecentItems
    let item: RecentItem
    private let hovered = State(initialValue: false)

    var body: some View {
        HStack(spacing: 11) {
            Group {
                if item.running, let download = item.download {
                    ProgressRing(download: download, size: 25)
                } else {
                    Thumbnail(item: item, size: 43)
                }
            }
            .frame(width: 43, height: 43)
            VStack(alignment: .leading, spacing: 2) {
                // Like the tabs' titles, in the theme's title color; cut in the middle, like the
                // Finder's, to keep the extension.
                Text(item.title).font(.system(size: 14)).foregroundStyle(model.titleInk)
                    .lineLimit(1).truncationMode(.middle)
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    Text(subtitle).font(.system(size: 12))
                        .foregroundStyle(model.titleInk.opacity(0.6)).lineLimit(1)
                }
            }
            Spacer(minLength: 4)
            if item.running, let download = item.download {
                Button {
                    model.run("download/cancel/\(download.id)")
                } label: {
                    Image(systemName: "xmark").font(.system(size: 9, weight: .bold))
                        .foregroundStyle(model.titleInk.opacity(0.6))
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Cancel Download")
            }
        }
        .padding(.leading, 2)
        .padding(.trailing, 10)
        .frame(height: 48)
        .background(
            RoundedRectangle(cornerRadius: 12).fill(Ink.item.opacity(hovered.wrappedValue ? 1 : 0)))
        .contentShape(Rectangle())
        .onHover { hovered.wrappedValue = $0 }
        .onTapGesture(perform: open)
        .help(item.title)
        .contextMenu {
            Button("Open “\(item.title)”", action: open)
            Button("Copy “\(item.title)”") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.writeObjects([URL(fileURLWithPath: item.path) as NSURL])
            }
            Button("Show in Finder") {
                NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: item.path)])
            }
            Divider()
            Button("Hide from Arbalete") { recent.hide(item) }
            Button("Move to Trash") {
                try? FileManager.default.trashItem(at: URL(fileURLWithPath: item.path), resultingItemURL: nil)
                if let download = item.download { model.run("download/remove/\(download.id)") }
                recent.hide(item)
            }
            if item.running, let download = item.download {
                Button("Cancel Download") { model.run("download/cancel/\(download.id)") }
            }
        }
    }

    private var subtitle: String {
        if item.running, let download = item.download { return downloadDetail(download) }
        return timeAgo(item.date.timeIntervalSince1970 * 1000)
    }

    /// Like Arc's: a picture or a video in the media viewer, another file in its app.
    private func open() {
        if let download = item.download, mediaKind(download) != nil, !item.running {
            model.viewing = download.id
        } else if !item.running {
            NSWorkspace.shared.open(URL(fileURLWithPath: item.path))
        }
    }
}

/// An item's picture, rounded, a capture's with a dashed edge like Arc's; a file's icon otherwise.
private struct Thumbnail: View {
    let item: RecentItem
    let size: CGFloat

    var body: some View {
        FileThumbnail(path: picturePath) { picture($0) }
    }

    @ViewBuilder private func picture(_ image: NSImage?) -> some View {
        let radius = size * 0.14
        if let image {
            Image(nsImage: image).resizable().aspectRatio(contentMode: .fill)
                .frame(width: size, height: size)
                .clipShape(RoundedRectangle(cornerRadius: radius))
                .overlay {
                    if item.kind == .capture {
                        RoundedRectangle(cornerRadius: radius)
                            .strokeBorder(
                                Ink.secondary,
                                style: StrokeStyle(lineWidth: size > 30 ? 1 : 0.5, dash: [3, 2]))
                    }
                }
        } else {
            Image(nsImage: NSWorkspace.shared.icon(forFile: item.path)).resizable()
                .aspectRatio(contentMode: .fit)
                .frame(width: size * 0.62, height: size * 0.84)
                .frame(width: size, height: size)
        }
    }

    /// The file to show a picture of, if it is one.
    private var picturePath: String? {
        if let download = item.download {
            return mediaKind(download) == .image ? download.path : nil
        }
        let ext = (item.path as NSString).pathExtension.lowercased()
        return ["png", "jpg", "jpeg", "gif", "heic", "webp", "tiff"].contains(ext) ? item.path : nil
    }
}

/// A download's progress, like Arc's: a ring filling clockwise around a down arrow.
private struct ProgressRing: View {
    let download: Download
    let size: CGFloat

    var body: some View {
        let accent = Color(red: 0.11, green: 0.15, blue: 1)
        let progress = download.total > 0 ? download.received / download.total : 0
        ZStack {
            Circle().fill(Color(red: 0.91, green: 0.92, blue: 1))
            Circle().stroke(Color(red: 0.73, green: 0.75, blue: 1), lineWidth: 1.5)
            Circle().trim(from: 0, to: max(0.02, progress))
                .stroke(accent, style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
                .rotationEffect(.degrees(-90))
            Image(systemName: "arrow.down").font(.system(size: size * 0.42, weight: .bold))
                .foregroundStyle(accent)
        }
        .frame(width: size, height: size)
    }
}

/// A menu item's action as a closure, kept alive by the item that runs it.
final class MenuAction: NSObject {
    let action: () -> Void
    init(_ action: @escaping () -> Void) { self.action = action }
    @objc func run() { action() }
}
