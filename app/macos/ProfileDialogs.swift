// Arc's profile dialogs, cards over their window, which they dim: Create a new Profile, in two
// steps from the settings (its name, then the Spaces that use it), in one for a Space, from its
// profile menus. Measured on Arc's.

import AppKit
import SwiftUI

extension AppModel {
    /// Shows the Create a new Profile dialog: for `space`, it creates the profile in one step and
    /// gives it to that Space; else a second step picks the Spaces that use it. `names` are the
    /// profiles' names, which a new one cannot take.
    func newProfile(for space: Int?, names: [String]? = nil) {
        let names = names ?? state?.profiles.map(\.name) ?? []
        let spaces = state?.spaces ?? []
        showDialog { close in
            CreateProfileDialog(names: names, spaces: spaces, space: space, close: close) {
                name, spaces in
                let json = (try? JSONSerialization.data(withJSONObject: ["name": name, "spaces": spaces]))
                    .flatMap { String(data: $0, encoding: .utf8) } ?? ""
                self.run("profile/create/\(percentEncoded(json))")
            }
        }
    }
}

/// The dialog shown, one at a time.
@MainActor private var dialog: NSPanel?

/// Shows a dialog over the window in front, which it dims, until it calls the function it is given.
@MainActor func showDialog<Content: View>(_ content: (@escaping () -> Void) -> Content) {
    var front = NSApp.keyWindow ?? NSApp.mainWindow
    while let parent = front?.parent { front = parent }
    guard dialog == nil, let window = front else { return }
    let panel = DialogPanel(
        contentRect: window.frame, styleMask: [.borderless], backing: .buffered, defer: false)
    panel.isOpaque = false
    panel.backgroundColor = .clear
    let close = { [weak window, weak panel] in
        guard let panel else { return }
        window?.removeChildWindow(panel)
        panel.orderOut(nil)
        dialog = nil
    }
    panel.contentView = NSHostingView(
        rootView: ZStack {
            Color.black.opacity(0.35)
                .clipShape(RoundedRectangle(cornerRadius: 12))
            content(close)
        }
        .environment(\.colorScheme, .dark))
    window.addChildWindow(panel, ordered: .above)
    panel.makeKeyAndOrderFront(nil)
    dialog = panel
}

/// Takes the keyboard, for the dialog's field and its Escape and Return.
private final class DialogPanel: NSPanel {
    override var canBecomeKey: Bool { true }
}

private let dialogFill = color("#121828")
private let dialogBlue = color("#282ffa")
private let dialogCancel = color("#282657")

/// Arc's dialog card: deep navy, its content from the left.
struct DialogCard<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) { content }
            .padding(.horizontal, 38)
            .padding(.top, 38)
            .padding(.bottom, 28)
            .frame(width: 450, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 16).fill(dialogFill))
            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(.white.opacity(0.1)))
            .shadow(color: .black.opacity(0.4), radius: 30, y: 10)
            .foregroundStyle(.white)
    }
}

/// Its title, then what it says.
struct DialogText: View {
    let title: String
    let text: String

    var body: some View {
        Text(title).font(.system(size: 19, weight: .semibold))
        Text(text).font(.system(size: 14))
            .foregroundStyle(.white.opacity(0.7))
            .lineSpacing(2)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.top, 12)
    }
}

/// Its buttons, at the bottom right, each with its key: the one that goes back, Escape, and the one
/// that goes on, Return.
struct DialogButtons: View {
    let cancel: String
    let confirm: String
    let enabled: Bool
    let back: () -> Void
    let next: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Spacer()
            button(cancel, key: "ESC", fill: dialogCancel, action: back)
                .keyboardShortcut(.cancelAction)
            button(confirm, key: "⏎", fill: dialogBlue, action: next)
                .keyboardShortcut(.defaultAction)
                .disabled(!enabled)
                .opacity(enabled ? 1 : 0.5)
        }
        .padding(.top, 28)
    }

    private func button(_ title: String, key: String, fill: Color, action: @escaping () -> Void)
        -> some View
    {
        Button(action: action) {
            HStack(spacing: 10) {
                Text(title).font(.system(size: 14, weight: .medium))
                Text(key).font(.system(size: 10, weight: .bold))
                    .padding(.horizontal, 6)
                    .frame(height: 20)
                    .background(RoundedRectangle(cornerRadius: 4).fill(.white.opacity(0.2)))
            }
            .padding(.leading, 12)
            .padding(.trailing, 8)
            .frame(height: 36)
            .background(RoundedRectangle(cornerRadius: 6).fill(fill))
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.white.opacity(0.2)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Arc's profile icon: a smiling face with a green "+".
private struct NewProfileIcon: View {
    var body: some View {
        Image(systemName: "face.smiling")
            .font(.system(size: 34))
            .foregroundStyle(.white.opacity(0.75))
            .frame(width: 50, height: 50)
            .background(Circle().fill(color("#282752")))
            .overlay(alignment: .bottomTrailing) {
                Image(systemName: "plus.circle.fill")
                    .symbolRenderingMode(.palette)
                    .font(.system(size: 16))
                    .foregroundStyle(.white, color("#309b21"))
                    .offset(x: 4, y: 4)
            }
            .padding(.bottom, 24)
    }
}

/// Create a new Profile, like Arc's: its name, then, without a Space given, the Spaces that use it.
private struct CreateProfileDialog: View {
    let names: [String]
    let spaces: [Space]
    let space: Int?
    let close: () -> Void
    let create: (String, [Int]) -> Void
    private let _name = State(initialValue: "")
    private let _picking = State(initialValue: false)
    private let _picked = State<Set<Int>>(initialValue: [])
    @FocusState private var naming: Bool

    private var name: String { _name.wrappedValue.trimmingCharacters(in: .whitespaces) }
    private var taken: Bool {
        names.contains { $0.trimmingCharacters(in: .whitespaces).lowercased() == name.lowercased() }
    }

    var body: some View {
        DialogCard {
            NewProfileIcon()
            if _picking.wrappedValue {
                spacesStep
            } else {
                nameStep
            }
        }
    }

    @ViewBuilder private var nameStep: some View {
        DialogText(
            title: "Create a new Profile",
            text: "Profiles make your Spaces truly separate — with their own history, saved passwords, extensions, and other browsing data. No more flipping between work and personal accounts!")
        Text("What do you want to call this Profile?").font(.system(size: 14))
            .foregroundStyle(.white.opacity(0.7))
            .padding(.top, 24)
        TextField("", text: _name.projectedValue, prompt: Text("Personal").foregroundStyle(.white.opacity(0.3)))
            .textFieldStyle(.plain)
            .font(.system(size: 15))
            .focused($naming)
            .padding(.horizontal, 12)
            .frame(height: 46)
            .background(RoundedRectangle(cornerRadius: 8).strokeBorder(.white.opacity(0.08)))
            .padding(.top, 10)
            .onAppear { naming = true }
        Text(taken ? "A Profile with that name already exists." : "Profiles are not synced between computers.")
            .font(.system(size: 12))
            .foregroundStyle(taken ? color("#ff6b6b") : .white.opacity(0.5))
            .padding(.top, 6)
        DialogButtons(
            cancel: "Cancel", confirm: space == nil ? "Next" : "Create Profile",
            enabled: !name.isEmpty && !taken, back: close
        ) {
            if let space {
                create(name, [space])
                close()
            } else {
                _picking.wrappedValue = true
            }
        }
    }

    @ViewBuilder private var spacesStep: some View {
        DialogText(
            title: "Which Spaces should use this Profile?",
            text: "These Spaces will start fresh in your brand new Profile — so you’ll have to log back in to websites, re-save your passwords, add extensions, etc. You can always switch back to your previous Profile though if you forgot something!")
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 18) {
                ForEach(spaces) { space in
                    SpaceChoice(space: space, picked: _picked.wrappedValue.contains(space.id)) {
                        if _picked.wrappedValue.contains(space.id) {
                            _picked.wrappedValue.remove(space.id)
                        } else {
                            _picked.wrappedValue.insert(space.id)
                        }
                    }
                }
            }
            .padding(.horizontal, 38)
        }
        .padding(.horizontal, -38)
        .padding(.top, 20)
        DialogButtons(
            cancel: "Back", confirm: "Create Profile", enabled: !_picked.wrappedValue.isEmpty,
            back: { _picking.wrappedValue = false }
        ) {
            create(name, spaces.map(\.id).filter(_picked.wrappedValue.contains))
            close()
        }
    }
}

/// A Space to pick, like Arc's: a card in its theme's colors, a check under it, and its name.
private struct SpaceChoice: View {
    let space: Space
    let picked: Bool
    let toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            VStack(spacing: 10) {
                Tint(theme: space.theme, systemIsDark: true)
                    .frame(width: 64, height: 96)
                    .clipShape(RoundedRectangle(cornerRadius: 10))
                    .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.white.opacity(0.15)))
                Image(systemName: picked ? "checkmark.circle.fill" : "circle")
                    .symbolRenderingMode(.palette)
                    .font(.system(size: 16))
                    .foregroundStyle(picked ? AnyShapeStyle(.white) : AnyShapeStyle(.white.opacity(0.4)),
                                     color("#309b21"))
                Text(space.title).font(.system(size: 12))
                    .foregroundStyle(.white.opacity(0.7))
                    .lineLimit(1)
                    .frame(width: 76)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
