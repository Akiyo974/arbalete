// The menu of the sidebar's "+", like Arc's: a dark panel pointing at the button, which turns
// into a "×" while it shows. Measured on Arc's, in points.

import AppKit
import SwiftUI

/// The panel's size and the room around its rows.
private let menuWidth: CGFloat = 210
private let rowHeight: CGFloat = 40
private let menuPadding: CGFloat = 8
/// From the panel's sides to a highlighted row's, and to a separator's.
private let rowInset: CGFloat = 9
private let separatorInset: CGFloat = 12
/// The arrow under the panel, its tip over the button.
private let arrowWidth: CGFloat = 10
private let arrowHeight: CGFloat = 5

/// A row of the menu: a disabled one says why on hover.
struct PlusMenuItem: Identifiable {
    let id: String
    let symbol: String
    let title: String
    var shortcut: String? = nil
    var enabled = true
    var help: String? = nil
    var action: () -> Void = {}
}

extension AppModel {
    /// The "+" menu's rows, in groups, like Arc's.
    var plusMenuGroups: [[PlusMenuItem]] {
        [
            [
                PlusMenuItem(id: "space", symbol: "plus.square.on.square", title: "New Space") {
                    [weak self] in self?.newSpace()
                },
                PlusMenuItem(id: "folder", symbol: "folder", title: "New Folder") {
                    [weak self] in self?.run("folder/new")
                },
            ],
            [
                PlusMenuItem(
                    id: "easel", symbol: "scribble", title: "New Easel", shortcut: "⌃⇧E",
                    enabled: false, help: "Coming soon"),
                PlusMenuItem(
                    id: "boost", symbol: "paintbrush.pointed", title: "New Boost", enabled: false,
                    help: "Coming soon"),
            ],
            [
                // Like Arc's, a split needs a page to join.
                PlusMenuItem(
                    id: "split", symbol: "rectangle.split.2x1", title: "New Split",
                    shortcut: "⌃⇧=", enabled: page?.selected != nil
                ) { [weak self] in self?.run("pane/add") },
                PlusMenuItem(id: "tab", symbol: "plus.square", title: "New Tab", shortcut: "⌘T") {
                    [weak self] in self?.run("overlay/show")
                },
            ],
        ]
    }
}

/// The panel, with its rows highlighted on hover or with the arrow keys, New Tab first.
struct PlusMenu: View {
    @ObservedObject var model: AppModel
    let close: () -> Void
    @Environment(\.colorScheme) private var scheme
    private let _highlighted = State<String?>(initialValue: "tab")
    private var highlighted: String? {
        get { _highlighted.wrappedValue }
        nonmutating set { _highlighted.wrappedValue = newValue }
    }
    private let _monitor = State<Any?>(initialValue: nil)

    // Measured on Arc's, a colorless Space in dark mode.
    private var background: Color {
        scheme == .dark ? Color(red: 1 / 255, green: 4 / 255, blue: 32 / 255) : .white
    }
    private var highlight: Color {
        scheme == .dark
            ? Color(red: 41 / 255, green: 44 / 255, blue: 131 / 255) : model.focusColor.opacity(0.18)
    }

    var body: some View {
        let groups = model.plusMenuGroups
        VStack(spacing: 0) {
            ForEach(Array(groups.enumerated()), id: \.offset) { index, group in
                if index > 0 {
                    Rectangle()
                        .fill(highlight)
                        .frame(height: 1)
                        .padding(.horizontal, separatorInset)
                        .padding(.vertical, 8)
                }
                ForEach(group) { item in row(item) }
            }
        }
        .padding(.top, menuPadding)
        .padding(.bottom, menuPadding - 1)
        .frame(width: menuWidth)
        .background(PanelShape().fill(background).shadow(color: .black.opacity(0.3), radius: 12, y: 4))
        .onAppear { watchKeys() }
        .onDisappear {
            if let monitor = _monitor.wrappedValue { NSEvent.removeMonitor(monitor) }
        }
    }

    private func row(_ item: PlusMenuItem) -> some View {
        let lit = item.enabled && highlighted == item.id
        let ink: Color = scheme == .dark ? .white : Ink.primary
        return HStack(spacing: 0) {
            Image(systemName: item.symbol)
                .font(.system(size: 15, weight: .regular))
                .frame(width: 18, height: 18)
                .padding(.leading, 18)
            Text(item.title)
                .font(.system(size: 13, weight: .semibold))
                .padding(.leading, 14)
            Spacer(minLength: 8)
            if let shortcut = item.shortcut {
                Text(shortcut)
                    .font(.system(size: 13))
                    .kerning(3)
                    .foregroundStyle(ink.opacity(0.4))
                    .padding(.trailing, 12)
            }
        }
        .foregroundStyle(lit && scheme == .light ? model.focusColor : ink)
        .opacity(item.enabled ? 1 : 0.42)
        .frame(height: rowHeight)
        .background(RoundedRectangle(cornerRadius: 8).fill(lit ? highlight : .clear))
        .padding(.horizontal, rowInset)
        .contentShape(Rectangle())
        .onHover { over in
            if over {
                highlighted = item.enabled ? item.id : nil
            } else if highlighted == item.id {
                highlighted = nil
            }
        }
        .onTapGesture { choose(item) }
        .help(item.help ?? "")
    }

    private func choose(_ item: PlusMenuItem) {
        guard item.enabled else { return }
        close()
        item.action()
    }

    /// Like a menu's, the arrow keys move the highlight, Return picks it and Escape closes.
    private func watchKeys() {
        _monitor.wrappedValue = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            let items = model.plusMenuGroups.flatMap { $0 }.filter(\.enabled)
            let index = items.firstIndex { $0.id == highlighted }
            switch event.keyCode {
            case 126:  // Up
                highlighted = items[index.map { max(0, $0 - 1) } ?? items.count - 1].id
            case 125:  // Down
                highlighted = items[index.map { min(items.count - 1, $0 + 1) } ?? 0].id
            case 36, 76:  // Return, Enter
                if let index { choose(items[index]) }
            case 53:  // Escape
                close()
            default:
                return event
            }
            return nil
        }
    }
}

/// The panel's rounded rectangle with the arrow under its trailing end, over the "+".
private struct PanelShape: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path(roundedRect: rect, cornerRadius: 10)
        let tip = CGPoint(x: rect.maxX - plusMenuArrowTrailing, y: rect.maxY + arrowHeight)
        path.move(to: CGPoint(x: tip.x - arrowWidth / 2, y: rect.maxY))
        path.addQuadCurve(
            to: CGPoint(x: tip.x + arrowWidth / 2, y: rect.maxY),
            control: CGPoint(x: tip.x, y: tip.y + 1))
        path.closeSubpath()
        return path
    }
}

/// From the panel's trailing side to its arrow's tip, over the "+"'s center.
let plusMenuArrowTrailing: CGFloat = 16
/// From the arrow's tip to the "+"'s center below it.
let plusMenuAbovePlus: CGFloat = 27.5
