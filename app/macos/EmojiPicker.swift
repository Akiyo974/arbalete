// A Space's icon picker, like Arc's: above the Space's icon in the footer, an emoji or a symbol,
// with a search field and categories for the emoji, and a button that removes the icon; or, our
// own, a site's icon.

import AppKit
import SwiftUI

/// A Space's icon as a view: an emoji, a symbol stored as "symbol:<name>", or a site's icon the
/// app sends as "site:<url>", a globe while its file is missing.
struct SpaceGlyph: View {
    let icon: String
    let size: CGFloat
    /// The square a symbol or a site's icon fills, when it is not drawn like an emoji's text.
    var picture: CGFloat? = nil

    var body: some View {
        if let name = symbolName(icon) {
            if let picture {
                Image(systemName: name).resizable().scaledToFit().fontWeight(.semibold)
                    .frame(width: picture, height: picture)
            } else {
                Image(systemName: name).font(.system(size: size * 0.85, weight: .semibold))
            }
        } else if let url = siteIcon(icon) {
            if let image = uiImage(url) {
                Image(nsImage: image).resizable().interpolation(.high)
                    .frame(width: picture ?? size, height: picture ?? size)
            } else {
                Image(systemName: "globe").font(.system(size: size * 0.85, weight: .semibold))
            }
        } else {
            Text(icon).font(.system(size: size))
        }
    }
}

/// The symbol's name a Space's icon stores, if it is one.
func symbolName(_ icon: String) -> String? {
    icon.hasPrefix("symbol:") ? String(icon.dropFirst("symbol:".count)) : nil
}

/// Where the site's icon a Space shows loads from, if it shows one.
func siteIcon(_ icon: String) -> String? {
    icon.hasPrefix("site:") ? String(icon.dropFirst("site:".count)) : nil
}

/// A Space's icon as text, for a menu: only an emoji; a symbol or a site's icon shows only in the
/// sidebar.
func emojiText(_ icon: String?) -> String? {
    guard let icon, symbolName(icon) == nil, siteIcon(icon) == nil else { return nil }
    return icon
}

/// The kinds of icon the picker offers, by their tab's title.
private enum IconKind: String, CaseIterable {
    case emoji = "Emoji"
    case symbol = "Icon"
    case site = "Site"

    init(_ icon: String) {
        self = symbolName(icon) != nil ? .symbol : siteIcon(icon) != nil ? .site : .emoji
    }
}

/// A category of the emoji, with the symbol of its button.
private struct EmojiCategory {
    let symbol: String
    let emoji: [(text: String, name: String)]
}

/// The emoji, by category, with their names to search them by.
@MainActor private let categories: [EmojiCategory] = {
    var seen = Set<String>()
    func scalars(_ ranges: [ClosedRange<UInt32>]) -> [(text: String, name: String)] {
        ranges.flatMap { $0 }.compactMap { value in
            guard let scalar = Unicode.Scalar(value), scalar.properties.isEmojiPresentation
            else { return nil }
            let text = String(scalar)
            guard seen.insert(text).inserted else { return nil }
            return (text, (scalar.properties.name ?? "").lowercased())
        }
    }
    let flags: [(text: String, name: String)] = Locale.Region.isoRegions.compactMap { region in
        let code = region.identifier
        guard code.count == 2, code.allSatisfy(\.isLetter) else { return nil }
        let text = String(
            String.UnicodeScalarView(
                code.uppercased().unicodeScalars.compactMap {
                    Unicode.Scalar(0x1F1E6 + $0.value - 65)
                }))
        let name = Locale(identifier: "en").localizedString(forRegionCode: code) ?? code
        return (text, "flag " + name.lowercased())
    }
    return [
        EmojiCategory(
            symbol: "face.smiling",
            emoji: scalars([
                0x1F600...0x1F64F, 0x1F910...0x1F92F, 0x1F970...0x1F97F, 0x1F9D0...0x1F9DF,
                0x1F466...0x1F487,
            ])),
        EmojiCategory(
            symbol: "sun.max",
            emoji: scalars([0x1F330...0x1F344, 0x1F400...0x1F43F, 0x1F980...0x1F9AF, 0x1F300...0x1F32F])),
        EmojiCategory(
            symbol: "cup.and.saucer",
            emoji: scalars([0x1F345...0x1F37F, 0x1F950...0x1F96F, 0x1F9C0...0x1F9CF])),
        EmojiCategory(
            symbol: "medal",
            emoji: scalars([0x1F380...0x1F3CF, 0x1F93A...0x1F94F, 0x26BD...0x26BE])),
        EmojiCategory(
            symbol: "box.truck", emoji: scalars([0x1F680...0x1F6FF, 0x1F3D4...0x1F3F0])),
        EmojiCategory(
            symbol: "gift",
            emoji: scalars([0x1F488...0x1F4FF, 0x1F500...0x1F5FF, 0x1FA70...0x1FAFF, 0x231A...0x23FF])),
        EmojiCategory(symbol: "heart", emoji: scalars([0x2600...0x27BF])),
        EmojiCategory(symbol: "flag", emoji: flags),
    ]
}()

/// The symbols a Space's icon may be: our own pick, drawn by the system.
private let symbols = [
    "star.fill", "bookmark.fill", "heart.fill", "flag.fill", "bolt.fill", "triangle.fill",
    "asterisk", "bell.fill", "lightbulb.fill", "square.on.circle.fill", "square.grid.2x2.fill",
    "circle.grid.3x3.fill", "square.stack.3d.up.fill", "cylinder.split.1x2.fill", "tray.2.fill",
    "square.on.square.fill", "folder.fill", "tray.fill", "calendar", "envelope.fill",
    "checkmark.square.fill", "doc.fill", "book.fill", "bubble.left.fill", "person.2.fill",
    "terminal.fill", "hammer.fill", "square.fill", "drop.fill", "circle.fill", "moon.fill",
    "sun.max.fill", "globe.americas.fill", "leaf.fill", "cloud.fill", "pawprint.fill",
    "basket.fill", "gift.fill", "bed.double.fill", "fork.knife", "dumbbell.fill", "airplane",
    "music.note", "paintpalette.fill", "video.fill", "bandage.fill",
    "chevron.left.forwardslash.chevron.right", "baseball.fill", "cloud", "map.fill",
    "flame.fill", "birthday.cake.fill", "theatermasks.fill", "scroll.fill",
    "hand.thumbsup.fill", "tram.fill",
]

struct EmojiPicker: View {
    @ObservedObject var model: AppModel
    let space: Int
    private let _query = State<String>(initialValue: "")
    private let _kind: State<IconKind>
    private let _address = State<String>(initialValue: "")
    @FocusState private var searching: Bool
    /// Closes the popover it shows in: a Space's in the footer, or the new Space's in its form.
    @Environment(\.dismiss) private var dismiss

    init(model: AppModel, space: Int) {
        _model = ObservedObject(wrappedValue: model)
        self.space = space
        // It opens on the kind of icon the Space has.
        let icon = model.state?.spaces.first { $0.id == space }?.emoji ?? ""
        _kind = State(initialValue: IconKind(icon))
    }

    private var current: String? {
        model.state?.spaces.first { $0.id == space }?.emoji
    }

    var body: some View {
        let shown = _kind.wrappedValue
        VStack(spacing: 10) {
            // Like Arc's: the kinds side by side in the middle, the trash beside them.
            HStack(spacing: 10) {
                HStack(spacing: 0) {
                    ForEach(IconKind.allCases, id: \.self) { kind in
                        self.kind(kind.rawValue, selected: kind == shown) {
                            _kind.wrappedValue = kind
                        }
                    }
                }
                .padding(2)
                .background(RoundedRectangle(cornerRadius: 8).fill(Color.white.opacity(0.1)))
                IconButton(symbol: "trash", label: "Remove the icon", size: 26, symbolSize: 14) {
                    pick("")
                }
            }
            switch shown {
            case .emoji: emojiGrid
            case .symbol: symbolGrid
            case .site: siteGrid
            }
        }
        .padding(12)
        .frame(width: 272)
        .onExitCommand { dismiss() }
    }

    private func kind(_ title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.white)
                .frame(width: 56, height: 24)
                .background(
                    RoundedRectangle(cornerRadius: 6).fill(selected ? Color.accentColor : .clear))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var emojiGrid: some View {
        let query = _query.wrappedValue.lowercased().trimmingCharacters(in: .whitespaces)
        let shown =
            query.isEmpty
            ? categories
            : [
                EmojiCategory(
                    symbol: "", emoji: categories.flatMap(\.emoji).filter { $0.name.contains(query) })
            ]
        return VStack(spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                TextField("Search", text: _query.projectedValue)
                    .textFieldStyle(.plain)
                    .focused($searching)
                    .background(TakesKeyboard())
                    .onSubmit {
                        if let first = shown.first?.emoji.first { pick(first.text) }
                    }
            }
            .font(.system(size: 14))
            .padding(.horizontal, 9)
            .frame(height: 28)
            .background(RoundedRectangle(cornerRadius: 9).fill(Color.black.opacity(0.3)))
            ScrollViewReader { reader in
                VStack(spacing: 0) {
                    ScrollView {
                        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 0), count: 8)) {
                            ForEach(Array(shown.enumerated()), id: \.offset) { index, category in
                                Section {
                                    ForEach(category.emoji, id: \.text) { item in
                                        Button { pick(item.text) } label: {
                                            Text(item.text).font(.system(size: 22))
                                                .frame(width: 30, height: 32)
                                        }
                                        .buttonStyle(.plain)
                                        .help(item.name.capitalized)
                                    }
                                } header: {
                                    Color.clear.frame(height: 0).id(index)
                                }
                            }
                        }
                    }
                    .frame(height: 170)
                    Divider()
                    // Like Arc's, a row of the categories, each scrolling to its own.
                    HStack(spacing: 0) {
                        ForEach(Array(categories.enumerated()), id: \.offset) { index, category in
                            Button {
                                _query.wrappedValue = ""
                                withAnimation { reader.scrollTo(index, anchor: .top) }
                            } label: {
                                Image(systemName: category.symbol)
                                    .font(.system(size: 14))
                                    .foregroundStyle(.secondary)
                                    .frame(maxWidth: .infinity, minHeight: 28)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.top, 4)
                }
            }
        }
        .onAppear { DispatchQueue.main.async { searching = true } }
    }

    private var symbolGrid: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 0), count: 8), spacing: 4) {
            ForEach(symbols, id: \.self) { name in
                let picked = current == "symbol:\(name)"
                Button { pick("symbol:\(name)") } label: {
                    Image(systemName: name)
                        .font(.system(size: 16, weight: .semibold))
                        .frame(width: 30, height: 30)
                        .background(
                            RoundedRectangle(cornerRadius: 6)
                                .fill(Color.white.opacity(picked ? 0.15 : 0)))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
    }

    /// The icons of the Space's tabs, one per icon, and a field for a site's address.
    private var siteGrid: some View {
        VStack(spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: "globe").foregroundStyle(.secondary)
                TextField("Site address", text: _address.projectedValue)
                    .textFieldStyle(.plain)
                    .focused($searching)
                    .background(TakesKeyboard())
                    .onSubmit {
                        dismiss()
                        model.run("set-space-site", space, _address.wrappedValue)
                    }
            }
            .font(.system(size: 14))
            .padding(.horizontal, 9)
            .frame(height: 28)
            .background(RoundedRectangle(cornerRadius: 9).fill(Color.black.opacity(0.3)))
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 0), count: 8), spacing: 4) {
                ForEach(siteTabs) { tab in
                    let picked = current == "site:\(tab.favicon ?? "")"
                    Button {
                        dismiss()
                        model.run("set-space-site", space, tab.url)
                    } label: {
                        Favicon(url: tab.favicon, size: 18)
                            .frame(width: 30, height: 30)
                            .background(
                                RoundedRectangle(cornerRadius: 6)
                                    .fill(Color.white.opacity(picked ? 0.15 : 0)))
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help(tab.label)
                }
            }
        }
        .onAppear { DispatchQueue.main.async { searching = true } }
    }

    /// The Space's tabs with an icon, one per icon, as the app sends them: for the displayed Space
    /// and its neighbors only.
    private var siteTabs: [Tab] {
        guard let page = model.state?.pages.first(where: { $0.space == space }) else { return [] }
        var seen = Set<String>()
        let items: [Item] = page.favorites + page.pinned + page.today
        return items.flatMap(\.tabs).filter { tab in
            guard let favicon = tab.favicon else { return false }
            return seen.insert(favicon).inserted
        }
    }

    /// Gives the Space this icon, or none for an empty one, and closes the picker.
    private func pick(_ text: String) {
        dismiss()
        model.run("set-space-emoji", space, text)
    }
}
