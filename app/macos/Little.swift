// The bar of a Little Arbalete window (ui/src/Little.tsx), like Arc's Little Arc: beside the
// window buttons, the page's domain, which a click edits, or a field while there is no page yet;
// then "Open in <Space>" (⌘O), which moves the page into that Space and closes the window.

import AppKit
import SwiftUI

struct LittleDetail: Decodable {
    struct SpaceName: Decodable {
        var title: String
        var emoji: String?
    }
    var url: String
    var title: String
    /// The displayed Space, which "Open in" moves the page into.
    var space: SpaceName
    var search: SearchEngine
}

@MainActor
final class LittleBar: ObservableObject {
    @Published var page: LittleDetail?
    @Published var editing = false
    let id: Int
    let app: AppModel

    init(id: Int, app: AppModel) {
        self.id = id
        self.app = app
    }

    func go(_ text: String) {
        guard let page else { return }
        let url = toURL(text) ?? page.search.url + percentEncoded(text)
        app.run("little/\(id)/navigate/\(percentEncoded(url))")
        editing = false
    }
}

/// A Little Arbalete window's size, and its bar's height, which holds the window buttons, like
/// Little Arc's: a 31-point row with 8 points above and below.
private let littleSize = NSSize(width: 768, height: 600)
private let littleBarHeight: CGFloat = 47

/// A Little Arbalete window, the delegate of its NSWindow: its bar on top, then its page, which
/// CEF draws in `page`. The app closes its page before the window closes.
@MainActor
final class LittleWindow: NSObject, NSWindowDelegate {
    let bar: LittleBar
    let window: NSWindow
    let page = PageHost()

    init(id: Int, app: AppModel) {
        bar = LittleBar(id: id, app: app)
        window = NSWindow(
            contentRect: NSRect(origin: .zero, size: littleSize),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered, defer: false)
        super.init()
        window.title = "Little Arbalete"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        let content = NSView(frame: NSRect(origin: .zero, size: littleSize))
        let host = NSHostingView(rootView: LittleBarView(bar: bar))
        host.frame = NSRect(
            x: 0, y: littleSize.height - littleBarHeight, width: littleSize.width,
            height: littleBarHeight)
        host.autoresizingMask = [.width, .minYMargin]
        page.frame = NSRect(
            x: 0, y: 0, width: littleSize.width, height: littleSize.height - littleBarHeight)
        page.autoresizingMask = [.width, .height]
        content.addSubview(page)
        content.addSubview(host)
        window.contentView = content
        window.delegate = self
        window.center()
        // Centered in its bar, like Little Arc's.
        centerWindowButtons(window, height: littleBarHeight)
        window.makeKeyAndOrderFront(nil)
    }

    func windowDidResize(_ notification: Notification) {
        centerWindowButtons(window, height: littleBarHeight)
    }

    /// Its page closes first, which closes the window (arbalete_ui_little_closed).
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        arbalete_little_should_close(UInt32(bar.id))
    }
}

struct LittleBarView: View {
    @ObservedObject var bar: LittleBar
    private let text = State(initialValue: "")
    private let _hovered = State(initialValue: false)

    var body: some View {
        if let page = bar.page {
            let empty = page.url.isEmpty || page.url == "about:blank"
            HStack(spacing: 8) {
                if bar.editing || empty {
                    TextField("Search \(page.search.name) or Enter URL…", text: text.projectedValue)
                        .textFieldStyle(.roundedBorder)
                        .onAppear { text.wrappedValue = empty ? "" : page.url }
                        .onSubmit {
                            let typed = text.wrappedValue.trimmingCharacters(in: .whitespaces)
                            if !typed.isEmpty { bar.go(typed) }
                        }
                        .onExitCommand { bar.editing = false }
                } else {
                    // The page's address in a pill the width of the bar, like Little Arc's.
                    Button {
                        bar.editing = true
                    } label: {
                        Text(domain(page.url)).lineLimit(1)
                            .padding(.horizontal, 10)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .frame(height: 31)
                            .background(
                                RoundedRectangle(cornerRadius: 5).fill(
                                    _hovered.wrappedValue
                                        ? Color.ink((0x0E0F10, 0.10), (0xFAFBFF, 0.15))
                                        : Color.ink((0x0E0F10, 0.06), (0xFAFBFF, 0.10))))
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .onHover { _hovered.wrappedValue = $0 }
                    .help(page.url)
                }
                Button {
                    bar.app.run("little/\(bar.id)/open")
                } label: {
                    HStack(spacing: 6) {
                        SpaceGlyph(icon: page.space.emoji ?? "", size: 13)
                            .frame(width: 26, height: 26)
                            .background(RoundedRectangle(cornerRadius: 4).fill(.black.opacity(0.15)))
                        Text("Open in \(page.space.title)")
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Open in \(page.space.title) (⌘O)")
                .disabled(empty)
            }
            .padding(.leading, 80)
            .padding(.trailing, 7)
            .padding(.vertical, 8)
            .frame(maxHeight: .infinity)
        }
    }
}

@MainActor var littles: [Int: LittleWindow] = [:]

/// Opens the Little Arbalete window `id` and returns the view CEF draws its page in.
@_cdecl("arbalete_ui_open_little")
public func openLittle(_ id: UInt32) -> UnsafeMutableRawPointer? {
    // As an address: a pointer may not leave the main actor's closure.
    let address: UInt = MainActor.assumeIsolated {
        // Its commands go through a window of the app's, any.
        guard let app = nativeWindows.values.first?.model else { return 0 }
        let little = LittleWindow(id: Int(id), app: app)
        littles[Int(id)] = little
        return UInt(bitPattern: Unmanaged.passUnretained(little.page).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// Closes the Little Arbalete window `id`, its page closing: its view goes with the window.
@_cdecl("arbalete_ui_little_closed")
public func littleClosed(_ id: UInt32) {
    MainActor.assumeIsolated { littles.removeValue(forKey: Int(id))?.window.close() }
}

/// The Little Arbalete window that has the keyboard, or -1.
@_cdecl("arbalete_ui_focused_little")
public func focusedLittle() -> Int64 {
    MainActor.assumeIsolated {
        littles.first { $0.value.window.isKeyWindow }.map { Int64($0.key) } ?? -1
    }
}
