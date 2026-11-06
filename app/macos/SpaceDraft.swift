// A new Space, like Arc's: it shows at once, without colors, and the sidebar holds a form for its
// name, its icon, its profile and its theme, which the window shows as it is picked. Created, it
// stays; cancelled, it goes.

import AppKit
import SwiftUI

extension AppModel {
    /// Creates a Space to fill in, with the tab `moving` moved into it.
    func newSpace(moving tab: Int? = nil) {
        guard let state else { return }
        // Like Arc's, the form shows in the sidebar, the library closed.
        if libraryShown { closeLibrary() }
        awaitingDraft = Set(state.spaces.map(\.id))
        let title = percentEncoded("Space \(state.spaces.count + 1)")
        run("space/create/purple//\(title)" + (tab.map { "/\($0)" } ?? ""))
    }

    /// Notes the Space created to fill in, once displayed, and forgets it once another one is.
    func followDraft() {
        guard let state else { return }
        if let known = awaitingDraft, !known.contains(state.space) {
            awaitingDraft = nil
            draftSpace = state.space
            // Like Arc's, it starts without colors.
            let bare = #"{"colors":[],"mode":"auto","intensity":0.8,"noise":0}"#
            run("space/theme/\(state.space)/\(percentEncoded(bare))")
        } else if let draft = draftSpace, draft != state.space {
            // Left for another Space without its button, it was never created, like Arc's.
            draftSpace = nil
            run("space/delete/\(draft)")
        }
    }
}

/// The form for a new Space, in the sidebar in place of its tabs.
struct SpaceDraft: View {
    @ObservedObject var model: AppModel
    let space: Space
    private let _name = State<String>(initialValue: "")
    private let _emoji = State<String?>(initialValue: nil)
    private let _picking = State<Bool>(initialValue: false)
    @FocusState private var naming: Bool

    private var name: String { _name.wrappedValue.trimmingCharacters(in: .whitespaces) }

    var body: some View {
        // Measured on Arc's: the header from 56 points under the window's buttons, fields 36
        // points high 7 apart, and the buttons at the bottom.
        VStack(spacing: 0) {
            Image(systemName: "square.stack.3d.up.fill")
                .font(.system(size: 40))
                .frame(height: 44)
                .padding(.top, 40)
            Text("Create a Space").font(.system(size: 17, weight: .bold))
                .padding(.top, 17)
            Text("Keep the tabs of each part of your life apart.")
                .font(.system(size: 14))
                .foregroundStyle(Ink.primary)
                .multilineTextAlignment(.center)
                .padding(.top, 10)
                .padding(.bottom, 32)
            VStack(spacing: 7) {
                field {
                    icon
                    TextField("Space name…", text: _name.projectedValue)
                        .textFieldStyle(.plain)
                        .font(.system(size: 14, weight: .medium))
                        .focused($naming)
                        .onSubmit(create)
                }
                field {
                    Image(systemName: "face.smiling").frame(width: 24)
                        .foregroundStyle(Ink.secondary)
                    Text("Profile")
                    Menu(profileName) {
                        ForEach(model.state?.profiles ?? []) { profile in
                            Button(profile.name) {
                                model.run("space/profile/\(space.id)/\(profile.id)")
                            }
                        }
                        Divider()
                        Button("New Profile…") { model.newProfile(for: space.id) }
                    }
                    .menuStyle(.borderlessButton)
                    .font(.system(size: 13, weight: .semibold))
                    .fixedSize()
                    .padding(.horizontal, 8)
                    .padding(.vertical, 2)
                    .background(Capsule().fill(Ink.item))
                    Spacer(minLength: 0)
                    Image(systemName: "info.circle")
                        .foregroundStyle(Ink.secondary)
                        .frame(width: 24)
                        .help("You can use a Profile to give this Space its own logins, history, and browsing data.")
                }
                Button {
                    model.run("theme/show/\(space.id)")
                } label: {
                    field {
                        Image(systemName: "paintbrush.pointed.fill").frame(width: 24)
                            .foregroundStyle(Ink.secondary)
                        Text("Choose a Theme")
                    }
                }
                .buttonStyle(.plain)
            }
            Spacer()
            Button(action: create) {
                Text("Create Space")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity, minHeight: 36)
                    .background(RoundedRectangle(cornerRadius: 6).fill(model.accent))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(name.isEmpty)
            .opacity(name.isEmpty ? 0.5 : 1)
            Button("Cancel", action: cancel)
                .buttonStyle(.plain)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(Ink.primary)
                .padding(.top, 14)
                .padding(.bottom, 21)
        }
        .font(.system(size: 14, weight: .semibold))
        .padding(.horizontal, 6)
        .onAppear { naming = true }
    }

    /// The icon: a dashed square until an emoji is picked; a click opens the character palette.
    @ViewBuilder private var icon: some View {
        if _picking.wrappedValue {
            EmojiCatcher { picked in
                if let picked { _emoji.wrappedValue = picked }
                _picking.wrappedValue = false
            }
            .frame(width: 24)
        } else {
            Button {
                _picking.wrappedValue = true
            } label: {
                Group {
                    if let emoji = _emoji.wrappedValue {
                        Text(emoji).font(.system(size: 16))
                    } else {
                        Image(systemName: "plus")
                            .font(.system(size: 10, weight: .semibold))
                            .frame(width: 18, height: 18)
                            .overlay(
                                RoundedRectangle(cornerRadius: 4)
                                    .strokeBorder(style: StrokeStyle(lineWidth: 1, dash: [2, 2])))
                            .foregroundStyle(.secondary)
                    }
                }
                .frame(width: 24, height: 24)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Choose an Icon")
        }
    }

    private func field(@ViewBuilder _ content: () -> some View) -> some View {
        HStack(spacing: 6) {
            content()
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 6)
        .frame(height: Measure.rowFill)
        .background(RoundedRectangle(cornerRadius: 6).fill(Ink.child))
        .contentShape(Rectangle())
    }

    private var profileName: String {
        model.state?.profiles.first { $0.id == space.profile }?.name ?? "Default"
    }

    private func create() {
        guard !name.isEmpty else { return }
        model.draftSpace = nil
        model.run("space/rename/\(space.id)/\(percentEncoded(name))")
        if let emoji = _emoji.wrappedValue {
            model.run("space/emoji/\(space.id)/\(percentEncoded(emoji))")
        }
    }

    private func cancel() {
        model.draftSpace = nil
        model.run("space/delete/\(space.id)")
    }
}

/// Takes the first emoji picked in the character palette, which it opens, or typed; nil once
/// left without one.
private struct EmojiCatcher: View {
    let picked: (String?) -> Void
    private let _text = State<String>(initialValue: "")
    @FocusState private var focused: Bool

    var body: some View {
        TextField("", text: _text.projectedValue)
            .textFieldStyle(.plain)
            .font(.system(size: 16))
            .focused($focused)
            .onAppear {
                focused = true
                DispatchQueue.main.async { NSApp.orderFrontCharacterPalette(nil) }
            }
            .onChange(of: _text.wrappedValue) { _, now in
                if let first = now.first { picked(String(first)) }
            }
            .onExitCommand { picked(nil) }
            .onChange(of: focused) { _, now in
                if !now { picked(nil) }
            }
    }
}
