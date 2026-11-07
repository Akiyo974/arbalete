// The small panels above the page: the find bar, the bar at the page's top in toolbar mode, the
// site controls, the permission prompt and the sign-in prompt.

import AppKit
import SecurityInterface
import SwiftUI

/// Finds text in the page. Each keystroke searches; Enter goes to the next match, Shift-Enter to
/// the previous one. A new search has no active match until Enter.
@MainActor
final class FindBar: ObservableObject {
    @Published var text = ""
    @Published var found: (count: Int, active: Int)?
    weak var field: NSTextField?
    private let app: AppModel

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        switch name {
        case "arbalete:show":
            text = (try? JSONDecoder().decode(String.self, from: detail)) ?? ""
            field?.stringValue = text
            found = nil
        case "arbalete:found":
            struct Found: Decodable {
                var count: Int
                var active: Int
            }
            if let detail = try? JSONDecoder().decode(Found.self, from: detail) {
                found = (detail.count, detail.active)
            }
        default:
            break
        }
    }

    func find(forward: Bool) {
        if text.isEmpty { found = nil }
        app.run("find", ["text": text, "forward": forward])
    }

    func hide() {
        app.run("hide-find")
    }
}

struct FindView: View {
    @ObservedObject var bar: FindBar

    var body: some View {
        // Like Arc's: a 15-point magnifying glass in a 26-point box, 3 points in, the field 32
        // points from the edge, and the buttons 3 points from the other.
        HStack(spacing: 0) {
            Image(systemName: "magnifyingglass").font(.system(size: 15))
                .foregroundStyle(.secondary)
                .frame(width: 26, height: 26)
                .padding(.trailing, 3)
            KeyField(
                placeholder: "Find in page", size: 13,
                attach: { bar.field = $0 },
                changed: {
                    bar.text = $0
                    bar.find(forward: true)
                },
                key: { key in
                    switch key {
                    case .enter(let shift): bar.find(forward: !shift)
                    case .escape: bar.hide()
                    default: return false
                    }
                    return true
                })
            if let found = bar.found {
                Text(found.count > 0 ? "\(found.active > 0 ? "\(found.active)" : "–")/\(found.count)" : "No results")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .fixedSize()
                    .padding(.horizontal, 6)
            }
            IconButton(symbol: "chevron.up", label: "Previous match", size: 24) {
                bar.find(forward: false)
            }
            IconButton(symbol: "chevron.down", label: "Next match", size: 24) {
                bar.find(forward: true)
            }
            IconButton(symbol: "xmark", label: "Close", size: 24) { bar.hide() }
        }
        .padding(.horizontal, 3)
        .frame(maxHeight: .infinity)
    }
}

/// A key a field passes on: Enter, with Shift or not, Escape, or the arrows.
enum FieldKey {
    case enter(shift: Bool)
    case escape
    case up
    case down
}

/// An AppKit field, for the keys SwiftUI's leaves to it.
struct KeyField: NSViewRepresentable {
    let placeholder: String
    let size: CGFloat
    let attach: (NSTextField) -> Void
    let changed: (String) -> Void
    /// Whether the field handled the key.
    let key: (FieldKey) -> Bool

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = .systemFont(ofSize: size)
        field.placeholderString = placeholder
        field.delegate = context.coordinator
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        attach(field)
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.field = self
    }

    func makeCoordinator() -> Coordinator { Coordinator(field: self) }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var field: KeyField

        init(field: KeyField) {
            self.field = field
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let control = notification.object as? NSTextField else { return }
            field.changed(control.stringValue)
        }

        func control(
            _ control: NSControl, textView: NSTextView, doCommandBy selector: Selector
        ) -> Bool {
            let shift = NSApp.currentEvent?.modifierFlags.contains(.shift) ?? false
            switch selector {
            case #selector(NSResponder.insertNewline(_:)): return field.key(.enter(shift: shift))
            case #selector(NSResponder.cancelOperation(_:)): return field.key(.escape)
            case #selector(NSResponder.moveUp(_:)): return field.key(.up)
            case #selector(NSResponder.moveDown(_:)): return field.key(.down)
            default: return false
            }
        }
    }
}

/// The bar at the page's top in toolbar mode: back, forward and reload at its left, the selected
/// tab's domain in its middle, which a click edits in the Command Bar, its site controls at its
/// right, and the page loading along its bottom.
struct PageToolbar: View {
    @ObservedObject var model: AppModel

    var body: some View {
        if let tab = model.activeTab, tab.developer == true {
            DeveloperBar(model: model, url: tab.url)
        } else {
            plain
        }
    }

    /// Like Arc's, measured: buttons 37 points apart from each end, the sidebar's toggle first
    /// while it is not docked, the pinned extensions, the site controls and Split View last.
    private var plain: some View {
        let tab = model.activeTab
        return ZStack {
            HStack(spacing: 0) {
                BarStart(model: model)
                Spacer()
                if tab != nil {
                    ForEach(model.state?.extensions ?? []) { ext in
                        PinnedExtension(model: model, extension: ext, view: "toolbar")
                    }
                    BarEnd(model: model)
                }
            }
            if let tab {
                Button {
                    model.run("edit-url")
                } label: {
                    Label(domain(tab.url), systemImage: "link")
                        .font(.system(size: 13, weight: .semibold))
                        .lineLimit(1)
                        .padding(.horizontal, 10)
                        .frame(height: 26)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(tab.url)
            }
        }
        .padding(.horizontal, 2.25)
        .frame(maxHeight: .infinity)
        // A hairline under the bar, over the page, like Arc's, which shows no loading bar.
        .overlay(alignment: .bottom) {
            Rectangle().fill(Color.ink((0x000000, 0.05), (0xFFFFFF, 0.05))).frame(height: 1)
        }
    }
}

/// A button of the bar above the page in toolbar mode, 37 points wide.
private func barTool(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
    IconButton(symbol: symbol, label: label, size: 37, symbolSize: 14, action: action)
        .frame(height: 28)
}

/// The start of the bar above the page in toolbar mode, the plain one's and the developer one's,
/// so that nothing moves as developer mode turns on or off: the sidebar's toggle while it is not
/// docked, back, forward, and reload.
struct BarStart: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let page = model.state?.page
        HStack(spacing: 0) {
            if !model.sidebarDocked {
                barTool("sidebar.left", "Toggle sidebar") { model.run("toggle-sidebar") }
            }
            barTool("arrow.left", "Back") { model.run("back") }
                .disabled(!(page?.canGoBack ?? false))
            barTool("arrow.right", "Forward") { model.run("forward") }
                .disabled(!(page?.canGoForward ?? false))
            // Like Arc's, it stops the page while it loads.
            if page?.loading == true {
                barTool("xmark", "Stop") { model.run("stop") }
            } else {
                barTool("arrow.clockwise", "Reload") { model.run("reload") }
            }
        }
    }
}

/// The end of the bar above the page in toolbar mode, the plain one's and the developer one's,
/// so that the site controls stay under their button as developer mode turns on or off: the
/// site controls and Split View.
struct BarEnd: View {
    @ObservedObject var model: AppModel

    var body: some View {
        HStack(spacing: 0) {
            SiteControlsButton(
                model: model, view: "toolbar", symbol: "slider.horizontal.2.square", symbolSize: 15
            )
            .frame(width: 37)
            barTool("rectangle.split.2x1", "Add Split View") { model.run("add-pane") }
        }
    }
}

/// The bar above a page in developer mode, like Arc's, measured: in the dark, indigo with faint
/// diagonal stripes; in the light, white with its buttons in the accent. The lock, or an "i"
/// without one, and the full address in a monospaced font, then buttons to copy it, capture the
/// page, open the developer tools, a menu of the extensions, the site controls (not in Arc's)
/// and Split View; in toolbar mode, the navigation buttons first while the sidebar is not docked.
/// On a split's page, like Arc's, the page's own, with × to close it.
struct DeveloperBar: View {
    @ObservedObject var model: AppModel
    let url: String
    /// On a split's page: it is focused before any action, and closed by ×.
    var focus: () -> Void = {}
    var close: (() -> Void)?
    /// Its address edited in place, like Arc's: selected as a click starts it.
    private let _editing = State(initialValue: false)
    private let _typed = State(initialValue: "")
    private let _focused = FocusState<Bool>()

    private var dark: Bool { isDark(model.space?.theme) }

    /// In toolbar mode, over a page alone, its end is the plain bar's.
    private var asToolbar: Bool { model.state?.toolbar == true && close == nil }
    /// Its start is the plain bar's: in toolbar mode, like Arc's, the navigation buttons go to the
    /// sidebar's top while it is docked (SidebarToolbar).
    private var navigates: Bool { asToolbar && !model.sidebarDocked }

    var body: some View {
        HStack(spacing: 2) {
            if navigates {
                BarStart(model: model)
            } else if model.state?.toolbar == true, !asToolbar {
                NavButtons(model: model)
            }
            // Like the site controls' Secure, the lock shows the site's certificate.
            if url.hasPrefix("https://") {
                Button {
                    focus()
                    model.run("show-certificate")
                } label: {
                    Image(systemName: "lock.fill")
                        .font(.system(size: 13, weight: .bold))
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Show Certificate")
            } else {
                Image(systemName: "info")
                    .font(.system(size: 13, weight: .bold))
                    .frame(width: 24)
            }
            address
                .font(.system(size: 13, design: .monospaced))
                .foregroundStyle(dark ? Color.white : Ink.primary)
                .padding(.leading, 6)
            Spacer(minLength: 8)
            tool("link", "Copy URL") { model.run("choose-menu", "site", "copy-link") }
            separator
            // Like Arc's: Portrait Mode, whose panel shows under it, then a part of the page.
            GeometryReader { geometry in
                tool("photo.artframe", "Capture in Portrait Mode") {
                    nativeWindows[model.window]?.capture.button = geometry.frame(in: .global)
                    model.run("capture")
                }
            }
            .frame(width: 28, height: 28)
            tool("camera", "Capture Part of the Page") { model.run("capture-area") }
            separator
            tool("apple.terminal", "Console") { model.run("dev-tools-panel", "console") }
            tool("network", "Network") { model.run("dev-tools-panel", "network") }
            tool("scope", "Inspect Element") { model.run("dev-tools-panel", "elements") }
            separator
            // Like Arc's, a menu of the extensions in place of the pinned ones.
            GeometryReader { geometry in
                tool("puzzlepiece.extension", "Extensions") {
                    model.run("extensions-menu", commandButton("toolbar", inMainWindow(geometry.frame(in: .global))))
                }
            }
            .frame(width: 28, height: 28)
            // The site controls stay, between the extensions and Split View.
            if asToolbar {
                BarEnd(model: model)
            } else {
                SiteControlsButton(model: model, view: "toolbar")
                tool("rectangle.split.2x1", "Add Split View") { model.run("add-pane") }
            }
            if let close {
                IconButton(symbol: "xmark", label: "Close", size: 28, symbolSize: 15, action: close)
            }
        }
        .foregroundStyle(dark ? Color.white : model.accent)
        .padding(.leading, navigates ? 2.25 : 10)
        .padding(.trailing, asToolbar ? 2.25 : 10)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background {
            if dark { stripes } else { color("#f8f8fd") }
        }
        .overlay(alignment: .bottom) {
            if !dark { Rectangle().fill(.black.opacity(0.08)).frame(height: 1) }
        }
        .environment(\.colorScheme, dark ? .dark : .light)
    }

    /// The full address, which a click edits in place, like Arc's: Enter loads what is typed,
    /// Escape or a click elsewhere gives it up.
    @ViewBuilder private var address: some View {
        if _editing.wrappedValue {
            TextField("", text: _typed.projectedValue)
                .textFieldStyle(.plain)
                .focused(_focused.projectedValue)
                .onSubmit {
                    let typed = _typed.wrappedValue.trimmingCharacters(in: .whitespaces)
                    _editing.wrappedValue = false
                    guard !typed.isEmpty, typed != url else { return }
                    let engine = nativeWindows[model.window]?.commandBar.search.url
                        ?? "https://www.google.com/search?q="
                    let target = toURL(typed) ?? engine + percentEncoded(typed)
                    focus()
                    model.run("navigate", target)
                }
                .onExitCommand { _editing.wrappedValue = false }
                .onChange(of: _focused.wrappedValue) { _, focused in
                    if !focused { _editing.wrappedValue = false }
                }
                .onAppear { _focused.wrappedValue = true }
        } else {
            Text(url)
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
                .onTapGesture {
                    _typed.wrappedValue = url
                    _editing.wrappedValue = true
                }
                .help(url)
        }
    }

    private var stripes: some View {
            Canvas { context, size in
                context.fill(Path(CGRect(origin: .zero, size: size)), with: .color(color("#5459d8")))
                // Stripes 12 points wide, every 24, leaning left, like Arc's.
                var x: CGFloat = -size.height
                while x < size.width {
                    var stripe = Path()
                    stripe.move(to: CGPoint(x: x, y: 0))
                    stripe.addLine(to: CGPoint(x: x + size.height, y: size.height))
                    stripe.addLine(to: CGPoint(x: x + size.height + 12, y: size.height))
                    stripe.addLine(to: CGPoint(x: x + 12, y: 0))
                    context.fill(stripe, with: .color(color("#5055d7")))
                    x += 24
                }
            }
    }

    private var separator: some View {
        Rectangle().fill(dark ? Color.white.opacity(0.2) : Color.black.opacity(0.1))
            .frame(width: 1, height: 18).padding(.horizontal, 6)
    }

    private func tool(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
        IconButton(symbol: symbol, label: label, size: 28, symbolSize: 15) {
            focus()
            action()
        }
    }
}

/// The selected tab's site controls, under their button (show_site_controls in
/// app/src/shell.rs): actions on its link and what the site keeps, the profile's extensions,
/// whose popup a click opens in place of the panel, the Chrome Web Store to add more, the library
/// to manage them, and whether the connection is secure. It closes on losing the focus and on
/// Escape, and sizes itself to what it shows.
@MainActor
final class SiteControls: ObservableObject {
    struct Site: Decodable {
        struct SiteExtension: Decodable, Identifiable {
            var id: String
            var name: String
            var icon: String?
            var popup: Bool
            var enabled: Bool
            var pinned: Bool
        }
        var url: String
        /// The url's host, and whether its tabs are kept awake while tabs are suspended.
        var site: String
        var awake: Bool
        /// Its site is in developer mode.
        var developer: Bool
        var suspends: Bool
        var extensions: [SiteExtension]
        /// What the site was allowed or blocked, by the content setting that keeps it.
        struct SitePermission: Decodable, Identifiable {
            var setting: UInt32
            var text: String
            var symbol: String
            var allowed: Bool
            var id: UInt32 { setting }
        }
        var permissions: [SitePermission]
    }

    @Published var site: Site?
    let app: AppModel
    /// Points the panel's beak at the button, from its left edge.
    var beak: (CGFloat) -> Void = { _ in }

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        switch name {
        case "arbalete:show": site = try? JSONDecoder().decode(Site.self, from: detail)
        case "arbalete:beak": (try? JSONDecoder().decode(CGFloat.self, from: detail)).map(beak)
        default: break
        }
    }
}

/// The site controls' actions, like Arc's: share, copy the link (where Arc has its Boosts),
/// capture a part of the page, and capture all of it; their commands.
private let siteActions = [
    (command("choose-menu", "site", "share"), "Share", "square.and.arrow.up"),
    (command("choose-menu", "site", "copy-link"), "Copy Link", "link"),
    (command("capture-area"), "Capture", "camera"),
    (command("capture-full"), "Capture Full Page", "text.viewfinder"),
]

/// The site controls' panel, like Arc's, measured from a recording of it, in points: 236 wide,
/// its content 10 from its edges; a row of 48 by 32 tiles, 8 apart; then its sections, a
/// 14 point header over their content; and its footer under a line.
struct SiteView: View {
    @ObservedObject var controls: SiteControls

    var body: some View {
        if let site = controls.site {
            let app = controls.app
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 8) {
                    ForEach(siteActions, id: \.0) { command, label, symbol in
                        SiteTile(label: label, sunken: true) {
                            app.run("hide-site-controls")
                            app.run(command: command)
                        } content: {
                            Image(systemName: symbol).font(.system(size: 17))
                        }
                    }
                }
                SiteSection(
                    title: "Extensions", more: "Manage", action: { app.run("manage-extensions") }
                ) {
                    LazyVGrid(
                        columns: Array(repeating: GridItem(.fixed(48), spacing: 8), count: 4),
                        alignment: .leading, spacing: 8
                    ) {
                        ForEach(site.extensions) { ext in
                            SiteExtensionTile(app: app, extension: ext)
                        }
                        SiteTile(label: "Add Extensions") {
                            app.run("show-store")
                        } content: {
                            Image(systemName: "plus").font(.system(size: 15))
                        }
                    }
                }
                .padding(.top, 10.5)
                SiteSection(title: "Settings") {
                    VStack(alignment: .leading, spacing: 11) {
                        SiteSetting(
                            symbol: "hammer.fill", title: "Developer Mode", on: site.developer,
                            help: "Turn \(site.developer ? "off" : "on") Developer Mode for this site"
                        ) {
                            app.run("developer-mode", site.site, !site.developer)
                        }
                        if site.suspends {
                            SiteSetting(
                                symbol: "sun.max.fill", title: "Keep Awake", on: site.awake,
                                help: "Keep this site's tabs from being suspended"
                            ) {
                                app.run("keep-awake", site.site, !site.awake)
                            }
                        }
                    }
                }
                .padding(.top, 15.5)
                if !site.permissions.isEmpty {
                    SiteSection(
                        title: "Permissions", more: "Reset",
                        action: { app.run("reset-site-permissions") }
                    ) {
                        VStack(alignment: .leading, spacing: 11) {
                            ForEach(site.permissions) { permission in
                                SiteSetting(
                                    symbol: permission.symbol, title: permission.text,
                                    on: permission.allowed,
                                    help: permission.allowed ? "Block" : "Allow"
                                ) {
                                    app.run(
                                        "set-site-permission", permission.setting, !permission.allowed)
                                }
                            }
                        }
                    }
                    .padding(.top, 15.5)
                }
                Divider().padding(.top, 15.5)
                HStack(spacing: 0) {
                    let secure = site.url.hasPrefix("https://")
                    // Like Arc's, it shows the site's certificate.
                    Button {
                        app.run("show-certificate")
                    } label: {
                        HStack(spacing: 9) {
                            Image(systemName: secure ? "lock.fill" : "lock.open.fill")
                                .font(.system(size: 13))
                            Text(secure ? "Secure" : "Not Secure")
                                .font(.system(size: 14, weight: .medium))
                        }
                        .padding(.horizontal, 10)
                        .frame(height: 32)
                        .background(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .fill(.primary.opacity(0.08)))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(!secure)
                    .help(secure ? "Show Certificate" : "")
                    Spacer()
                    SiteMore(app: app)
                }
                .padding(.top, 10)
            }
            .padding(.horizontal, 10)
            .padding(.top, 10)
            .padding(.bottom, 9)
            .fixedSize(horizontal: false, vertical: true)
            .background(
                GeometryReader { geometry in
                    Color.clear.onAppear { report(geometry.size.height) }
                        .onChange(of: geometry.size.height) { _, height in report(height) }
                })
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    /// Tells the app the panel's height, with its beak, which it fits it to.
    private func report(_ height: CGFloat) {
        controls.app.run("site-controls-height", Int((height + Surface.beakHeight).rounded(.up)))
    }
}

/// The site controls' "…" button, like Arc's: a round button that pulls down what the site keeps,
/// the extensions and the site's settings. Arc's Boosts have no counterpart here.
struct SiteMore: View {
    let app: AppModel
    private let hovered = State(initialValue: false)

    var body: some View {
        Menu {
            Button("Clear Cache") { run(command("choose-menu", "site", "clear-cache")) }
            Button("Clear Cookies") { run(command("choose-menu", "site", "clear-cookies")) }
            Divider()
            Button("Manage Extensions…") { run(command("manage-extensions")) }
            Button("Add Extension…") { run(command("show-store")) }
            Divider()
            Button("All Site Settings…") { run(command("show-site-settings")) }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 32, height: 32)
                .contentShape(Circle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .background(Circle().fill(.primary.opacity(hovered.wrappedValue ? 0.1 : 0)))
        .onHover { hovered.wrappedValue = $0 }
        .help("More")
    }

    /// Runs `command` in place of the site controls.
    private func run(_ command: String) {
        app.run("hide-site-controls")
        app.run(command: command)
    }
}

/// A section of the site controls, like Arc's: its title over its content, and a button beside
/// the title while the pointer is over the section.
struct SiteSection<Content: View>: View {
    let title: String
    var more: String?
    var action: () -> Void = {}
    @ViewBuilder let content: () -> Content
    private let hovered = State(initialValue: false)

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(title).font(.system(size: 14, weight: .bold))
                Spacer()
                if let more {
                    Button(more, action: action)
                        .buttonStyle(.plain)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .opacity(hovered.wrappedValue ? 1 : 0)
                }
            }
            content()
        }
        .contentShape(Rectangle())
        .onHover { over in
            withAnimation(.easeOut(duration: 0.1)) { hovered.wrappedValue = over }
        }
    }
}

/// An extension's tile in the site controls, like Arc's, measured from a recording: a click
/// opens its popup, a right click its menu, and a 14-point badge 4 points inside its bottom right
/// corner pins it by the address or unpins it. The badge shows while it is pinned, light; and
/// otherwise, grey, once the pointer has stayed over the tile 0.35 s, fading in over 0.1 s.
struct SiteExtensionTile: View {
    let app: AppModel
    let `extension`: SiteControls.Site.SiteExtension
    private let hovered = State(initialValue: false)
    private let waiting = State<DispatchWorkItem?>(initialValue: nil)

    var body: some View {
        let ext = `extension`
        SiteTile(label: ext.enabled ? ext.name : "\(ext.name) (off)") {
            app.run("open-extension-popup", ext.id)
        } content: {
            Favicon(url: ext.icon, size: 16)
                .opacity(ext.enabled ? 1 : 0.4)
        }
        .disabled(!ext.popup || !ext.enabled)
        .overlay(alignment: .bottomTrailing) {
            if ext.pinned || hovered.wrappedValue {
                Button {
                    app.run("pin-extension", ext.id, !ext.pinned)
                } label: {
                    Image(systemName: "pin.fill")
                        .font(.system(size: 7.5, weight: .bold))
                        .rotationEffect(.degrees(45))
                        .foregroundStyle(ext.pinned ? Color.black.opacity(0.8) : .black.opacity(0.45))
                        .frame(width: 14, height: 14)
                        .background(Circle().fill(ext.pinned ? .white.opacity(0.9) : .white.opacity(0.3)))
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .help(ext.pinned ? "Unpin" : "Pin")
                // Its center 4 points inside the corner.
                .offset(x: 3, y: 3)
                .transition(.opacity)
            }
        }
        .rightClick { app.run("extension-menu", ext.id) }
        .onHover { over in
            waiting.wrappedValue?.cancel()
            guard over else {
                withAnimation(.easeOut(duration: 0.1)) { hovered.wrappedValue = false }
                return
            }
            let show = DispatchWorkItem {
                withAnimation(.easeOut(duration: 0.1)) { hovered.wrappedValue = true }
            }
            waiting.wrappedValue = show
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.35, execute: show)
        }
    }
}

/// A tile of the site controls, like Arc's: the actions' are sunken, the extensions' flat.
struct SiteTile<Content: View>: View {
    let label: String
    var sunken = false
    let action: () -> Void
    @ViewBuilder let content: () -> Content
    @Environment(\.colorScheme) private var scheme
    private let hovered = State(initialValue: false)

    var body: some View {
        Button(action: action) {
            content()
                .frame(width: 48, height: 32)
                .background(background)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered.wrappedValue = $0 }
        .help(label)
    }

    private var background: some View {
        let shape = RoundedRectangle(cornerRadius: 6, style: .continuous)
        let dark = scheme == .dark
        let lit = hovered.wrappedValue ? 0.05 : 0
        return Group {
            if sunken {
                // Darker than the panel, its top edge catching the light.
                shape.fill(
                    dark
                        ? LinearGradient(
                            colors: [Color(white: 0.13 + lit), Color(white: 0.07 + lit)],
                            startPoint: .top, endPoint: .bottom)
                        : LinearGradient(
                            colors: [.white.opacity(0.85 + lit), .white.opacity(0.7 + lit)],
                            startPoint: .top, endPoint: .bottom)
                )
                .shadow(color: .black.opacity(dark ? 0.35 : 0.1), radius: 1.5, y: 1)
                    .overlay(
                        shape.strokeBorder(
                            LinearGradient(
                                colors: [.white.opacity(dark ? 0.15 : 0.6), .clear],
                                startPoint: .top, endPoint: .center),
                            lineWidth: 1))
            } else {
                shape.fill(.primary.opacity(0.08 + lit))
            }
        }
    }
}

/// A setting of the site, like Arc's: its symbol in a circle, light while it is on, over its name
/// and state.
struct SiteSetting: View {
    let symbol: String
    let title: String
    let on: Bool
    let help: String
    let action: () -> Void
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                Image(systemName: symbol)
                    .font(.system(size: 14))
                    .foregroundStyle(on ? (scheme == .dark ? Color.black : Color.white) : Color.primary)
                    .frame(width: 32, height: 32)
                    .background(Circle().fill(.primary.opacity(on ? 0.88 : 0.25)))
                VStack(alignment: .leading, spacing: 0) {
                    Text(title).font(.system(size: 14, weight: .semibold)).lineLimit(1)
                    Text(on ? "On" : "Off")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
    }
}

/// A site's permission request, which the prompt asks the user about (sync_permission_prompt in
/// app/src/shell.rs).
@MainActor
final class PermissionPrompt: ObservableObject {
    struct Request: Decodable, Equatable {
        struct Ask: Decodable, Hashable {
            var text: String
            var symbol: String
        }
        var id: UInt64
        var site: String
        var asks: [Ask]
    }

    @Published var request: Request?
    /// When the request showed, before which its answers do nothing, like Chromium's: a page
    /// cannot have a click it asked for land on Allow as the prompt appears under it.
    private var shown = Date.distantPast
    let app: AppModel

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        guard name == "arbalete:permission" else { return }
        let request = try? JSONDecoder().decode(Request.self, from: detail)
        // The app sends it again after each command: only a new one changes the view.
        if request != self.request {
            self.request = request
            shown = Date()
        }
    }

    /// Gives the site the user's answer, once the request has shown for half a second.
    func answer(_ allow: Bool) {
        guard let request, Date().timeIntervalSince(shown) >= 0.5 else { return }
        app.run("answer-permission", request.id, allow)
    }
}

/// The permission prompt, like Arc's: a small panel at the top of the page, with the site, what
/// it asks to use, and Block and Allow.
struct PermissionView: View {
    @ObservedObject var prompt: PermissionPrompt

    var body: some View {
        if let request = prompt.request {
            VStack(alignment: .leading, spacing: 12) {
                // Cut at the head, a long site keeps its end, the domain it belongs to, in view.
                (Text(request.site).fontWeight(.semibold) + Text(" wants to"))
                    .font(.system(size: 14))
                    .lineLimit(2)
                    .truncationMode(.head)
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(request.asks, id: \.self) { ask in
                        HStack(spacing: 9) {
                            Image(systemName: ask.symbol)
                                .font(.system(size: 13))
                                .foregroundStyle(.secondary)
                                .frame(width: 20)
                            Text(ask.text).font(.system(size: 13))
                        }
                    }
                }
                HStack(spacing: 8) {
                    PermissionButton(title: "Block", prominent: false) { prompt.answer(false) }
                    PermissionButton(title: "Allow", prominent: true) { prompt.answer(true) }
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .background(
                GeometryReader { geometry in
                    Color.clear.onAppear { report(geometry.size.height) }
                        .onChange(of: geometry.size.height) { _, height in report(height) }
                })
            .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    /// Tells the app the prompt's height, which it fits it to.
    private func report(_ height: CGFloat) {
        prompt.app.run("permission-prompt-height", Int(height.rounded(.up)))
    }
}

/// One of the permission prompt's answers, half its width.
struct PermissionButton: View {
    let title: String
    let prominent: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(prominent ? Color.white : Color.primary)
                .frame(maxWidth: .infinity)
                .frame(height: 28)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(prominent ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.primary.opacity(0.08))))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// A site's or a proxy's request to sign in with HTTP authentication, which the sign-in prompt
/// asks the user about (sync_auth_prompt in app/src/shell.rs). What the user types goes to the app
/// alone, which hands it to Chromium.
@MainActor
final class AuthPrompt: ObservableObject {
    struct Request: Decodable, Equatable {
        var id: UInt64
        var host: String
        var realm: String
        var proxy: Bool
    }

    @Published var request: Request?
    @Published var user = ""
    @Published var password = ""
    let app: AppModel

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        guard name == "arbalete:auth" else { return }
        let request = try? JSONDecoder().decode(Request.self, from: detail)
        // The app sends it again after each command: only a new one empties the fields.
        if request != self.request {
            self.request = request
            (user, password) = ("", "")
        }
    }

    /// Signs in with what the user typed, or, `false`, turns the request down.
    func answer(_ signIn: Bool) {
        guard let request else { return }
        if signIn {
            app.run("answer-auth", request.id, [user, password])
        } else {
            app.run("answer-auth", request.id, NSNull())
        }
        (user, password) = ("", "")
    }
}

/// The sign-in prompt, like the permission prompt: a small panel at the top of the page, with the
/// site, or the proxy, and its realm, a user name and a password, and Cancel and Sign In.
struct AuthView: View {
    @ObservedObject var prompt: AuthPrompt
    @FocusState private var naming: Bool

    var body: some View {
        if let request = prompt.request {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(request.proxy ? "Sign in to the proxy" : "Sign in to")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                    // Cut at the head, a long host keeps its end, the domain it belongs to, in view.
                    Text(request.host)
                        .font(.system(size: 14, weight: .semibold))
                        .lineLimit(1)
                        .truncationMode(.head)
                    if !request.realm.isEmpty {
                        Text("“\(request.realm)”")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                }
                VStack(spacing: 8) {
                    TextField("User name", text: $prompt.user)
                        .focused($naming)
                    SecureField("Password", text: $prompt.password)
                }
                .textFieldStyle(.roundedBorder)
                .font(.system(size: 13))
                .onSubmit { prompt.answer(true) }
                HStack(spacing: 8) {
                    PermissionButton(title: "Cancel", prominent: false) { prompt.answer(false) }
                        .keyboardShortcut(.cancelAction)
                    PermissionButton(title: "Sign In", prominent: true) { prompt.answer(true) }
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .background(
                GeometryReader { geometry in
                    Color.clear.onAppear { report(geometry.size.height) }
                        .onChange(of: geometry.size.height) { _, height in report(height) }
                })
            .frame(maxHeight: .infinity, alignment: .top)
            .onAppear { naming = true }
            .onChange(of: request.id) { naming = true }
        }
    }

    /// Tells the app the prompt's height, which it fits it to.
    private func report(_ height: CGFloat) {
        prompt.app.run("auth-prompt-height", Int(height.rounded(.up)))
    }
}

/// Shows a site's certificate on the window `id`, like Arc's: macOS's certificate sheet, with the
/// certificates that issued it above it. `certificates` are DER encoded, the site's first.
@_cdecl("arbalete_ui_certificate")
public func showCertificate(
    _ id: UInt32, _ certificates: UnsafePointer<UnsafePointer<UInt8>>,
    _ sizes: UnsafePointer<Int>, _ count: Int
) {
    MainActor.assumeIsolated {
        guard let window = nativeWindows[id]?.window else { return }
        let chain = (0..<count).compactMap {
            SecCertificateCreateWithData(nil, Data(bytes: certificates[$0], count: sizes[$0]) as CFData)
        }
        // Evaluated, the trust holds the whole chain up to its root, which the site does not send,
        // like Arc's sheet shows it, and whether it is valid.
        var trust: SecTrust?
        guard !chain.isEmpty,
            SecTrustCreateWithCertificates(chain as CFArray, SecPolicyCreateSSL(true, nil), &trust)
                == errSecSuccess, let trust
        else { return }
        SecTrustEvaluateAsyncWithError(trust, .main) { trust, _, _ in
            MainActor.assumeIsolated {
                SFCertificatePanel.shared().beginSheet(
                    for: window, modalDelegate: nil, didEnd: nil, contextInfo: nil, trust: trust,
                    showGroup: true)
            }
        }
    }
}
