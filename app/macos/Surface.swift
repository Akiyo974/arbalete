// A panel above the page, like the Command Bar, which the app shows, places and hides (Spot and
// mirror in app/src/shell.rs): a view of the main window (Overlay).

import AppKit
import SwiftUI

/// The view over each window's card and sidebar, by window, which the views standing in for
/// its child windows show in (PagePlacer.overlay).
@MainActor var overlays: [ObjectIdentifier: () -> NSView?] = [:]

/// A view over the window's card and sidebar, where a child window above them would be, placed
/// from the window's top left corner, as the app counts.
@MainActor
final class Overlay {
    let view: NSView

    init(view: NSView) {
        self.view = view
        view.isHidden = true
    }

    var isVisible: Bool { view.superview != nil && !view.isHidden }

    /// Shows it at `frame`, in `parent`'s points from its top left corner, above the others there
    /// unless `raise` is off; or hides it without a frame. Raising it takes it out of the window a
    /// moment, which ends a drag in it.
    func place(in parent: NSWindow, _ frame: NSRect?, raise: Bool = true) {
        guard let frame, let over = overlays[ObjectIdentifier(parent)]?() else {
            view.isHidden = true
            return
        }
        if view.superview !== over || (raise && over.subviews.last !== view) {
            over.addSubview(view, positioned: .above, relativeTo: nil)
        }
        view.frame = frame
        view.isHidden = false
    }

    /// Gives it the keyboard.
    func makeKey() { view.window?.makeFirstResponder(view) }

    func close() { view.removeFromSuperview() }
}

/// A rounded surface with a blur under its content and a shadow, a view over the window's card
/// and sidebar.
@MainActor
final class Surface {
    /// How tall a beak is, and half as wide at its base.
    static let beakHeight: CGFloat = 10
    private static let beakHalfWidth: CGFloat = 13
    private let overlay: Overlay
    /// Casts its shadow, which its rounded content would clip.
    private let holder = NSView()
    private let effect = NSVisualEffectView()
    /// Its edge, a line just inside its outline.
    private let border = CAShapeLayer()
    /// A light line inside the edge, like a macOS panel's, for the surfaces that ask for it.
    private let inner = CAShapeLayer()
    private let host: NSView
    /// A color over its blur, under its content, which lightens it like Arc's site controls.
    private let tintView = NSView()
    var tint: NSColor? {
        didSet { tintView.layer?.backgroundColor = tint?.cgColor }
    }
    private let parent: NSWindow
    /// How rounded it is.
    var radius: CGFloat {
        didSet { if radius != oldValue { shape() } }
    }
    /// Where a beak on its top edge points, from its left edge, like Arc's site controls: its top
    /// `beakHeight` points are then the beak's, and its content is under them.
    var beak: CGFloat? {
        didSet { if beak != oldValue { shape() } }
    }
    /// Whether it grows from its beak, or its top's middle, as it shows, and shrinks back as it
    /// hides, like Arc's site controls.
    var pops = false
    /// Told whether it shows, each time the app shows or hides it.
    var visibility: (Bool) -> Void = { _ in }
    /// Hides it once it has shrunk.
    private var hiding: DispatchWorkItem?
    /// Called once the surface shows, to focus what it holds.
    var shown: () -> Void = {}
    /// Whether it takes the keyboard when it shows.
    var takesKeyboard = false
    /// Called when it shows with the keyboard and loses it: to another window, or to a click
    /// elsewhere in its own.
    var resigned: () -> Void = {}
    /// What had the keyboard before it took it, which gets it back as it hides.
    private weak var before: NSResponder?
    private var resignedKey: NSObjectProtocol?
    private var clicks: Any?

    init<Content: View>(
        in parent: NSWindow, radius: CGFloat, material: NSVisualEffectView.Material = .popover,
        @ViewBuilder view: () -> Content
    ) {
        self.parent = parent
        self.radius = radius
        effect.material = material
        // Over the page and the sidebar, in the same window: it blurs them.
        effect.blendingMode = .withinWindow
        effect.state = .active
        effect.autoresizingMask = [.width, .height]
        let host = NSHostingView(rootView: view())
        host.autoresizingMask = [.width, .height]
        host.safeAreaRegions = []
        self.host = host
        tintView.wantsLayer = true
        tintView.autoresizingMask = [.width, .height]
        effect.addSubview(tintView)
        effect.addSubview(host)
        holder.wantsLayer = true
        holder.layer?.shadowColor = NSColor.black.cgColor
        holder.layer?.shadowOpacity = 0.3
        holder.layer?.shadowRadius = 16
        holder.layer?.shadowOffset = .zero
        holder.addSubview(effect)
        // Half its stroke shows: the half inside the outline.
        border.fillColor = nil
        border.strokeColor = NSColor.separatorColor.cgColor
        border.lineWidth = 1
        border.mask = CAShapeLayer()
        inner.fillColor = nil
        inner.lineWidth = 3
        inner.mask = CAShapeLayer()
        inner.isHidden = true
        // Above its blur, whose layer AppKit keeps on top of the ones added by hand.
        inner.zPosition = 1
        border.zPosition = 1
        holder.layer?.addSublayer(inner)
        holder.layer?.addSublayer(border)
        overlay = Overlay(view: holder)
        resignedKey = NotificationCenter.default.addObserver(
            forName: NSWindow.didResignKeyNotification, object: parent, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self, self.takesKeyboard, self.isShown else { return }
                self.resigned()
            }
        }
        clicks = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) {
            [weak self] event in
            if let self, self.takesKeyboard, self.isShown, !self.contains(event) {
                DispatchQueue.main.async { self.resigned() }
            }
            return event
        }
    }

    var isShown: Bool { overlay.isVisible }

    /// Edges it like a macOS panel, as Arc's Command Bar: a black line outside a light one.
    func panelEdge() {
        border.strokeColor = NSColor.black.cgColor
        inner.strokeColor = NSColor.white.withAlphaComponent(0.19).cgColor
        inner.isHidden = false
    }

    /// Takes its edge away, like Arc's find bar.
    func edgeless() { border.isHidden = true }

    /// Casts its shadow `radius` points around it, `drop` points down, at `opacity`.
    func shadow(opacity: Float, radius: CGFloat, drop: CGFloat) {
        holder.layer?.shadowOpacity = opacity
        holder.layer?.shadowRadius = radius
        // AppKit's layers count up.
        holder.layer?.shadowOffset = CGSize(width: 0, height: -drop)
    }

    /// Whether `event` is a click in it.
    func contains(_ event: NSEvent) -> Bool {
        isShown && event.window === parent
            && holder.bounds.contains(holder.convert(event.locationInWindow, from: nil))
    }

    /// Takes it away.
    func close() {
        resignedKey.map(NotificationCenter.default.removeObserver)
        if let clicks { NSEvent.removeMonitor(clicks) }
        overlay.close()
    }

    /// Shows the surface at `frame`, from the window's top left corner, or hides it.
    func place(_ shown: Bool, _ frame: NSRect) {
        visibility(shown)
        // Already shrinking away.
        if !shown, hiding != nil { return }
        // Shown again as it shrinks, it grows back.
        let shrunk = hiding != nil
        hiding?.cancel()
        hiding = nil
        if pops, !shown, isShown {
            pop(growing: false)
            let hide = DispatchWorkItem { [weak self] in
                self?.hiding = nil
                self?.apply(false, frame)
            }
            hiding = hide
            DispatchQueue.main.asyncAfter(deadline: .now() + Self.shrinking, execute: hide)
            return
        }
        let showing = shown && !isShown
        apply(shown, frame)
        if pops, showing || shrunk { pop(growing: true) }
    }

    /// How long it takes to shrink away, like Arc's site controls.
    private static let shrinking = 0.1

    /// Grows it from its beak, or shrinks it back, like Arc's site controls, measured from a
    /// recording: from half its size with a critically damped spring settling in about 0.2 s,
    /// fading in over 0.08 s; back to 0.6 of its size, fading out, over 0.1 s.
    private func pop(growing: Bool) {
        guard let layer = holder.layer else { return }
        let bounds = holder.bounds
        // About its beak's tip, at its top: AppKit anchors a view's layer at its bottom left
        // corner, y going up.
        let tip = CGPoint(x: beak ?? bounds.midX, y: bounds.maxY)
        let small = growing ? 0.5 : 0.6
        var scaled = CATransform3DMakeTranslation(tip.x, tip.y, 0)
        scaled = CATransform3DScale(scaled, small, small, 1)
        scaled = CATransform3DTranslate(scaled, -tip.x, -tip.y, 0)
        let size: CABasicAnimation
        let fade = CABasicAnimation(keyPath: "opacity")
        if growing {
            let spring = CASpringAnimation(keyPath: "transform")
            spring.stiffness = 500
            spring.damping = 45
            spring.duration = spring.settlingDuration
            size = spring
            fade.duration = 0.08
            fade.timingFunction = CAMediaTimingFunction(name: .easeOut)
        } else {
            size = CABasicAnimation(keyPath: "transform")
            size.duration = Self.shrinking
            size.timingFunction = CAMediaTimingFunction(name: .easeIn)
            fade.duration = Self.shrinking
            fade.timingFunction = size.timingFunction
        }
        let (from, to) = growing ? (scaled, CATransform3DIdentity) : (CATransform3DIdentity, scaled)
        size.fromValue = NSValue(caTransform3D: from)
        size.toValue = NSValue(caTransform3D: to)
        fade.fromValue = growing ? 0 : 1
        fade.toValue = growing ? 1 : 0
        let group = CAAnimationGroup()
        group.animations = [size, fade]
        group.duration = max(size.duration, fade.duration)
        // Shrunk, it stays so until it hides.
        group.fillMode = .forwards
        group.isRemovedOnCompletion = growing
        layer.add(group, forKey: "pop")
    }

    /// Shows or hides it at once.
    private func apply(_ shown: Bool, _ frame: NSRect) {
        let showing = shown && !isShown
        if showing { before = parent.firstResponder }
        let had = (parent.firstResponder as? NSView)?.isDescendant(of: holder) == true
        overlay.place(in: parent, shown ? frame : nil)
        if shown { shape() }
        // Hiding, it gives the keyboard back to what had it.
        if !shown, had {
            parent.makeFirstResponder((before as? NSView).flatMap { $0.window === parent ? $0 : nil })
        }
        guard showing else { return }
        if takesKeyboard { parent.makeKey() }
        // Once its views are made, which they are on first showing.
        DispatchQueue.main.async { self.focus() }
    }

    /// Fits its blur, edge, shadow and content to its outline.
    private func shape() {
        let bounds = holder.bounds
        // Not placed yet.
        guard bounds.height > Self.beakHeight else { return }
        let outline = outline(in: bounds)
        effect.maskImage = NSImage(size: bounds.size, flipped: false) { _ in
            NSColor.black.setFill()
            NSBezierPath(cgPath: outline).fill()
            return true
        }
        border.frame = bounds
        border.path = outline
        (border.mask as? CAShapeLayer)?.path = outline
        inner.frame = bounds
        inner.path = outline
        (inner.mask as? CAShapeLayer)?.path = outline
        holder.layer?.shadowPath = outline
        tintView.frame = bounds
        let mask = CAShapeLayer()
        mask.path = outline
        tintView.layer?.mask = mask
        var content = bounds
        if beak != nil { content.size.height -= Self.beakHeight }
        host.frame = content
    }

    /// Its outline in `bounds`: a rounded rectangle, with its beak on top if it has one, a bump
    /// with a rounded tip, like Arc's.
    private func outline(in bounds: CGRect) -> CGPath {
        guard let beak else {
            return CGPath(
                roundedRect: bounds, cornerWidth: radius, cornerHeight: radius, transform: nil)
        }
        let body = bounds.divided(atDistance: Self.beakHeight, from: .maxYEdge).remainder
        let half = Self.beakHalfWidth
        // Within its top edge's straight part.
        let x = min(max(beak, body.minX + radius + half), body.maxX - radius - half)
        let (top, tip) = (body.maxY, bounds.maxY)
        let path = CGMutablePath()
        path.move(to: CGPoint(x: body.minX + radius, y: top))
        path.addLine(to: CGPoint(x: x - half, y: top))
        path.addCurve(
            to: CGPoint(x: x, y: tip), control1: CGPoint(x: x - 8, y: top),
            control2: CGPoint(x: x - 5, y: tip))
        path.addCurve(
            to: CGPoint(x: x + half, y: top), control1: CGPoint(x: x + 5, y: tip),
            control2: CGPoint(x: x + 8, y: top))
        let corners = [
            (CGPoint(x: body.maxX, y: top), CGPoint(x: body.maxX, y: body.minY)),
            (CGPoint(x: body.maxX, y: body.minY), CGPoint(x: body.minX, y: body.minY)),
            (CGPoint(x: body.minX, y: body.minY), CGPoint(x: body.minX, y: top)),
            (CGPoint(x: body.minX, y: top), CGPoint(x: body.maxX, y: top)),
        ]
        for (corner, next) in corners {
            path.addArc(tangent1End: corner, tangent2End: next, radius: radius)
        }
        path.closeSubpath()
        return path
    }

    /// Focuses what it holds, or itself, if it takes the keyboard.
    private func focus() {
        shown()
        guard takesKeyboard, (parent.firstResponder as? NSView)?.isDescendant(of: holder) != true
        else { return }
        parent.makeFirstResponder(host)
    }
}

@_cdecl("arbalete_ui_surface")
public func placeSurface(
    _ id: UInt32, _ name: UnsafePointer<CChar>, _ shown: Bool, _ x: Double, _ y: Double,
    _ width: Double, _ height: Double
) {
    MainActor.assumeIsolated {
        nativeWindows[id]?.surfaces[String(cString: name)]?.place(
            shown, NSRect(x: x, y: y, width: width, height: height))
    }
}
