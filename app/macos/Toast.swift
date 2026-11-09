// Toasts, like Arc's: short notes at the page's top right, like "Copied Current URL", which drop
// in from above and go after 5 seconds, but stay while the pointer is over them; a few show at
// once, one under the other. The app sends them (toast in app/src/shell/toasts.rs), the
// interface shows its own too, like the captures'.

import AppKit
import SwiftUI

/// What a toast says: an icon and a label, a key to press inside it, like "Cleared Tabs. ⌘Z to
/// Undo", a line under it, and a button.
struct Toast {
    /// The toast showing with the same key gives way to this one, like the zoom's level.
    var key: String?
    var text: String
    var icon: String?
    /// The keys drawn as keycaps after the label, and the words after them.
    var keys: String?
    var trailing: String?
    var sublabel: String?
    var button: String?
    var buttonIcon: String?
    var error = false
    /// What a click does, on its button or, without one, on the whole toast.
    var action: (() -> Void)?
}

/// A toast as the app sends it, its action the commands it runs.
private struct SentToast: Decodable {
    var key: String?
    var text: String
    var icon: String?
    var keys: String?
    var trailing: String?
    var sublabel: String?
    var button: String?
    var buttonIcon: String?
    var error: Bool?
    var commands: [String]?
}

@MainActor
final class Toasts {
    /// How long a toast shows, and how many show at once: a new one takes the oldest's place.
    private static let duration: TimeInterval = 5
    private static let most = 3
    private static let gap: CGFloat = 8

    private let parent: NSWindow
    private let model: AppModel
    /// The toasts showing, from the top.
    private var shown: [Shown] = []
    /// Where their top right corner is, in the window's points from its top left corner.
    private var corner = NSPoint.zero
    private var count = 0

    @MainActor private final class Shown {
        let id: Int
        let key: String?
        let host: NSHostingController<ToastView>
        let overlay: Overlay
        var hiding: DispatchWorkItem?

        init(id: Int, key: String?, host: NSHostingController<ToastView>) {
            self.id = id
            self.key = key
            self.host = host
            overlay = Overlay(view: host.view)
        }
    }

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        self.model = model
    }

    /// Shows a toast at the top right of the page at `page`, in the window's points from its top
    /// left corner.
    func show(_ toast: Toast, over page: NSRect) {
        show(toast, at: NSPoint(x: page.maxX - 8, y: page.minY + 8))
    }

    /// Shows a toast, its top right corner at `corner`, in the window's points from its top left
    /// corner.
    func show(_ toast: Toast, at corner: NSPoint) {
        self.corner = corner
        if let key = toast.key, let same = shown.first(where: { $0.key == key }) {
            same.host.rootView = view(toast, same.id)
            hide(same.id, after: Self.duration)
            arrange(entering: nil)
            return
        }
        count += 1
        let host = NSHostingController(rootView: view(toast, count))
        host.sizingOptions = []
        host.safeAreaRegions = []
        let entering = Shown(id: count, key: toast.key, host: host)
        shown.append(entering)
        if shown.count > Self.most { remove(shown[0].id) }
        hide(entering.id, after: Self.duration)
        arrange(entering: entering)
    }

    /// Shows a toast the app sent, as JSON, at the top right of the page at `page`.
    func show(json: Data, over page: NSRect) {
        guard let sent = try? JSONDecoder().decode(SentToast.self, from: json) else { return }
        let model = model
        let action = sent.commands.map { commands in { commands.forEach { model.run(command: $0) } } }
        show(
            Toast(
                key: sent.key, text: sent.text, icon: sent.icon, keys: sent.keys,
                trailing: sent.trailing, sublabel: sent.sublabel, button: sent.button,
                buttonIcon: sent.buttonIcon, error: sent.error ?? false, action: action),
            over: page)
    }

    private func view(_ toast: Toast, _ id: Int) -> ToastView {
        ToastView(
            toast: toast, fill: toastFill(model.space?.theme, error: toast.error),
            hover: { [weak self] inside in
                self?.hide(id, after: inside ? nil : Self.duration)
            },
            click: { [weak self] in
                toast.action?()
                self?.remove(id)
            })
    }

    /// Lays the toasts out one under the other, the one `entering` dropping in from above.
    private func arrange(entering: Shown?) {
        var top = corner.y
        for toast in shown {
            let size = toast.host.sizeThatFits(in: NSSize(width: 560, height: 200))
            let frame = NSRect(x: corner.x - size.width, y: top, width: size.width, height: size.height)
            top += size.height + Self.gap
            guard toast === entering else {
                NSAnimationContext.runAnimationGroup { context in
                    context.duration = 0.2
                    toast.overlay.view.animator().frame = frame
                }
                continue
            }
            toast.overlay.place(in: parent, frame.offsetBy(dx: 0, dy: -12))
            toast.overlay.view.alphaValue = 0
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.25
                context.timingFunction = CAMediaTimingFunction(name: .easeOut)
                toast.overlay.view.animator().frame = frame
                toast.overlay.view.animator().alphaValue = 1
            }
        }
    }

    /// Takes a toast away after `delay`, or keeps it without one.
    private func hide(_ id: Int, after delay: TimeInterval?) {
        guard let toast = shown.first(where: { $0.id == id }) else { return }
        toast.hiding?.cancel()
        guard let delay else { return }
        let hide = DispatchWorkItem { [weak self] in self?.remove(id) }
        toast.hiding = hide
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: hide)
    }

    /// Fades a toast out, the others moving up into its place.
    private func remove(_ id: Int) {
        guard let index = shown.firstIndex(where: { $0.id == id }) else { return }
        let toast = shown.remove(at: index)
        toast.hiding?.cancel()
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.2
            toast.overlay.view.animator().alphaValue = 0
        } completionHandler: {
            MainActor.assumeIsolated { toast.overlay.close() }
        }
        arrange(entering: nil)
    }

    func close() {
        shown.forEach { $0.hiding?.cancel(); $0.overlay.close() }
        shown = []
    }
}

/// A toast's color: its Space's leading color made dark, like a theme's titles over a light
/// background, which white text reads well on over light and dark pages alike; Arc's dark purple
/// without one; red for an error.
@MainActor private func toastFill(_ theme: Theme?, error: Bool) -> Color {
    if error { return color("#6b1d1d") }
    return color(theme?.colors.first.map { ThemePalette($0).titles.light } ?? "#220f41")
}

struct ToastView: View {
    let toast: Toast
    let fill: Color
    let hover: (Bool) -> Void
    let click: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            if let icon = toast.icon {
                Image(systemName: icon).font(.system(size: 13, weight: .semibold))
            }
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(toast.text)
                    if let keys = toast.keys { Keycap(keys: keys) }
                    if let trailing = toast.trailing { Text(trailing) }
                }
                .font(.system(size: 14, weight: .semibold))
                if let sublabel = toast.sublabel {
                    Text(sublabel).font(.system(size: 12)).opacity(0.7)
                }
            }
            if let button = toast.button {
                Button(action: click) {
                    HStack(spacing: 4) {
                        if let icon = toast.buttonIcon { Image(systemName: icon) }
                        Text(button)
                    }
                }
                .buttonStyle(.plain)
                .font(.system(size: 12, weight: .semibold))
                .padding(.horizontal, 8)
                .frame(height: 24)
                .background(RoundedRectangle(cornerRadius: 6).fill(.white.opacity(0.18)))
            }
        }
        .lineLimit(1)
        .foregroundStyle(.white)
        .padding(.horizontal, 16)
        .padding(.vertical, 9)
        .frame(minHeight: 42)
        .background(RoundedRectangle(cornerRadius: 12).fill(fill))
        // Its edge, which sets it apart from a dark page.
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.white.opacity(0.14)))
        .fixedSize()
        .contentShape(Rectangle())
        .onTapGesture { if toast.button == nil, toast.action != nil { click() } }
        .onHover(perform: hover)
    }
}

/// Keys to press, drawn like a keyboard's.
private struct Keycap: View {
    let keys: String

    var body: some View {
        Text(keys)
            .font(.system(size: 12, weight: .semibold))
            .padding(.horizontal, 5)
            .frame(minWidth: 22, minHeight: 22)
            .background(RoundedRectangle(cornerRadius: 5).fill(.white.opacity(0.16)))
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(.white.opacity(0.25)))
    }
}

/// Shows a toast the app sent, as JSON (SentToast), at the top right of the page at a frame in
/// the window `id`'s points from its top left corner.
@_cdecl("arbalete_ui_toast")
public func toast(
    _ id: UInt32, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ json: UnsafePointer<CChar>
) {
    let json = Data(String(cString: json).utf8)
    MainActor.assumeIsolated {
        nativeWindows[id]?.toasts.show(
            json: json, over: NSRect(x: x, y: y, width: width, height: height))
    }
}
