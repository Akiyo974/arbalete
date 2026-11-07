// Little Arbalete, like Arc's Little Arc: the menu (⌥⌘N) opens a Command Bar of its own at the
// screen's top right, which opens the window on what is picked. The window's bar holds, beside
// the window buttons, its page's domain with its profile and a button copying its address, which
// a click edits in a Command Bar over it; then "Open in <Space>" (⌘O), which moves the page into
// that Space and closes the window.

import AppKit
import Carbon.HIToolbox
import SwiftUI

struct LittleDetail: Decodable {
    struct SpaceName: Decodable {
        var title: String
        var emoji: String?
    }
    struct SpaceChoice: Decodable, Identifiable {
        var id: UInt64
        var title: String
        var emoji: String?
    }
    struct ProfileChoice: Decodable, Identifiable {
        var id: UInt64
        var name: String
        var current: Bool
    }
    var url: String
    var title: String
    /// The profile its page is in, and the others it may move to.
    var profile: String?
    var profiles: [ProfileChoice]?
    /// The displayed Space, which "Open in" moves the page into, and the others its chevron
    /// offers.
    var space: SpaceName
    var spaces: [SpaceChoice]?
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

    /// Shows the menu of the extensions under its button at `frame`, from its window's top left
    /// corner: the app places their popup from the main window's, which it counts the button in.
    func extensions(from frame: CGRect) {
        guard let little = littles[id]?.window, let main = nativeWindows[app.window]?.window
        else { return }
        let x = little.frame.minX + frame.minX - main.frame.minX
        let y = main.frame.maxY - (little.frame.maxY - frame.minY)
        app.run(
            "extensions-menu",
            commandButton("toolbar", CGRect(x: x, y: y, width: frame.width, height: frame.height)))
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

    /// `size` is its window's: a popup's page asks for its own.
    init(id: Int, app: AppModel, size: NSSize) {
        bar = LittleBar(id: id, app: app)
        window = NSWindow(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered, defer: false)
        super.init()
        window.title = "Little Arbalete"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        let content = NSView(frame: NSRect(origin: .zero, size: size))
        let host = NSHostingView(rootView: LittleBarView(bar: bar, edit: { [weak self] in self?.edit() }))
        // Its own room, not the title bar's safe area.
        host.safeAreaRegions = []
        host.frame = NSRect(
            x: 0, y: size.height - littleBarHeight, width: size.width,
            height: littleBarHeight)
        host.autoresizingMask = [.width, .minYMargin]
        page.frame = NSRect(
            x: 0, y: 0, width: size.width, height: size.height - littleBarHeight)
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

    /// Its page becomes its profile's active tab for the extensions.
    func windowDidBecomeKey(_ notification: Notification) {
        bar.app.run("little", bar.id, "focused")
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
        // The screen the pointer is on, where the shortcut was pressed, rather than the one with
        // the key window, maybe another.
        let pointer = NSEvent.mouseLocation
        guard
            let screen = NSScreen.screens.first(where: { NSMouseInRect(pointer, $0.frame, false) })
                ?? NSScreen.main
        else { return }
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
                // Like Little Arc's: the displayed Space, or another from the chevron.
                HStack(spacing: 0) {
                    Button {
                        bar.app.run("little", bar.id, ["open-in-space": NSNull()])
                    } label: {
                        HStack(spacing: 5) {
                            Text("Open in").foregroundStyle(.secondary)
                            Text(page.space.title).fontWeight(.semibold).lineLimit(1)
                            Text("⌘O").font(.system(size: 11, weight: .medium))
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 4)
                                .frame(height: 18)
                                .background(
                                    RoundedRectangle(cornerRadius: 4).fill(.primary.opacity(0.1)))
                        }
                        .padding(.leading, 10)
                        .padding(.trailing, 8)
                        .frame(maxHeight: .infinity)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help("Open in \(page.space.title) (⌘O)")
                    Rectangle().fill(.primary.opacity(0.15)).frame(width: 1, height: 16)
                    Menu {
                        ForEach(page.spaces ?? []) { space in
                            Button((space.emoji.map { "\($0) " } ?? "") + space.title) {
                                bar.app.run("little", bar.id, ["open-in-space": space.id])
                            }
                        }
                    } label: {
                        Image(systemName: "chevron.down").font(.system(size: 10, weight: .semibold))
                            .foregroundStyle(.secondary)
                    }
                    .menuStyle(.borderlessButton)
                    .menuIndicator(.hidden)
                    .frame(width: 26)
                    .help("Open in Another Space")
                }
                .font(.system(size: 13))
                .frame(height: 30)
                .background(RoundedRectangle(cornerRadius: 6).fill(.black.opacity(0.3)))
                .fixedSize()
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
                    // Like Little Arc's, a menu opening the page again in another profile.
                    Menu {
                        ForEach(page.profiles ?? []) { choice in
                            Toggle(
                                choice.name,
                                isOn: Binding(
                                    get: { choice.current },
                                    set: { _ in
                                        bar.app.run("little", bar.id, ["set-profile": choice.id])
                                    }))
                        }
                    } label: {
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
                    .menuStyle(.borderlessButton)
                    .menuIndicator(.hidden)
                    .fixedSize()
                    .disabled((page.profiles?.count ?? 0) < 2)
                    .help("Profile")
                }
                Spacer(minLength: 0)
                if !empty {
                    // Like Little Arc's, a menu of the extensions, whose popup acts on its page.
                    GeometryReader { geometry in
                        Button {
                            bar.extensions(from: geometry.frame(in: .global))
                        } label: {
                            Image(systemName: "puzzlepiece.extension")
                                .font(.system(size: 13))
                                .foregroundStyle(.secondary)
                                .frame(width: 26, height: 26)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .help("Extensions")
                    }
                    .frame(width: 26, height: 26)
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

/// Opens the Little Arbalete window `id`, its page `width` by `height` points, or the usual size
/// for 0, and returns the view CEF draws its page in.
@_cdecl("arbalete_ui_open_little")
public func openLittle(_ id: UInt32, _ width: Int32, _ height: Int32) -> UnsafeMutableRawPointer? {
    // As an address: a pointer may not leave the main actor's closure.
    let address: UInt = MainActor.assumeIsolated {
        // Its commands go through a window of the app's, any.
        guard let app = nativeWindows.values.first?.model else { return 0 }
        let size = width > 0 && height > 0
            ? NSSize(width: CGFloat(width), height: CGFloat(height) + littleBarHeight) : littleSize
        let little = LittleWindow(id: Int(id), app: app, size: size)
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

/// The view the Little Arbalete window `id`'s page is drawn in.
@_cdecl("arbalete_ui_little_page")
public func littlePage(_ id: UInt32) -> UnsafeMutableRawPointer? {
    let address: UInt = MainActor.assumeIsolated {
        guard let page = littles[Int(id)]?.page else { return 0 }
        return UInt(bitPattern: Unmanaged.passUnretained(page).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// Takes a Little Arbalete window's page closing out of it, by CEF's view of it: only its
/// address, as CEF may have destroyed it since.
@_cdecl("arbalete_ui_little_page_closed")
public func littlePageClosed(_ view: UnsafeMutableRawPointer) {
    let address = UInt(bitPattern: view)
    MainActor.assumeIsolated {
        for little in littles.values {
            little.page.subviews
                .first { UInt(bitPattern: Unmanaged.passUnretained($0).toOpaque()) == address }?
                .removeFromSuperview()
        }
    }
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

// MARK: ⌥⌘N in any app

/// The registered ⌥⌘N and the handler that receives it.
@MainActor private var littleHotKey: EventHotKeyRef?
@MainActor private var littleHotKeyHandler: EventHandlerRef?

/// Registers ⌥⌘N to open Little Arbalete from any app, like Arc's Little Arc, or unregisters it.
/// False when another app, like Arc, already has it.
@_cdecl("arbalete_ui_little_hotkey")
public func setLittleHotKey(_ on: Bool) -> Bool {
    MainActor.assumeIsolated {
        if let key = littleHotKey {
            UnregisterEventHotKey(key)
            littleHotKey = nil
        }
        guard on else { return true }
        if littleHotKeyHandler == nil {
            var pressed = EventTypeSpec(
                eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
            InstallEventHandler(
                GetApplicationEventTarget(),
                { _, _, _ in
                    MainActor.assumeIsolated {
                        NSApp.activate(ignoringOtherApps: true)
                        showLittleBar()
                    }
                    return noErr
                }, 1, &pressed, nil, &littleHotKeyHandler)
        }
        // "ARBL", the app's own signature for its hot keys.
        let id = EventHotKeyID(signature: 0x4152_424C, id: 1)
        var key: EventHotKeyRef?
        let status = RegisterEventHotKey(
            UInt32(kVK_ANSI_N), UInt32(cmdKey | optionKey), id, GetApplicationEventTarget(), 0, &key)
        littleHotKey = key
        return status == noErr
    }
}
