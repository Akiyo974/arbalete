// Little Arbalete, like Arc's Little Arc: the menu (⌥⌘N) opens a Command Bar of its own at the
// screen's top right, which opens the window on what is picked. The window's bar holds, beside
// the window buttons, its page's domain with its profile and a button copying its address, which
// a click edits in a Command Bar over it; then "Open in <Space>" (⌘O), which moves the page into
// that Space and closes the window.

import AppKit
import SwiftUI

struct LittleDetail: Decodable {
    struct SpaceName: Decodable {
        var title: String
        var emoji: String?
    }
    var url: String
    var title: String
    /// The profile its page is in.
    var profile: String?
    /// The displayed Space, which "Open in" moves the page into.
    var space: SpaceName
    var search: SearchEngine
}

/// A Little Arbalete window's bar, measured on Little Arc's: 46 points tall, the window buttons
/// centered in it 10 points from the left edge, its address 83 points from it.
private let littleSize = NSSize(width: 768, height: 600)
private let littleBarHeight: CGFloat = 46
private let littleButtonsLeading: CGFloat = 10
private let littleAddressLeading: CGFloat = 83
/// The Command Bar opening a Little Arbalete window, at the screen's top right, like Little
/// Arc's, measured.
private let littleBarWidth: CGFloat = 490
private let littleBarMargin: CGFloat = 22

@MainActor
final class LittleBar: ObservableObject {
    @Published var page: LittleDetail?
    @Published var copied = false
    let id: Int
    let app: AppModel
    /// Edits its page's address, like Little Arc's.
    let command: CommandBar

    init(id: Int, app: AppModel) {
        self.id = id
        self.app = app
        command = CommandBar(app: app, little: .some(id))
    }

    /// Copies its page's address, which the button shows a moment.
    func copy() {
        guard let url = page?.url else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(url, forType: .string)
        copied = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in self?.copied = false }
    }
}

/// The view over a window's content that its surfaces show in, from its top left corner,
/// letting clicks through but on them.
private final class SurfaceLayer: NSView {
    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit === self ? nil : hit
    }
}

/// Gives a window a view over its content for surfaces, like the Command Bar.
@MainActor private func addSurfaceLayer(to window: NSWindow, over content: NSView) {
    let layer = SurfaceLayer(frame: content.bounds)
    layer.autoresizingMask = [.width, .height]
    content.addSubview(layer)
    overlays[ObjectIdentifier(window)] = { [weak layer] in layer }
}

/// A Command Bar of a Little Arbalete window's, in a surface of `window`, and the one that gets
/// the app's results while it shows.
@MainActor
private func littleCommandSurface(_ bar: CommandBar, in window: NSWindow) -> Surface {
    let surface = Surface(in: window, radius: 14) { CommandBarView(bar: bar) }
    surface.takesKeyboard = true
    surface.shown = { [weak bar] in
        guard let field = bar?.field, field.currentEditor() == nil else { return }
        field.window?.makeFirstResponder(field)
    }
    surface.resigned = { [weak bar] in bar?.hide() }
    return surface
}

/// The Command Bar that gets the app's results for Little Arbalete, the one showing.
@MainActor var littleCommandBar: CommandBar?

/// A Little Arbalete window, the delegate of its NSWindow: its bar on top, then its page, which
/// CEF draws in `page`. The app closes its page before the window closes.
@MainActor
final class LittleWindow: NSObject, NSWindowDelegate {
    let bar: LittleBar
    let window: NSWindow
    let page = PageHost()
    private var command: Surface!

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
        let host = NSHostingView(rootView: LittleBarView(bar: bar, edit: { [weak self] in self?.edit() }))
        // Its own room, not the title bar's safe area.
        host.safeAreaRegions = []
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
        addSurfaceLayer(to: window, over: content)
        command = littleCommandSurface(bar.command, in: window)
        bar.command.fit = { [weak self] height in self?.placeCommand(height: height) }
        bar.command.dismiss = { [weak self] in self?.closeCommand() }
        window.delegate = self
        window.center()
        centerButtons()
        window.makeKeyAndOrderFront(nil)
    }

    private func centerButtons() {
        centerWindowButtons(window, height: littleBarHeight, leading: littleButtonsLeading)
    }

    /// Edits its page's address in a Command Bar over its own, like Little Arc's.
    func edit() {
        guard let page = bar.page else { return }
        littleCommandBar = bar.command
        bar.command.littleShown = true
        bar.command.showLittle(url: page.url.isEmpty ? nil : page.url, search: page.search)
        placeCommand(height: 50)
    }

    private func placeCommand(height: CGFloat) {
        guard bar.command.littleShown, let content = window.contentView else { return }
        let width = content.bounds.width - littleAddressLeading - 8
        command.place(true, NSRect(x: littleAddressLeading, y: 6, width: width, height: height))
    }

    private func closeCommand() {
        bar.command.littleShown = false
        command.place(false, .zero)
        if littleCommandBar === bar.command { littleCommandBar = nil }
    }

    func windowDidResize(_ notification: Notification) {
        centerButtons()
    }

    /// Its page closes first, which closes the window (arbalete_ui_little_closed).
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        arbalete_little_should_close(UInt32(bar.id))
    }

    func windowWillClose(_ notification: Notification) {
        closeCommand()
        command.close()
        overlays[ObjectIdentifier(window)] = nil
    }
}

/// A panel that takes the keyboard without a title bar.
private final class KeyPanel: NSPanel {
    override var canBecomeKey: Bool { true }
}

/// The Command Bar opening a Little Arbalete window, in a panel of its own at the screen's top
/// right, like Little Arc's: picking a page opens the window on it, and the bar goes.
@MainActor
final class LittleNewBar {
    private let panel = KeyPanel(
        contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
        defer: false)
    private let bar: CommandBar
    private var surface: Surface!
    /// Room around the bar for its shadow.
    private let margin: CGFloat = 30

    init(app: AppModel, search: SearchEngine) {
        bar = CommandBar(app: app, little: .some(nil))
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.level = .floating
        panel.isReleasedWhenClosed = false
        let content = NSView()
        panel.contentView = content
        addSurfaceLayer(to: panel, over: content)
        surface = littleCommandSurface(bar, in: panel)
        bar.fit = { [weak self] height in self?.place(height: height) }
        bar.dismiss = { [weak self] in self?.close() }
        bar.littleShown = true
        littleCommandBar = bar
        place(height: 50)
        NSApp.activate()
        panel.makeKeyAndOrderFront(nil)
        bar.showLittle(url: nil, search: search)
    }

    private func place(height: CGFloat) {
        guard let screen = NSScreen.main else { return }
        let visible = screen.visibleFrame
        let frame = NSRect(
            x: visible.maxX - littleBarMargin - littleBarWidth - margin,
            y: visible.maxY - littleBarMargin - height - margin,
            width: littleBarWidth + 2 * margin, height: height + 2 * margin)
        panel.setFrame(frame, display: true)
        surface.place(true, NSRect(x: margin, y: margin, width: littleBarWidth, height: height))
    }

    func close() {
        guard bar.littleShown else { return }
        bar.littleShown = false
        if littleCommandBar === bar { littleCommandBar = nil }
        surface.close()
        overlays[ObjectIdentifier(panel)] = nil
        panel.orderOut(nil)
        if littleNewBar === self { littleNewBar = nil }
    }
}

@MainActor var littleNewBar: LittleNewBar?

struct LittleBarView: View {
    @ObservedObject var bar: LittleBar
    let edit: () -> Void
    private let _hovered = State(initialValue: false)

    var body: some View {
        if let page = bar.page {
            let empty = page.url.isEmpty || page.url == "about:blank"
            HStack(spacing: 9) {
                address(page, empty: empty)
                Button {
                    bar.app.run("little/\(bar.id)/open")
                } label: {
                    HStack(spacing: 5) {
                        Text("Open in").foregroundStyle(.secondary)
                        Text(page.space.title).fontWeight(.semibold).lineLimit(1)
                        Text("⌘O").font(.system(size: 11, weight: .medium))
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 4)
                            .frame(height: 18)
                            .background(RoundedRectangle(cornerRadius: 4).fill(.primary.opacity(0.1)))
                    }
                    .font(.system(size: 13))
                    .padding(.horizontal, 10)
                    .frame(height: 30)
                    .background(RoundedRectangle(cornerRadius: 6).fill(.black.opacity(0.3)))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .fixedSize()
                .help("Open in \(page.space.title) (⌘O)")
                .disabled(empty)
            }
            .padding(.leading, littleAddressLeading)
            .padding(.trailing, 6)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    /// The page's domain, centered in a pill the width of the bar, with its profile at its
    /// left and a button copying its address at its right, like Little Arc's.
    private func address(_ page: LittleDetail, empty: Bool) -> some View {
        ZStack {
            Text(empty ? "Search or Enter URL…" : domain(page.url))
                .font(.system(size: 13))
                .foregroundStyle(empty ? .secondary : .primary)
                .lineLimit(1)
                .padding(.horizontal, 90)
            HStack(spacing: 0) {
                if let profile = page.profile, !profile.isEmpty {
                    HStack(spacing: 4) {
                        Image(systemName: "face.smiling").font(.system(size: 11))
                        Text(profile).font(.system(size: 12, weight: .semibold)).lineLimit(1)
                    }
                    .padding(.horizontal, 6)
                    .frame(height: 21)
                    .background(
                        RoundedRectangle(cornerRadius: 4).fill(
                            Color.ink((0x0E0F10, 0.06), (0xFAFBFF, 0.08))))
                }
                Spacer(minLength: 0)
                if !empty {
                    Button(action: bar.copy) {
                        Image(systemName: bar.copied ? "checkmark" : "link")
                            .font(.system(size: 13))
                            .foregroundStyle(.secondary)
                            .frame(width: 26, height: 26)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help("Copy Link")
                }
            }
            .padding(.horizontal, 6)
        }
        .frame(maxWidth: .infinity)
        .frame(height: 31)
        .background(
            RoundedRectangle(cornerRadius: 5).fill(
                _hovered.wrappedValue
                    ? Color.ink((0x0E0F10, 0.10), (0xFAFBFF, 0.15))
                    : Color.ink((0x0E0F10, 0.06), (0xFAFBFF, 0.10))))
        .contentShape(Rectangle())
        .onTapGesture(perform: edit)
        .onHover { _hovered.wrappedValue = $0 }
        .help(page.url)
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

/// Shows the Command Bar that opens a Little Arbalete window, like Little Arc's.
@_cdecl("arbalete_ui_little_bar")
public func showLittleBar() {
    MainActor.assumeIsolated {
        guard let window = nativeWindows.values.first else { return }
        littleNewBar?.close()
        littleNewBar = LittleNewBar(app: window.model, search: window.commandBar.search)
    }
}
