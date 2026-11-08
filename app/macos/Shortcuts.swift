// The settings' Shortcuts pane, like Arc's: every shortcut of the menus in a searchable list of key
// caps, the picked one's keys, what it does and who takes it first beside it; a click on a key cap
// records new keys. The menus are the shortcuts' source: the app names each changeable item and
// keeps the changes in its settings (mac::apply_shortcuts in app/src/mac.rs).

import AppKit
import SwiftUI

/// A menu item's keys, as the settings keep them: its key equivalent and AppKit's modifier mask.
struct Keys: Codable, Equatable {
    var key: String
    var modifiers: UInt

    var flags: NSEvent.ModifierFlags { NSEvent.ModifierFlags(rawValue: modifiers) }
    var isEmpty: Bool { key.isEmpty }

    /// The keys as a menu shows them, like "⇧⌘K"; "---" for none, like Arc's.
    var glyphs: String { isEmpty ? "---" : modifierGlyphs(flags, key: key).joined() + keyGlyph(key) }

    /// Each key apart, the modifiers in the menus' order, then the key, for the large tiles.
    var tiles: [String] { isEmpty ? [] : modifierGlyphs(flags, key: key) + [keyGlyph(key)] }

    /// Two keys are the same shortcut whatever the case of their key: an uppercase key adds ⇧.
    func same(_ other: Keys) -> Bool {
        !isEmpty && normalized == other.normalized
    }

    private var normalized: String {
        let shift = key != key.lowercased() || flags.contains(.shift)
        let mask = flags.intersection([.command, .control, .option]).rawValue
        return "\(key.lowercased())|\(mask)|\(shift)"
    }
}

struct ShortcutOverride: Codable, Equatable {
    var keys: Keys?
    var priority: String?
}

private func modifierGlyphs(_ flags: NSEvent.ModifierFlags, key: String) -> [String] {
    var glyphs: [String] = []
    if flags.contains(.control) { glyphs.append("⌃") }
    if flags.contains(.option) { glyphs.append("⌥") }
    if flags.contains(.shift) || key != key.lowercased() { glyphs.append("⇧") }
    if flags.contains(.command) { glyphs.append("⌘") }
    return glyphs
}

private func keyGlyph(_ key: String) -> String {
    guard let scalar = key.unicodeScalars.first else { return "" }
    switch Int(scalar.value) {
    case NSUpArrowFunctionKey: return "↑"
    case NSDownArrowFunctionKey: return "↓"
    case NSLeftArrowFunctionKey: return "←"
    case NSRightArrowFunctionKey: return "→"
    case NSF1FunctionKey...NSF20FunctionKey: return "F\(Int(scalar.value) - NSF1FunctionKey + 1)"
    case 0x1B: return "⎋"
    case 0x09: return "⇥"
    case 0x0D: return "↩"
    case 0x20: return "Space"
    case 0x7F, 0x08: return "⌫"
    default: return key.uppercased()
    }
}

/// A changeable shortcut of the menus.
struct Shortcut: Identifiable {
    var id: String
    var title: String
    var keys: Keys
    var defaults: Keys
    var priority: String
    var changed: Bool { keys != defaults }
}

/// The menus' changeable shortcuts, which the app named, with their keys now: the settings'
/// changes, or the defaults.
@MainActor func menuShortcuts(_ detail: SettingsDetail) -> [Shortcut] {
    var shortcuts: [Shortcut] = []
    var menus = NSApp.mainMenu.map { [$0] } ?? []
    while let menu = menus.popLast() {
        for item in menu.items {
            if let submenu = item.submenu {
                menus.append(submenu)
                continue
            }
            let id = item.identifier?.rawValue ?? ""
            guard let defaults = detail.shortcutDefaults[id] else { continue }
            // From the settings rather than the menu, whose keys macOS adapts to the keyboard
            // layout, like ⌘0 shown ⌘À on a French one.
            let keys = detail.settings.shortcuts[id]?.keys ?? defaults
            let title = item.isHidden ? "\(item.title) (Alternate)" : item.title
            let priority =
                detail.settings.shortcuts[id]?.priority ?? detail.shortcutPriorities[id] ?? "websites"
            shortcuts.append(
                Shortcut(id: id, title: title, keys: keys, defaults: defaults, priority: priority))
        }
    }
    return shortcuts.sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
}

/// What a shortcut does, in a sentence, by its name; the menu item's title stands for the others.
private let shortcutDescriptions: [String: String] = [
    "newTab": "Opens the Command Bar to search the web or open a tab.",
    "newWindow": "Opens a new window on the Command Bar.",
    "newIncognito": "Opens a window whose browsing is forgotten once it closes.",
    "newLittle": "Opens Little Arbalete, a small window for a quick look.",
    "openLittleInSpace": "Moves the Little Arbalete page or the Peek in front into a Space.",
    "openLocation": "Opens the Command Bar on the address of the page shown.",
    "closeTab": "Closes the tab shown, or the Peek or Little Arbalete window in front; without a tab, the window.",
    "performClose": "Closes the window in front.",
    "reopenTab": "Opens again the tab closed last.",
    "togglePin": "Pins the tab shown, or unpins it.",
    "print": "Prints the page shown, or saves it as a PDF.",
    "captureArea": "Captures a part of the page, to copy or save.",
    "captureFull": "Captures the whole page, scrolled to its end, to copy or save.",
    "capturePortrait": "Copies the page framed on a color, like a portrait.",
    "toggleSidebar": "Shows or hides the sidebar.",
    "toggleToolbar": "Shows or hides the toolbar above the page.",
    "addPane": "Adds a page beside the one shown, in a Split View.",
    "closePane": "Closes the Split View page in focus.",
    "nextPane": "Moves the focus to the next Split View page.",
    "previousPane": "Moves the focus to the previous Split View page.",
    "closePeek": "Closes Peek, or leaves the current context.",
    "stop": "Stops loading the page.",
    "reload": "Loads the page again.",
    "forceReload": "Loads the page again, without what the browser kept of it.",
    "actualSize": "Shows the page at its actual size.",
    "zoomIn": "Magnifies the page.",
    "zoomOut": "Shrinks the page.",
    "viewSource": "Opens the page's source in a new tab.",
    "devTools": "Opens the developer tools for the page shown.",
    "back": "Goes back to the page visited before in this tab.",
    "forward": "Goes forward to the page visited after in this tab.",
    "history": "Shows the pages visited.",
    "copyUrl": "Copies the address of the page shown.",
    "copyMarkdownUrl": "Copies the page's title and address as a Markdown link.",
    "find": "Finds words in the page shown.",
    "findNext": "Goes to the next match of the words found.",
    "findPrevious": "Goes to the previous match of the words found.",
    "pasteAndMatchStyle": "Pastes the text copied in the style of the text around it.",
    "nextTab": "Shows the next tab of the sidebar.",
    "previousTab": "Shows the previous tab of the sidebar.",
    "clearToday": "Archives the Space's Today tabs.",
    "nextSpace": "Slides to the next Space.",
    "previousSpace": "Slides to the previous Space.",
    "library": "Opens the library: media, downloads, Spaces and archived tabs.",
    "downloads": "Opens the library's downloads.",
    "settings": "Opens the settings.",
    "performMiniaturize": "Puts the window in front in the Dock.",
    "toggleFullScreen": "Lets the window fill the screen, or not.",
    "hide": "Hides Arbalete's windows.",
    "hideOtherApplications": "Hides the other apps' windows.",
    "terminate": "Quits Arbalete.",
]

private func description(_ shortcut: Shortcut, _ settings: SettingsDetail.General) -> String {
    let base = shortcut.id.replacingOccurrences(of: "-alt", with: "")
    if let text = shortcutDescriptions[base] { return text }
    if base.hasPrefix("sidebarItem-") {
        if base == "sidebarItem-9" && !settings.ninthSidebarItem {
            return "Shows the last tab, at the bottom of the sidebar."
        }
        let number = base.dropFirst("sidebarItem-".count)
        return settings.countFavorites
            ? "Shows the sidebar's item number \(number), counting the favorites first."
            : "Shows the sidebar's item number \(number), counting from the first tab under the favorites."
    }
    if base.hasPrefix("space-") {
        return "Slides to the Space number \(base.dropFirst("space-".count))."
    }
    return shortcut.title
}

/// The priorities, like Arc's: their names and what they do.
private let priorities: [(id: String, name: String, caption: String)] = [
    ("websites", "Prefer Websites", "When shortcuts conflict, the website wins."),
    (
        "both", "Use Both",
        "Press this shortcut once to use the website’s shortcut. Press it twice to use Arbalete’s shortcut."
    ),
    ("app", "Prefer Arbalete", "When shortcuts conflict, Arbalete wins."),
]

/// The pane's state: the shortcut picked, and the one whose keys are recorded.
@MainActor
final class ShortcutsModel: ObservableObject {
    @Published var picked: String?
    @Published var recording: String?
    /// The modifiers held while recording, which the tiles show.
    @Published var held: NSEvent.ModifierFlags = []
    @Published var query = ""
    private var monitor: Any?

    func record(_ id: String, save: @escaping (Keys) -> Void) {
        stop()
        picked = id
        recording = id
        held = []
        monitor = NSEvent.addLocalMonitorForEvents(matching: [.keyDown, .flagsChanged]) {
            [weak self] event in
            guard let self else { return event }
            let flags = event.modifierFlags.intersection([.command, .control, .option, .shift])
            if event.type == .flagsChanged {
                self.held = flags
                return nil
            }
            // Escape cancels, like Arc's.
            if event.keyCode == 53 && flags.isEmpty {
                self.stop()
                return nil
            }
            let function = event.modifierFlags.contains(.function)
            guard !flags.intersection([.command, .control, .option]).isEmpty || function,
                let key = event.charactersIgnoringModifiers?.lowercased(), !key.isEmpty
            else { return nil }
            save(Keys(key: key, modifiers: flags.rawValue))
            self.stop()
            return nil
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        recording = nil
        held = []
    }
}

struct ShortcutsSettings: View {
    @ObservedObject var settings: SettingsModel
    @StateObject private var model = ShortcutsModel()

    var body: some View {
        if let detail = settings.detail {
            let all = menuShortcuts(detail)
            let typed = model.query.trimmingCharacters(in: .whitespaces).lowercased()
            let found = all.filter {
                typed.isEmpty || $0.title.lowercased().contains(typed)
                    || $0.keys.glyphs.lowercased().contains(typed)
            }
            HStack(alignment: .top, spacing: 16) {
                ShortcutList(
                    settings: settings, model: model, all: all,
                    customized: found.filter(\.changed), defaults: found.filter { !$0.changed })
                ShortcutDetail(
                    settings: settings, model: model, all: all, general: detail.settings,
                    shortcut: all.first { $0.id == model.picked })
            }
            .padding(.top, 20)
            .frame(width: 556)
            .onDisappear { model.stop() }
        }
    }
}

/// The searchable list, like Arc's: the shortcuts changed first, then the others.
private struct ShortcutList: View {
    @ObservedObject var settings: SettingsModel
    @ObservedObject var model: ShortcutsModel
    let all: [Shortcut]
    let customized: [Shortcut]
    let defaults: [Shortcut]

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                TextField("Type a feature name or shortcut (like “⌘E”)", text: $model.query)
                    .textFieldStyle(.plain)
            }
            .font(.system(size: 13))
            .padding(.horizontal, 14)
            .frame(height: 40)
            .background(Color.ink((0x000000, 0.04), (0xFFFFFF, 0.06)))
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0, pinnedViews: .sectionHeaders) {
                    if !customized.isEmpty {
                        Section {
                            rows(customized)
                        } header: {
                            SectionHeader(title: "Customized Shortcuts")
                        }
                    }
                    Section {
                        rows(defaults)
                    } header: {
                        SectionHeader(title: "Default Shortcuts")
                    }
                }
            }
        }
        .frame(width: 340, height: 450)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(
            RoundedRectangle(cornerRadius: 8).strokeBorder(Color.ink((0x000000, 0.1), (0xFFFFFF, 0.1))))
    }

    @ViewBuilder private func rows(_ shortcuts: [Shortcut]) -> some View {
        ForEach(shortcuts) { shortcut in
            ShortcutRow(
                settings: settings, model: model, shortcut: shortcut,
                conflicts: all.filter { $0.id != shortcut.id && $0.keys.same(shortcut.keys) })
        }
    }
}

private struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title).font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
            .padding(.leading, 16)
            .frame(maxWidth: .infinity, minHeight: 28, alignment: .leading)
            .background(.regularMaterial)
            .overlay(alignment: .bottom) {
                Rectangle().fill(Color.ink((0x000000, 0.07), (0xFFFFFF, 0.08))).frame(height: 1)
            }
    }
}

/// A shortcut, like Arc's: its name, a red dot when its keys are another's too, and its keys on a
/// white cap, which records new ones on a click; its menu removes or resets them.
private struct ShortcutRow: View {
    @ObservedObject var settings: SettingsModel
    @ObservedObject var model: ShortcutsModel
    let shortcut: Shortcut
    let conflicts: [Shortcut]

    var body: some View {
        let picked = model.picked == shortcut.id
        let conflict = !conflicts.isEmpty
        HStack(spacing: 8) {
            Text(shortcut.title).font(.system(size: 13)).lineLimit(1)
            Spacer(minLength: 6)
            if conflict {
                ConflictDot(text: conflictText(shortcut, conflicts))
            }
            KeyCap(
                keys: shortcut.keys, recording: model.recording == shortcut.id
            ) {
                if model.recording == shortcut.id {
                    model.stop()
                } else {
                    model.record(shortcut.id) { keys in
                        settings.app.run(
                            "shortcut", ["set": [shortcut.id, keys.key, keys.modifiers]])
                    }
                }
            }
            // Drawn again with new keys, which it would otherwise keep showing the old ones of.
            .id(shortcut.keys.glyphs)
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .frame(height: 40)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(
                    picked
                        ? (conflict
                            ? Color(red: 0.25, green: 0.13, blue: 0.14)
                            : Color(red: 0.09, green: 0.13, blue: 0.19))
                        : .clear)
        )
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .contentShape(Rectangle())
        .onTapGesture { model.picked = shortcut.id }
        .contextMenu {
            Button("Remove Shortcut") { settings.app.run("shortcut", ["remove": shortcut.id]) }
                .disabled(shortcut.keys.isEmpty)
            Button("Reset Shortcut to Default") { settings.app.run("shortcut", ["reset": shortcut.id]) }
                .disabled(!shortcut.changed)
            if conflict {
                Divider()
                Button(conflicts.count == 1 ? "Remove Conflicting Shortcut" : "Remove Conflicting Shortcuts") {
                    for other in conflicts { settings.app.run("shortcut", ["remove": other.id]) }
                }
            }
        }
        .overlay(alignment: .bottom) {
            Rectangle().fill(Color.ink((0x000000, 0.07), (0xFFFFFF, 0.08))).frame(height: 1)
                .padding(.leading, 24)
        }
    }
}

private func conflictText(_ shortcut: Shortcut, _ conflicts: [Shortcut]) -> String {
    let first = "\(shortcut.keys.glyphs) is already in use by “\(conflicts[0].title)”"
    switch conflicts.count {
    case 1: return first
    case 2: return first + " and one other action"
    default: return first + " and \(conflicts.count - 1) other actions"
    }
}

/// The red dot of a shortcut whose keys are another's too, like Arc's: hovered, it says whose in
/// a dark capsule above it, in a window of its own so that nothing of the list covers it.
private struct ConflictDot: View {
    let text: String
    private let anchor = State(initialValue: NSView())

    var body: some View {
        Image(systemName: "exclamationmark.circle.fill")
            .font(.system(size: 12))
            .foregroundStyle(Color(red: 0.96, green: 0.21, blue: 0.22))
            .background(Anchor(view: anchor.wrappedValue))
            .onHover { over in
                if over { CapsuleTip.shared.show(text, over: anchor.wrappedValue) } else { CapsuleTip.shared.hide() }
            }
            .onDisappear { CapsuleTip.shared.hide() }
    }

    private struct Anchor: NSViewRepresentable {
        let view: NSView
        func makeNSView(context: Context) -> NSView { view }
        func updateNSView(_ view: NSView, context: Context) {}
    }
}

/// A dark capsule over a view, like Arc's tooltips.
@MainActor
private final class CapsuleTip {
    static let shared = CapsuleTip()
    private let panel: NSPanel = {
        let panel = NSPanel(
            contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
            defer: true)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.level = .popUpMenu
        return panel
    }()

    func show(_ text: String, over view: NSView) {
        guard let window = view.window else { return }
        let host = NSHostingView(
            rootView: Text(text).font(.system(size: 13, weight: .medium))
                .foregroundStyle(.white)
                .lineLimit(1)
                .fixedSize()
                .padding(.horizontal, 12)
                .frame(height: 28)
                .background(Capsule().fill(Color(white: 0.17)))
                .overlay(Capsule().strokeBorder(Color.white.opacity(0.1), lineWidth: 0.5))
                .shadow(color: .black.opacity(0.3), radius: 6, y: 2)
                .padding(8))
        let size = host.fittingSize
        let dot = window.convertToScreen(view.convert(view.bounds, to: nil))
        // Measured on Arc's: centred on the dot, its bottom 6 points over the dot's top.
        panel.setFrame(
            NSRect(x: dot.midX - size.width / 2, y: dot.maxY + 6 - 8, width: size.width, height: size.height),
            display: false)
        panel.contentView = host
        window.addChildWindow(panel, ordered: .above)
    }

    func hide() {
        panel.parent?.removeChildWindow(panel)
        panel.orderOut(nil)
    }
}

/// A shortcut's keys on a white cap, like Arc's: a blue edge under the pointer, blue keys while
/// it records, in a halo that comes and goes.
private struct KeyCap: View {
    let keys: Keys
    let recording: Bool
    let action: () -> Void
    private let hovered = State(initialValue: false)

    var body: some View {
        let blue = Color(red: 0.0, green: 0.43, blue: 0.99)
        Button(action: action) {
            Text(keys.glyphs).font(.system(size: 13))
                .foregroundStyle(recording ? blue : Color(white: 0.4))
                .lineLimit(1)
                .padding(.horizontal, 8)
                .frame(minWidth: 62, minHeight: 24)
                .background(
                    RoundedRectangle(cornerRadius: 5)
                        .fill(hovered.wrappedValue && !recording ? Color(red: 0.94, green: 1, blue: 1) : .white)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 5)
                        .strokeBorder(
                            recording
                                ? blue
                                : hovered.wrappedValue
                                    ? Color(red: 0.6, green: 0.68, blue: 0.8) : Color(white: 0.2),
                            lineWidth: recording ? 2 : hovered.wrappedValue ? 1 : 0.5))
                .background {
                    if recording { Halo(color: blue) }
                }
                .animation(.easeOut(duration: 0.15), value: hovered.wrappedValue)
        }
        .buttonStyle(.plain)
        .onHover { hovered.wrappedValue = $0 }
    }
}

/// The halo around a cap recording keys, like Arc's: a soft blue ring fading in and out.
private struct Halo: View {
    let color: Color
    private let shown = State(initialValue: false)

    var body: some View {
        RoundedRectangle(cornerRadius: 8)
            .strokeBorder(color.opacity(0.35), lineWidth: 3.5)
            .padding(-3.5)
            .opacity(shown.wrappedValue ? 1 : 0)
            .onAppear {
                withAnimation(.easeInOut(duration: 0.7).repeatForever(autoreverses: true)) {
                    shown.wrappedValue = true
                }
            }
    }
}

/// Beside the list, like Arc's: the picked shortcut's keys as large tiles, its name, what it does
/// and who takes it first, then, for ⌘9, which item it goes to; without one picked, what
/// shortcuts are for.
private struct ShortcutDetail: View {
    @ObservedObject var settings: SettingsModel
    @ObservedObject var model: ShortcutsModel
    let all: [Shortcut]
    let general: SettingsDetail.General
    let shortcut: Shortcut?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let shortcut {
                let conflict = all.contains { $0.id != shortcut.id && $0.keys.same(shortcut.keys) }
                let recording = model.recording == shortcut.id
                let tiles =
                    recording && !model.held.isEmpty
                    ? modifierGlyphs(model.held, key: "") : shortcut.keys.tiles
                KeyTiles(tiles: tiles, conflict: conflict && !recording)
                Text(shortcut.title).font(.system(size: 15, weight: .bold))
                    .padding(.top, 22)
                Text(description(shortcut, general)).font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .lineSpacing(2)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 6)
                if !shortcut.keys.isEmpty {
                    Rectangle().fill(Color.ink((0x000000, 0.07), (0xFFFFFF, 0.08))).frame(height: 1)
                        .padding(.vertical, 18)
                    Picker(
                        "",
                        selection: Binding(
                            get: { shortcut.priority },
                            set: { settings.app.run("shortcut", ["priority": [shortcut.id, $0]]) })
                    ) {
                        ForEach(priorities, id: \.id) { Text($0.name).tag($0.id) }
                    }
                    .labelsHidden()
                    .frame(width: 143)
                    Text(priorities.first { $0.id == shortcut.priority }?.caption ?? "")
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 10)
                }
                // Like Arc's: ⌘9 goes to the last item or to the ninth, and only then may leave
                // the favorites out of the count.
                if shortcut.id == "sidebarItem-9" {
                    VStack(alignment: .leading, spacing: 8) {
                        Toggle("Go to the ninth item.", isOn: binding(general.ninthSidebarItem, "ninth-sidebar-item"))
                        if general.ninthSidebarItem {
                            Toggle("Count the favorites first.", isOn: binding(general.countFavorites, "count-favorites"))
                        }
                    }
                    .toggleStyle(.checkbox)
                    .font(.system(size: 12))
                    .padding(.top, 14)
                }
                Spacer(minLength: 12)
                HStack(spacing: 4) {
                    Button("Reset Shortcut to Default") {
                        settings.app.run("shortcut", ["reset": shortcut.id])
                    }
                    .disabled(!shortcut.changed)
                    MoreMenu(settings: settings, all: all)
                }
            } else {
                HStack(spacing: 3) {
                    KeyTile(glyph: "⌘", conflict: false, size: 65)
                    KeyTile(glyph: "☺︎", conflict: false, size: 65)
                }
                Text("Custom Shortcuts").font(.system(size: 15, weight: .bold))
                    .padding(.top, 26)
                Text("Change shortcuts for your favorite actions, and even choose whether Arbalete or a website takes priority.")
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 8)
                Spacer(minLength: 12)
                HStack(spacing: 4) {
                    Button("Reset All Shortcuts") { settings.app.run("shortcut", "reset-all") }
                        .disabled(!all.contains(where: \.changed))
                    MoreMenu(settings: settings, all: all)
                }
            }
        }
        .frame(width: 200, height: 450, alignment: .topLeading)
    }

    private func binding(_ value: Bool, _ key: String) -> Binding<Bool> {
        Binding(get: { value }, set: { settings.app.run("set-setting", key, $0) })
    }
}

/// The "…" menu, like Arc's: resetting every shortcut.
private struct MoreMenu: View {
    @ObservedObject var settings: SettingsModel
    let all: [Shortcut]

    var body: some View {
        Menu {
            Button("Reset All Shortcuts") { settings.app.run("shortcut", "reset-all") }
                .disabled(!all.contains(where: \.changed))
        } label: {
            Image(systemName: "ellipsis").font(.system(size: 11, weight: .bold))
                .frame(width: 24, height: 24)
                .background(Circle().fill(Color.ink((0x000000, 0.06), (0xFFFFFF, 0.1))))
                .contentShape(Circle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
    }
}

/// The keys as large blue keycaps, like Arc's: 65 points each up to three, smaller beyond to fit
/// 200 points; red for keys another shortcut has too, two dim empty ones for none.
private struct KeyTiles: View {
    let tiles: [String]
    let conflict: Bool

    var body: some View {
        let size: CGFloat = tiles.count <= 3 ? 65 : min(65, (200 - CGFloat(tiles.count - 1) * 4) / CGFloat(tiles.count))
        HStack(spacing: tiles.count <= 3 ? 3 : 4) {
            if tiles.isEmpty {
                KeyTile(glyph: "", conflict: false, size: 65).opacity(0.3)
                KeyTile(glyph: "", conflict: false, size: 65).opacity(0.3)
            } else {
                ForEach(Array(tiles.enumerated()), id: \.offset) { _, glyph in
                    KeyTile(glyph: glyph, conflict: conflict, size: size)
                }
            }
        }
        .frame(height: 65, alignment: .leading)
        .modifier(Shake(trigger: conflict))
    }
}

/// A large keycap: a blue base, a lighter face, a bold blue glyph; red ones for a conflict.
private struct KeyTile: View {
    let glyph: String
    let conflict: Bool
    let size: CGFloat

    var body: some View {
        let base = conflict ? Color(red: 0.8, green: 0.2, blue: 0.2) : Color(red: 0.04, green: 0.37, blue: 0.83)
        let face =
            conflict
            ? [Color(red: 0.98, green: 0.82, blue: 0.82), Color(red: 0.93, green: 0.75, blue: 0.75)]
            : [Color(red: 0.78, green: 0.86, blue: 0.98), Color(red: 0.73, green: 0.81, blue: 0.94)]
        let ink = conflict ? Color(red: 0.97, green: 0.35, blue: 0.36) : Color(red: 0.0, green: 0.43, blue: 0.99)
        ZStack(alignment: .top) {
            RoundedRectangle(cornerRadius: size * 0.16).fill(base)
            RoundedRectangle(cornerRadius: size * 0.12)
                .fill(LinearGradient(colors: face, startPoint: .top, endPoint: .bottom))
                .padding(.top, size * 0.077)
                .padding(.horizontal, size * 0.1)
                .padding(.bottom, size * 0.17)
            Text(glyph).font(.system(size: size * 0.42, weight: .bold)).foregroundStyle(ink)
                .lineLimit(1).minimumScaleFactor(0.5)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(.bottom, size * 0.1)
        }
        .frame(width: size, height: size)
    }
}

/// Shakes its content sideways once a conflict appears, like Arc's tiles.
private struct Shake: ViewModifier {
    let trigger: Bool
    private let offset = State<CGFloat>(initialValue: 0)

    func body(content: Content) -> some View {
        content
            .offset(x: offset.wrappedValue)
            .onChange(of: trigger) { _, now in
                guard now else { return }
                withAnimation(.interpolatingSpring(stiffness: 900, damping: 9)) {
                    offset.wrappedValue = 10
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                    withAnimation(.interpolatingSpring(stiffness: 900, damping: 9)) {
                        offset.wrappedValue = 0
                    }
                }
            }
    }
}
