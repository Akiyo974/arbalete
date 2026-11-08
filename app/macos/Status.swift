// The address of the link the pointer is over, in a pill at the bottom of its page, like Arc's:
// Chromium's status text (on_status_message in app/src/shell/content.rs), and what a click with
// the keys held would do.

import AppKit

private let pillHeight: CGFloat = 24
/// How far the pill keeps from the page's edges.
private let pillInset: CGFloat = 10
private let pillPadding: CGFloat = 9
/// Its widest until it has shown the same address a while.
private let compactWidth: CGFloat = 400
/// How long the address changes before the pill shows or hides, and stays before it widens.
private let settleDelay = 0.5
private let expandDelay = 1.5
/// How far from the page's bottom the pointer moves the pill to the other side, like Arc's.
private let avoidedHeight: CGFloat = 44

/// The pill, over a page's view (PageHost): it never takes the clicks.
@MainActor
final class StatusPill: NSView {
    private let effect = NSVisualEffectView()
    /// The theme's color, very dark, over the blur, like Arc's.
    private let tint = NSView()
    private let label = NSTextField(labelWithString: "")
    /// The address, as Chromium gives it.
    private var address = ""
    /// Whether it shows, or is showing, rather than hiding.
    private var shown = false
    /// Wider, the address shown a while, until it hides.
    private var expanded = false
    /// At the page's bottom right, out of the pointer's way.
    private var trailing = false
    private var settle: DispatchWorkItem?
    private var expand: DispatchWorkItem?
    /// Counts the texts given, so that a fade out ends hidden only if none came since.
    private var given = 0
    /// The text around the address, and the address alone, in the theme's light shade.
    private var quietColor = NSColor(white: 1, alpha: 0.7)
    /// Whether the pointer is over the link.
    private var hovering = false
    /// Tells it the keys held change and where the pointer moves, and takes the clicks it opens
    /// itself, while it has an address.
    private var monitor: Any?

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.masksToBounds = false
        // Arc's: black at 12%, 5 points of radius, centered.
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = 0.12
        layer?.shadowRadius = 5
        layer?.shadowOffset = .zero
        // Dark whatever the window's theme, like Arc's.
        appearance = NSAppearance(named: .darkAqua)
        effect.material = .popover
        effect.blendingMode = .withinWindow
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = pillHeight / 2
        effect.layer?.cornerCurve = .circular
        effect.layer?.masksToBounds = true
        effect.layer?.borderWidth = 1
        // Arc's outline, white at 20% over its dark fill.
        effect.layer?.borderColor = NSColor.white.withAlphaComponent(0.2).cgColor
        label.usesSingleLineMode = true
        tint.wantsLayer = true
        effect.addSubview(tint)
        effect.addSubview(label)
        addSubview(effect)
        alphaValue = 0
        isHidden = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// Shows `text` at once, the pill showing once it has settled; hides it without text, after
    /// it has settled, or at once when `instant`.
    func show(_ text: String, instant: Bool) {
        settle?.cancel()
        given += 1
        hovering = !text.isEmpty
        if instant { return hideNow() }
        if !text.isEmpty, text != address {
            address = text
            watch()
            takeTheme()
            render()
            expand?.cancel()
            let expand = DispatchWorkItem { [weak self] in self?.widen() }
            self.expand = expand
            DispatchQueue.main.asyncAfter(deadline: .now() + expandDelay, execute: expand)
        }
        let settle = DispatchWorkItem { [weak self] in self?.fade(in: !text.isEmpty) }
        self.settle = settle
        DispatchQueue.main.asyncAfter(deadline: .now() + settleDelay, execute: settle)
    }

    private func fade(in shows: Bool) {
        guard shows != shown else {
            if !shows { hideNow() }
            return
        }
        shown = shows
        if shows {
            isHidden = false
            if let window { avoid(window.mouseLocationOutsideOfEventStream) }
        } else {
            expand?.cancel()
        }
        let given = given
        NSAnimationContext.runAnimationGroup { _ in
            animator().alphaValue = shows ? 1 : 0
        } completionHandler: {
            MainActor.assumeIsolated {
                if !self.shown, self.given == given { self.hideNow() }
            }
        }
    }

    private func widen() {
        expanded = true
        render()
    }

    private func hideNow() {
        settle?.cancel()
        expand?.cancel()
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        shown = false
        hovering = false
        expanded = false
        address = ""
        alphaValue = 0
        if !isHidden { isHidden = true }
    }

    /// Hidden at once as its page hides, for another tab.
    override func viewDidHide() {
        super.viewDidHide()
        hideNow()
    }

    /// Watches the window's events rather than tracking the pointer, which the page's own view
    /// keeps, like Arc's.
    private func watch() {
        guard monitor == nil else { return }
        let events: NSEvent.EventTypeMask = [.flagsChanged, .mouseMoved, .leftMouseDown]
        monitor = NSEvent.addLocalMonitorForEvents(matching: events) { [weak self] event in
            let taken = MainActor.assumeIsolated { self?.take(event) ?? false }
            return taken ? nil : event
        }
    }

    /// Shows what a click does with the keys held now, moves out of the pointer's way, or opens
    /// the link on a click Chromium would not open as it says.
    private func take(_ event: NSEvent) -> Bool {
        if event.type == .leftMouseDown { return opens(event) }
        if event.type == .flagsChanged {
            render()
        } else if let window {
            // Without a window, its location is on the screen.
            let screen = event.window?.convertPoint(toScreen: event.locationInWindow)
            avoid(window.convertPoint(fromScreen: screen ?? event.locationInWindow))
        }
        return false
    }

    /// Opens the link in Split View on a click with ⌥ alone, which Chromium would download, or
    /// in a Little Arbalete window with ⌘⌥, which it would open in the page, like Arc's: the page
    /// never gets the click.
    private func opens(_ click: NSEvent) -> Bool {
        let commands = ["in Split View": "open-in-split", "in Little Arbalete": "open-in-little"]
        guard hovering, let command = explanation().flatMap({ commands[$0] }),
            let page = superview,
            click.window === window,
            window?.contentView?.superview?.hitTest(click.locationInWindow)?
                .isDescendant(of: page) == true,
            let model = nativeWindows.values.first(where: { $0.window === window })?.model
        else { return false }
        model.run(command, address)
        return true
    }

    /// The window's theme's color, like Arc's: a very dark shade under the text and a light one
    /// for the text but the address it explains; gray without one.
    private func takeTheme() {
        let theme = nativeWindows.values.first { $0.window === window }?.model.space?.theme
        let lead = theme?.colors.first ?? "#808080"
        tint.layer?.backgroundColor =
            NSColor(color(mix(lead, "#000000", 0.18))).withAlphaComponent(0.85).cgColor
        quietColor = NSColor(color(mix(lead, "#ffffff", 0.25))).withAlphaComponent(0.7)
    }

    /// What a click on the link would do with the keys held, as the page's handler does
    /// (on_open_urlfrom_tab in app/src/shell/content.rs), or `opens`: a Little Arbalete window's
    /// page has none.
    private func explanation() -> String? {
        guard !littles.values.contains(where: { $0.window === window }) else { return nil }
        switch NSEvent.modifierFlags.intersection([.command, .shift, .option, .control]) {
        case [.command]: return "in a new tab"
        case [.command, .shift], [.command, .shift, .option]: return "in a new tab and focus it"
        case [.command, .option]: return "in Little Arbalete"
        case [.shift], [.shift, .option]: return "in Peek"
        case [.option]: return "in Split View"
        default: return nil
        }
    }

    /// The address without its scheme, light; or what a click does, the whole address white and
    /// bold.
    private func render() {
        let explanation = explanation()
        let wraps = NSMutableParagraphStyle()
        wraps.lineBreakMode =
            expanded || explanation != nil ? .byTruncatingTail : .byTruncatingMiddle
        let quiet: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 12, weight: .medium),
            .foregroundColor: quietColor, .paragraphStyle: wraps,
        ]
        guard let explanation else {
            let bare = address.replacingOccurrences(
                of: "^https?://", with: "", options: .regularExpression)
            label.attributedStringValue = NSAttributedString(string: bare, attributes: quiet)
            return place()
        }
        var bold = quiet
        bold[.font] = NSFont.systemFont(ofSize: 12, weight: .semibold)
        bold[.foregroundColor] = NSColor.white
        let text = NSMutableAttributedString(string: "Open ", attributes: quiet)
        text.append(NSAttributedString(string: address, attributes: bold))
        text.append(NSAttributedString(string: " \(explanation)", attributes: quiet))
        label.attributedStringValue = text
        place()
    }

    /// At the page's bottom left, or right, as wide as its text, within the page: up to 400
    /// points until it widens, or explains a click.
    func place() {
        guard let page = superview else { return }
        let room = max(0, page.bounds.width - 2 * pillInset)
        let compact = !expanded && explanation() == nil
        let widest = compact ? min(compactWidth, room) : room
        // Measured as drawn, the field's own size following its frame once it truncates.
        let text = label.attributedStringValue.size()
        let width = min(ceil(text.width) + 2 * pillPadding, widest)
        let x = trailing ? page.bounds.maxX - pillInset - width : pillInset
        let y = page.isFlipped ? page.bounds.maxY - pillInset - pillHeight : pillInset
        frame = NSRect(x: x, y: y, width: width, height: pillHeight)
        effect.frame = bounds
        tint.frame = bounds
        // The field keeps its text 2 points from its edges.
        let height = ceil(text.height)
        label.frame = NSRect(
            x: pillPadding - 2, y: (pillHeight - height) / 2,
            width: max(0, width - 2 * pillPadding + 4), height: height)
        layer?.shadowPath = CGPath(
            roundedRect: bounds, cornerWidth: pillHeight / 2, cornerHeight: pillHeight / 2,
            transform: nil)
    }

    /// At the page's bottom right while the pointer, at `point` in the window, is in its bottom
    /// left corner, as wide as the pill and its margins, and at its bottom left otherwise, like
    /// Arc's; unless both corners hold the pointer, in a page too narrow.
    private func avoid(_ point: NSPoint) {
        guard let page = superview else { return }
        let point = page.convert(point, from: nil)
        let height = page.isFlipped ? page.bounds.maxY - point.y : point.y - page.bounds.minY
        let zone = frame.width + 2 * pillInset
        let low = height >= 0 && height < avoidedHeight
        let left = low && point.x >= page.bounds.minX && point.x < page.bounds.minX + zone
        let right = low && point.x > page.bounds.maxX - zone && point.x <= page.bounds.maxX
        guard !(left && right), left != trailing else { return }
        trailing = left
        place()
    }
}

/// Shows `text`, the address of the link hovered in the page CEF draws at `view`, or hides it
/// without text: at once when `instant`.
@_cdecl("arbalete_ui_page_status")
public func pageStatus(
    _ view: UnsafeMutableRawPointer?, _ text: UnsafePointer<CChar>, _ instant: Bool
) {
    // As an address: a pointer may not leave the main actor's closure.
    let address = UInt(bitPattern: view)
    let text = String(cString: text)
    MainActor.assumeIsolated {
        guard let view = UnsafeMutableRawPointer(bitPattern: address),
            let page = Unmanaged<NSView>.fromOpaque(view).takeUnretainedValue().superview
                as? PageHost
        else { return }
        page.status(text, instant: instant)
    }
}
