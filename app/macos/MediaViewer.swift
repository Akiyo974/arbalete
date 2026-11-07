// The library's media viewer, like Arc's: a picture or a video downloaded, on a card over the
// window tinted with its colors, which steps through the others.

import AVKit
import AppKit
import SwiftUI

/// The images and videos the library's Media section shows, the most recent first.
@MainActor func libraryMedia(_ model: AppModel) -> [Download] {
    model.downloads.reversed().filter { download in
        download.state == "complete" && !download.missing && mediaKind(download) != nil
    }
}

enum MediaKind { case image, video }

func mediaKind(_ download: Download) -> MediaKind? {
    switch (download.name as NSString).pathExtension.lowercased() {
    case "png", "jpg", "jpeg", "gif", "webp", "avif": .image
    case "mp4", "m4v", "webm", "mov": .video
    default: nil
    }
}

/// Shows the viewer over the window `parent` while its model views a download.
@MainActor
final class MediaViewer {
    private let overlay: Overlay
    private weak var parent: NSWindow?
    private let model: AppModel
    private var keys: Any?
    private var resized: NSObjectProtocol?

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        self.model = model
        let host = NSHostingView(rootView: MediaViewerView(model: model))
        host.safeAreaRegions = []
        host.sizingOptions = []
        host.autoresizingMask = [.width, .height]
        let veil = MediaVeil()
        veil.addSubview(host)
        overlay = Overlay(view: veil)
        resized = NotificationCenter.default.addObserver(
            forName: NSWindow.didResizeNotification, object: parent, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.place() }
        }
    }

    /// Shows it over the whole window while a download is viewed, or hides it.
    func update() {
        if model.viewing == nil {
            overlay.place(in: parent ?? NSWindow(), nil)
            if let keys { NSEvent.removeMonitor(keys) }
            keys = nil
            return
        }
        place()
        guard keys == nil else { return }
        // Escape closes it, the arrows step through the media, like Arc's.
        keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === self.parent else { return event }
            switch event.keyCode {
            case 53: self.model.viewing = nil
            case 123: self.model.stepMedia(-1)
            case 124: self.model.stepMedia(1)
            default: return event
            }
            return nil
        }
    }

    private func place() {
        guard let parent, model.viewing != nil, let content = parent.contentView else { return }
        overlay.place(in: parent, NSRect(origin: .zero, size: content.bounds.size))
    }

    func close() {
        if let keys { NSEvent.removeMonitor(keys) }
        if let resized { NotificationCenter.default.removeObserver(resized) }
        overlay.close()
    }
}

/// The veil over the window, under the card, which draws itself like the capture's: a SwiftUI
/// color, or a layer's, would not show over the page.
final class MediaVeil: NSView {
    override func draw(_ dirtyRect: NSRect) {
        NSColor.black.withAlphaComponent(0.45).setFill()
        dirtyRect.fill()
    }
}

extension AppModel {
    /// Views the medium `step` places after the one viewed, in the Media section's order.
    func stepMedia(_ step: Int) {
        let media = libraryMedia(self)
        guard let at = media.firstIndex(where: { $0.id == viewing }) else { return }
        let next = at + step
        if media.indices.contains(next) { viewing = media[next].id }
    }
}

/// The card, measured on Arc's: about three quarters of the window, centered, over a veil that
/// closes it on a click.
struct MediaViewerView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture { model.viewing = nil }
                if let download = libraryMedia(model).first(where: { $0.id == model.viewing }) {
                    MediaCard(model: model, download: download)
                        .id(download.id)
                        .frame(
                            width: geometry.size.width * 0.77, height: geometry.size.height * 0.75)
                }
            }
        }
    }
}

struct MediaCard: View {
    @ObservedObject var model: AppModel
    let download: Download
    // The picture, once read, and its colors (State spelled out: see Sidebar.swift).
    private let _image = State<NSImage?>(initialValue: nil)
    private var image: NSImage? {
        get { _image.wrappedValue }
        nonmutating set { _image.wrappedValue = newValue }
    }
    private let _colors = State<[Color]>(initialValue: [Color(white: 0.2), Color(white: 0.15)])
    private var colors: (top: Color, bottom: Color) {
        get { (_colors.wrappedValue[0], _colors.wrappedValue[1]) }
        nonmutating set { _colors.wrappedValue = [newValue.top, newValue.bottom] }
    }

    var body: some View {
        let media = libraryMedia(model)
        let at = media.firstIndex(where: { $0.id == download.id }) ?? 0
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                HeaderButton(symbol: "xmark", label: "Close", filled: true) { model.viewing = nil }
                HeaderButton(symbol: "chevron.left", label: "Previous") { model.stepMedia(-1) }
                    .disabled(at == 0)
                HeaderButton(symbol: "chevron.right", label: "Next") { model.stepMedia(1) }
                    .disabled(at + 1 >= media.count)
                Text(download.name).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                Text(timeAgo(download.startedAt)).font(.system(size: 12))
                    .foregroundStyle(.white.opacity(0.6))
                Spacer(minLength: 8)
                Menu {
                    Button("Show in Finder") { model.run("download", "show", download.id) }
                    if image != nil {
                        Button("Copy Image") { copyImage() }
                    }
                } label: {
                    Image(systemName: "ellipsis").font(.system(size: 13, weight: .semibold))
                        .frame(width: 26, height: 22)
                        .contentShape(Rectangle())
                }
                .menuStyle(.button)
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .fixedSize()
                Button { openOutside() } label: {
                    HStack(spacing: 4) {
                        Text("Open in \(defaultAppName ?? "App")")
                        Image(systemName: "arrow.up.right")
                    }
                    .font(.system(size: 13, weight: .semibold))
                    .padding(.horizontal, 7)
                    .frame(height: 24)
                    .background(RoundedRectangle(cornerRadius: 6).fill(.white.opacity(0.14)))
                }
                .buttonStyle(.plain)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 6)
            .frame(height: 30)
            .background(colors.top.opacity(0.85))
            ZStack {
                switch mediaKind(download) {
                case .video:
                    VideoPlayer(player: AVPlayer(url: URL(fileURLWithPath: download.path)))
                default:
                    if let image {
                        Image(nsImage: image).resizable().aspectRatio(contentMode: .fit)
                    }
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(
                LinearGradient(
                    colors: [colors.top.opacity(0.92), colors.bottom.opacity(0.92)],
                    startPoint: .top, endPoint: .bottom))
        }
        .background(.regularMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.4), radius: 30, y: 12)
        .environment(\.colorScheme, .dark)
        .task {
            guard mediaKind(download) == .image, let loaded = NSImage(contentsOfFile: download.path)
            else { return }
            image = loaded
            colors = averageColors(loaded)
        }
    }

    private var fileURL: URL { URL(fileURLWithPath: download.path) }

    /// The name of the app the file opens in, as the system names it, like "Aperçu".
    private var defaultAppName: String? {
        NSWorkspace.shared.urlForApplication(toOpen: fileURL).map {
            FileManager.default.displayName(atPath: $0.path).replacingOccurrences(of: ".app", with: "")
        }
    }

    private func openOutside() {
        NSWorkspace.shared.open(fileURL)
        model.viewing = nil
    }

    private func copyImage() {
        guard let image else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.writeObjects([image])
    }
}

/// A button of the viewer's header: a glyph, on a light square when `filled`.
struct HeaderButton: View {
    let symbol: String
    let label: String
    var filled = false
    let action: () -> Void
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 12, weight: .bold))
                .frame(width: 24, height: 24)
                .background(RoundedRectangle(cornerRadius: 6).fill(.white.opacity(filled ? 0.14 : 0)))
                .contentShape(Rectangle())
                .opacity(enabled ? 1 : 0.35)
        }
        .buttonStyle(.plain)
        .help(label)
    }
}

/// How long ago a time was, in milliseconds since the epoch, as the system says it: "il y a 39 s".
func timeAgo(_ time: Double) -> String {
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .short
    return formatter.localizedString(
        for: Date(timeIntervalSince1970: time / 1000), relativeTo: Date())
}

/// The average colors of a picture's top and bottom halves, which tint the viewer like Arc's.
func averageColors(_ image: NSImage) -> (top: Color, bottom: Color) {
    let fallback = (Color(white: 0.2), Color(white: 0.15))
    guard let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil),
        let context = CGContext(
            data: nil, width: 1, height: 2, bitsPerComponent: 8, bytesPerRow: 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue),
        let data = context.data
    else { return fallback }
    context.interpolationQuality = .medium
    context.draw(cg, in: CGRect(x: 0, y: 0, width: 1, height: 2))
    let pixels = data.bindMemory(to: UInt8.self, capacity: 8)
    // The bitmap's first row is its top.
    func color(_ row: Int) -> Color {
        let at = row * 4
        return Color(
            red: Double(pixels[at]) / 255, green: Double(pixels[at + 1]) / 255,
            blue: Double(pixels[at + 2]) / 255)
    }
    return (color(0), color(1))
}
