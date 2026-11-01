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
    let panel: ChildPanel
    weak var window: NSWindow?
    private var watch: NSObjectProtocol?

    init(id: Int, window: NSWindow, app: AppModel) {
        self.id = id
        self.app = app
        self.window = window
        panel = ChildPanel(view: NSView())
        let host = NSHostingView(rootView: LittleBarView(bar: self))
        host.autoresizingMask = [.width, .height]
        panel.contentView?.addSubview(host)
        place()
        watch = NotificationCenter.default.addObserver(
            forName: NSWindow.didResizeNotification, object: window, queue: .main
        ) { [weak self] _ in MainActor.assumeIsolated { self?.place() } }
    }

    /// Over the window's top, where the app leaves room for the bar.
    func place() {
        guard let window, let content = window.contentView else { return }
        let height = 40.0
        panel.place(in: window, NSRect(x: 0, y: 0, width: content.frame.width, height: height))
        panel.contentView?.subviews.forEach { $0.frame = panel.contentView?.bounds ?? .zero }
    }

    func close() {
        if let watch { NotificationCenter.default.removeObserver(watch) }
        if let window { panel.place(in: window, nil) }
    }

    func go(_ text: String) {
        guard let page else { return }
        let url = toURL(text) ?? page.search.url + percentEncoded(text)
        app.run("little/\(id)/navigate/\(percentEncoded(url))")
        editing = false
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
                        Text(page.space.emoji ?? "")
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

@MainActor var littles: [Int: LittleBar] = [:]

@_cdecl("arbalete_ui_little")
public func little(_ id: UInt32, _ handle: UnsafeMutableRawPointer?) {
    MainActor.assumeIsolated {
        let id = Int(id)
        // Its commands come from a window of the app's, any.
        guard let handle, let app = nativeWindows.values.first?.model else {
            littles.removeValue(forKey: id)?.close()
            return
        }
        let object = Unmanaged<AnyObject>.fromOpaque(handle).takeUnretainedValue()
        guard let window = (object as? NSWindow) ?? (object as? NSView)?.window else { return }
        littles[id] = LittleBar(id: id, window: window, app: app)
    }
}
