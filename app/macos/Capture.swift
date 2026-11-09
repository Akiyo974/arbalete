// Page captures, like Arc's (capture in app/src/shell/capture.rs hands the page's picture over).
// - A part of the page: the page turns grey under a veil, where a drag picks the part, in color,
//   then a menu copies it, saves it, edits it (CaptureEditor.swift) or picks another part.
// - The whole page, from the developer bar: framed on a color, copied at once, with a panel under
//   the button to pick the color, and a toast to save it.

import AppKit
import CoreImage
import SwiftUI
import UniformTypeIdentifiers

/// Puts a picture on the clipboard, as PNG.
@MainActor func copyImage(_ image: CGImage) {
    guard let png = pngData(image) else { return }
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.setData(png, forType: .png)
}

func pngData(_ image: CGImage) -> Data? {
    NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
}

/// Asks where to save a picture, named after the page, and saves it there as PNG.
@MainActor func saveImage(_ image: CGImage, named name: String, in window: NSWindow?) {
    let panel = NSSavePanel()
    panel.allowedContentTypes = [.png]
    let cleaned = name.replacingOccurrences(of: "/", with: "-").trimmingCharacters(in: .whitespaces)
    panel.nameFieldStringValue = cleaned.isEmpty ? "Capture.png" : "\(cleaned).png"
    let save = { (response: NSApplication.ModalResponse) in
        guard response == .OK, let url = panel.url, let png = pngData(image) else { return }
        try? png.write(to: url)
    }
    if let window { panel.beginSheetModal(for: window, completionHandler: save) } else {
        save(panel.runModal())
    }
}

@MainActor
final class CaptureChrome {
    private let parent: NSWindow
    private let model: AppModel
    private let veil = Veil()
    private let veilOverlay: Overlay
    private let actions: Overlay
    private let toasts: Toasts
    private let framing = FramingModel()
    private let framePanel: Surface
    private var editor: CaptureEditor?
    /// The marks the editor drew on the part picked, which it edits again.
    private var marks: [Mark] = []
    /// The page's frame and its picture, while a capture shows.
    private var frame = NSRect.zero
    /// What the page told of itself as its picture was taken, for a part picked: where it was
    /// scrolled, for an Easel's live capture.
    private var blocks: PageBlocks?
    /// Where the developer bar's capture button is, from the window's top left corner: the
    /// panel framing the page shows under it.
    var button: NSRect?

    init(in parent: NSWindow, model: AppModel, toasts: Toasts) {
        self.parent = parent
        self.model = model
        self.toasts = toasts
        veilOverlay = Overlay(view: veil)
        let menu = NSHostingView(rootView: CaptureMenu(veil: veil))
        menu.safeAreaRegions = []
        menu.sizingOptions = []
        actions = Overlay(view: menu)
        let framing = framing
        framePanel = Surface(in: parent, radius: 16) { FramingView(model: framing) }
        framePanel.takesKeyboard = true
        framePanel.resigned = { [weak self] in self?.closeFraming() }
        framing.capture = { [weak self] in self?.copyFramed() }
        veil.picked = { [weak self] rect in self?.picked(rect) }
        veil.done = { [weak self] in self?.closeVeil() }
        veil.copy = { [weak self] in self?.copyPart() }
        veil.save = { [weak self] in self?.savePart() }
        veil.library = { [weak self] in self?.keepPart() }
        veil.easel = { [weak self] in self?.addToEasel() }
        veil.edit = { [weak self] in self?.edit() }
        veil.scroll = { dx, dy, point in
            model.run("scroll-capture", dx, dy, Int(point.x), Int(point.y))
        }
    }

    /// The page's title, which a saved capture is named after.
    private var title: String { model.activeTab?.label ?? "Capture" }

    /// Shows the page's picture, `png`, over the page at `frame`: in the veil to pick a part of,
    /// or framed on a color in the panel under the button, like Arc's Portrait Mode.
    func show(_ png: Data, area: Bool, frame: NSRect, blocks: PageBlocks?) {
        guard let image = NSBitmapImageRep(data: png)?.cgImage else { return }
        self.frame = frame
        if area {
            self.blocks = blocks
            closeFraming()
            let scale = blocks.map { frame.width / max($0.width, 1) } ?? 1
            let rects = (blocks?.rects ?? []).compactMap { rect -> NSRect? in
                guard rect.count == 4 else { return nil }
                return NSRect(
                    x: rect[0] * scale, y: rect[1] * scale, width: rect[2] * scale,
                    height: rect[3] * scale)
            }
            // Taken again after a scroll, it shows the page as it is now.
            if veilOverlay.isVisible, editor == nil {
                veil.update(image, blocks: rects)
                return
            }
            veil.show(image, blocks: rects)
            veilOverlay.place(in: parent, frame)
            veilOverlay.makeKey()
        } else {
            closeVeil()
            framing.shot = image
            framing.websiteColor = topColor(image)
            // Measured on Arc's: 258 points wide, its beak 10 points tall over its body.
            let size = NSSize(width: 258, height: 219 + Surface.beakHeight)
            let anchor = button ?? NSRect(x: frame.maxX - 60, y: frame.minY - 30, width: 28, height: 28)
            let x = min(max(frame.minX + 8, anchor.midX - size.width / 2), frame.maxX - size.width - 8)
            framePanel.beak = anchor.midX - x
            framePanel.pops = true
            framePanel.place(
                true, NSRect(x: x, y: anchor.maxY + 2, width: size.width, height: size.height))
        }
    }

    /// Copies all of the page, from the site controls, which a toast tells, with a button saving
    /// it.
    func copyWhole(_ png: Data, frame: NSRect) {
        guard let image = NSBitmapImageRep(data: png)?.cgImage else { return }
        self.frame = frame
        copyImage(image)
        tell("Full Page Copied to Clipboard", saving: image)
    }

    // MARK: A part of the page

    /// Shows the menu inside the part picked, at its top left, like Arc's, or takes it away while
    /// another is picked.
    private func picked(_ rect: NSRect?) {
        guard let rect else {
            actions.place(in: parent, nil)
            marks = []
            return
        }
        let size = CaptureMenu.size
        let x = min(frame.minX + rect.minX + 10, frame.maxX - size.width - 4)
        let y = min(frame.minY + rect.minY + 10, frame.maxY - size.height - 4)
        actions.place(in: parent, NSRect(x: x, y: y, width: size.width, height: size.height))
    }

    private func copyPart() {
        guard let image = veil.part else { return }
        copyImage(image)
        closeVeil()
        tell("Copied Image")
    }

    /// Keeps the part picked in the library's Media, like Arc's Save to Library.
    private func keepPart() {
        guard let image = veil.part, let png = pngData(image) else { return }
        closeVeil()
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString + ".png")
        guard (try? png.write(to: file)) != nil else { return }
        model.run("save-capture-to-library", file.path)
        tell("Saved to Library")
    }

    /// Adds the part picked to an Easel, like Arc's: a menu at the pointer lists the displayed
    /// Space's, then a new one (decision 0009). An Easel's board shown adds it itself.
    private func addToEasel() {
        guard let image = veil.part, let content = parent.contentView else { return }
        let page = pageView()
        let easels = (model.page.map { $0.pinned + $0.today } ?? []).flatMap(\.tabs)
            .filter { $0.url.hasPrefix("arbalete://easel/") }
        let menu = NSMenu()
        for easel in easels {
            let item = NSMenuItem(title: easel.label, action: nil, keyEquivalent: "")
            item.image = NSImage(systemSymbolName: "scribble", accessibilityDescription: nil)
            menu.addItem(ClosureMenuItem(item) { [weak self] in self?.add(image, page, to: easel) })
        }
        if !easels.isEmpty { menu.addItem(.separator()) }
        let new = NSMenuItem(title: "New Easel", action: nil, keyEquivalent: "")
        new.image = NSImage(systemSymbolName: "plus", accessibilityDescription: nil)
        menu.addItem(ClosureMenuItem(new) { [weak self] in self?.add(image, page, to: nil) })
        let point = content.convert(parent.mouseLocationOutsideOfEventStream, from: nil)
        menu.popUp(positioning: nil, at: point, in: content)
    }

    /// Where the part picked is on the page, in its CSS pixels, once the page told where it was
    /// scrolled.
    private func pageView() -> EaselPage? {
        guard let blocks, let height = blocks.height, let scrollX = blocks.scrollX,
            let scrollY = blocks.scrollY, let selection = veil.selection, frame.width > 0
        else { return nil }
        let css = blocks.width / frame.width
        let part = CGRect(
            x: selection.minX * css, y: selection.minY * css, width: selection.width * css,
            height: selection.height * css)
        return EaselPage(
            width: blocks.width, height: height, scrollX: scrollX, scrollY: scrollY,
            part: EaselRect(part))
    }

    private func add(_ image: CGImage, _ page: EaselPage?, to easel: Tab?) {
        closeVeil()
        let scale = parent.backingScaleFactor
        let size = CGSize(width: CGFloat(image.width) / scale, height: CGFloat(image.height) / scale)
        if let easel, let board = pageContainers[easel.id]?.board {
            let tab = model.activeTab
            board.canvas.addCapture(
                image, size: size, url: tab?.url ?? "", title: tab?.label ?? "",
                icon: uiImage(tab?.favicon), page: page)
            // Like Arc's, the Easel shows in Peek, unless it shows already.
            guard board.isHiddenOrHasHiddenAncestor else { return tell("Added to Easel") }
            return model.run("peek-easel", easel.id)
        }
        guard let png = pngData(image) else { return }
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString + ".png")
        guard (try? png.write(to: file)) != nil else { return }
        let number = easel.flatMap { Int($0.url.dropFirst("arbalete://easel/".count)) }
        model.run(
            "add-capture-to-easel", file.path, Double(size.width), Double(size.height),
            number as Any? ?? NSNull(), page.map { jsonObject($0) } ?? NSNull())
    }

    private func savePart() {
        guard let image = veil.part else { return }
        closeVeil()
        saveImage(image, named: title, in: parent)
    }

    /// Edits the part picked, like Arc's: it zooms out of the page into the editor, over the
    /// whole window, and back with its marks, the menu showing again.
    private func edit() {
        guard let image = veil.captured, let selection = veil.selection,
            let content = parent.contentView
        else { return }
        actions.place(in: parent, nil)
        let from = selection.offsetBy(dx: frame.minX, dy: frame.minY)
        let editor = CaptureEditor(image: image, marks: marks, from: from) { [weak self] marks in
            guard let self else { return }
            self.editor?.overlay.close()
            self.editor = nil
            self.marks = marks
            self.veil.edited = flatten(image, marks)
            self.veilOverlay.makeKey()
            self.picked(selection)
        }
        self.editor = editor
        editor.overlay.place(in: parent, CGRect(origin: .zero, size: content.bounds.size))
        editor.open()
    }

    private func closeVeil() {
        editor?.overlay.close()
        editor = nil
        actions.place(in: parent, nil)
        veilOverlay.place(in: parent, nil)
        veil.clear()
        marks = []
    }

    // MARK: The whole page, framed

    /// Copies the framed page, like Arc's "Capture in Portrait Mode", which a toast in the bar
    /// tells, with a button saving it.
    private func copyFramed() {
        guard let image = framing.framed else { return }
        copyImage(image)
        tell("Screenshot Copied to Clipboard", saving: image, at: NSPoint(x: frame.maxX - 10, y: frame.minY - 33))
    }

    private func closeFraming() {
        framePanel.place(false, .zero)
    }

    // MARK: Toasts

    /// Tells `text` at the page's top right, or at `corner`, with a button saving `image`.
    private func tell(_ text: String, saving image: CGImage? = nil, at corner: NSPoint? = nil) {
        let save = image.map { image in
            { [weak self] in
                guard let self else { return }
                saveImage(image, named: self.title, in: self.parent)
            }
        }
        let toast = Toast(text: text, button: image == nil ? nil : "Save", action: save)
        if let corner { toasts.show(toast, at: corner) } else { toasts.show(toast, over: frame) }
    }

    func close() {
        closeVeil()
        veilOverlay.close()
        actions.close()
        framePanel.close()
    }
}

/// The color at a picture's top, its first rows' average: the website's color, which frames it.
private func topColor(_ image: CGImage) -> NSColor {
    let rows = min(8, image.height)
    guard let top = image.cropping(to: CGRect(x: 0, y: 0, width: image.width, height: rows))
    else { return .systemIndigo }
    let ci = CIImage(cgImage: top)
    let filter = CIFilter(name: "CIAreaAverage")
    filter?.setValue(ci, forKey: kCIInputImageKey)
    filter?.setValue(CIVector(cgRect: ci.extent), forKey: kCIInputExtentKey)
    guard let output = filter?.outputImage else { return .systemIndigo }
    var pixel = [UInt8](repeating: 0, count: 4)
    CIContext().render(
        output, toBitmap: &pixel, rowBytes: 4, bounds: CGRect(x: 0, y: 0, width: 1, height: 1),
        format: .RGBA8, colorSpace: CGColorSpace(name: CGColorSpace.sRGB))
    return NSColor(
        srgbRed: CGFloat(pixel[0]) / 255, green: CGFloat(pixel[1]) / 255,
        blue: CGFloat(pixel[2]) / 255, alpha: 1)
}

/// A picture without its colors.
private func greyed(_ image: CGImage) -> CGImage {
    let ci = CIImage(cgImage: image).applyingFilter(
        "CIColorControls", parameters: [kCIInputSaturationKey: 0, kCIInputBrightnessKey: -0.05])
    return CIContext().createCGImage(ci, from: ci.extent) ?? image
}

/// The page's picture, grey, where a drag picks the part to capture, in color, like Arc's;
/// Escape gives it up.
@MainActor
final class Veil: NSView {
    var picked: (NSRect?) -> Void = { _ in }
    var done: () -> Void = {}
    var copy: () -> Void = {}
    var save: () -> Void = {}
    var library: () -> Void = {}
    var easel: () -> Void = {}
    var edit: () -> Void = {}
    /// Scrolls the page under it, by a wheel's deltas, at a point of it.
    var scroll: (CGFloat, CGFloat, NSPoint) -> Void = { _, _, _ in }
    private var shot: CGImage?
    private var grey: CGImage?
    private var start: NSPoint?
    /// The page's blocks that show, from its top left corner, which a click picks, and the one
    /// under the pointer, in color, like Arc's.
    private var blocks: [NSRect] = []
    private var hovered: NSRect?
    private var dragged = false
    /// The part picked, from the veil's top left corner.
    private(set) var selection: NSRect?
    /// The part picked once edited, which shows and copies instead.
    var edited: CGImage? {
        didSet { needsDisplay = true }
    }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    func show(_ image: CGImage, blocks: [NSRect]) {
        update(image, blocks: blocks)
        clear()
    }

    /// Shows the page as it is now, as it scrolled, with its blocks.
    func update(_ image: CGImage, blocks: [NSRect]) {
        shot = image
        grey = greyed(image)
        self.blocks = blocks
        if let window, selection == nil {
            hover(convert(window.mouseLocationOutsideOfEventStream, from: nil))
        }
        needsDisplay = true
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: .zero, options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                owner: self))
    }

    /// The smallest block under `point`, like Arc's, which shows in color.
    private func hover(_ point: NSPoint) {
        let block = blocks.filter { $0.contains(point) }
            .min { $0.width * $0.height < $1.width * $1.height }
            .map { $0.intersection(bounds) }
        if block != hovered {
            hovered = block
            needsDisplay = true
        }
    }

    override func mouseMoved(with event: NSEvent) {
        guard selection == nil, start == nil else { return }
        hover(convert(event.locationInWindow, from: nil))
    }

    override func mouseExited(with event: NSEvent) {
        hovered = nil
        needsDisplay = true
    }

    override func scrollWheel(with event: NSEvent) {
        guard selection == nil else { return }
        // Points the page scrolls by, the way the content moves.
        let factor: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 10
        scroll(
            -event.scrollingDeltaX * factor, -event.scrollingDeltaY * factor,
            convert(event.locationInWindow, from: nil))
    }

    func clear() {
        start = nil
        selection = nil
        edited = nil
        hovered = nil
        needsDisplay = true
    }

    /// The picture's pixels per point of the veil.
    private var scale: CGFloat {
        guard let shot, bounds.width > 0 else { return 1 }
        return CGFloat(shot.width) / bounds.width
    }

    /// The part picked, as captured or as edited.
    var part: CGImage? { edited ?? captured }

    /// The part picked, as captured.
    var captured: CGImage? {
        guard let shot, let selection else { return nil }
        let pixels = CGRect(
            x: selection.minX * scale, y: selection.minY * scale, width: selection.width * scale,
            height: selection.height * scale
        ).integral
        return shot.cropping(to: pixels)
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .crosshair)
    }

    override func mouseDown(with event: NSEvent) {
        // Another drag picks another part, like Retake.
        start = convert(event.locationInWindow, from: nil)
        selection = nil
        edited = nil
        dragged = false
        picked(nil)
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start else { return }
        let point = convert(event.locationInWindow, from: nil)
        guard dragged || hypot(point.x - start.x, point.y - start.y) > 3 else { return }
        dragged = true
        hovered = nil
        selection = NSRect(
            x: min(start.x, point.x), y: min(start.y, point.y), width: abs(point.x - start.x),
            height: abs(point.y - start.y)
        ).intersection(bounds)
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        start = nil
        // A click picks the block under it, like Arc's.
        if !dragged, let hovered {
            selection = hovered
            self.hovered = nil
        }
        guard let selection, selection.width > 4, selection.height > 4 else {
            self.selection = nil
            needsDisplay = true
            return
        }
        picked(selection)
    }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 53: done()
        case 36 where selection != nil: copy()
        default: super.keyDown(with: event)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext, let shot, let grey else { return }
        drawImage(grey, in: bounds, context)
        NSColor.black.withAlphaComponent(0.15).setFill()
        bounds.fill()
        guard let selection else {
            if let hovered {
                context.saveGState()
                context.clip(to: hovered)
                drawImage(shot, in: bounds, context)
                context.restoreGState()
            }
            hint()
            return
        }
        if let edited {
            drawImage(edited, in: selection, context)
        } else {
            context.saveGState()
            context.clip(to: selection)
            drawImage(shot, in: bounds, context)
            context.restoreGState()
        }
        NSColor.white.withAlphaComponent(0.9).setStroke()
        let outline = NSBezierPath(rect: selection.insetBy(dx: -0.5, dy: -0.5))
        outline.lineWidth = 1
        outline.stroke()
    }

    /// Draws `image` into `rect` of this flipped view.
    private func drawImage(_ image: CGImage, in rect: NSRect, _ context: CGContext) {
        context.saveGState()
        context.translateBy(x: rect.minX, y: rect.maxY)
        context.scaleBy(x: 1, y: -1)
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(origin: .zero, size: rect.size))
        context.restoreGState()
    }

    /// How to capture, in a white pill at the bottom, like Arc's.
    private func hint() {
        let text =
            (hovered == nil
                ? "Drag to capture a portion of this page"
                : "Click or drag to capture a portion of this page") as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 12, weight: .medium),
            .foregroundColor: NSColor(srgbRed: 0.27, green: 0.29, blue: 0.83, alpha: 1),
        ]
        let size = text.size(withAttributes: attributes)
        let pill = NSRect(
            x: bounds.midX - (size.width + 88) / 2, y: bounds.maxY - 64,
            width: size.width + 88, height: 34)
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.25)
        shadow.shadowBlurRadius = 10
        NSGraphicsContext.saveGraphicsState()
        shadow.set()
        NSColor.white.setFill()
        NSBezierPath(roundedRect: pill, xRadius: 10, yRadius: 10).fill()
        NSGraphicsContext.restoreGraphicsState()
        text.draw(
            at: NSPoint(x: pill.midX - size.width / 2, y: pill.midY - size.height / 2),
            withAttributes: attributes)
    }
}

/// What a part picked may become, inside it, like Arc's menu, measured: rows 40 points apart,
/// a 32-point hover inset 10 points.
private struct CaptureMenu: View {
    static let size = NSSize(width: 272, height: 2 * 6 + 6 * 40 + 9)
    let veil: Veil

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            CaptureMenuRow(symbol: "doc.on.doc", title: "Copy", action: veil.copy)
            CaptureMenuRow(
                symbol: "photo.on.rectangle.angled", title: "Save to Library", action: veil.library)
            CaptureMenuRow(symbol: "square.and.arrow.down", title: "Save…", action: veil.save)
            CaptureMenuRow(symbol: "scribble", title: "Add to Easel", action: veil.easel)
            Rectangle().fill(Color(red: 0x1F / 255, green: 0x1D / 255, blue: 0x34 / 255))
                .frame(height: 0.5).padding(.horizontal, 10).padding(.vertical, 4)
            CaptureMenuRow(symbol: "pencil.circle", title: "Edit", action: veil.edit)
            CaptureMenuRow(symbol: "arrow.counterclockwise", title: "Retake") {
                veil.clear()
                veil.picked(nil)
            }
        }
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(
            RoundedRectangle(cornerRadius: 10).fill(Color(red: 0x11 / 255, green: 0x0F / 255, blue: 0x26 / 255))
                .shadow(color: .black.opacity(0.35), radius: 12)
        )
        .environment(\.colorScheme, .dark)
    }
}

private struct CaptureMenuRow: View {
    let symbol: String
    let title: String
    let action: () -> Void
    private let _hovered = State(initialValue: false)

    var body: some View {
        HStack(spacing: 0) {
            Image(systemName: symbol).font(.system(size: 14))
                .foregroundStyle(Color(red: 0x4E / 255, green: 0x4C / 255, blue: 0x61 / 255))
                .frame(width: 16)
                .padding(.leading, 18)
            Text(title).font(.system(size: 14, weight: .medium))
                .foregroundStyle(Color(red: 0xE7 / 255, green: 0xE3 / 255, blue: 0xF0 / 255))
                .padding(.leading, 8)
            Spacer(minLength: 0)
        }
        .frame(height: 40)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(Color(red: 0x32 / 255, green: 0x30 / 255, blue: 0x5C / 255))
                .opacity(_hovered.wrappedValue ? 1 : 0)
                .frame(height: 32)
                .padding(.horizontal, 10))
        .contentShape(Rectangle())
        .onHover { _hovered.wrappedValue = $0 }
        .onTapGesture(perform: action)
    }
}

// MARK: The whole page, framed

@MainActor
final class FramingModel: ObservableObject {
    @Published var shot: CGImage?
    /// The frame's color: from the hue track, from 0 to 1; or the website's; or grey.
    enum Choice: Equatable { case hue(Double), website, grey }
    @Published var choice = Choice.website
    /// Whether it says what captured it, under the page.
    @Published var signed = true
    var websiteColor = NSColor.black
    var capture: () -> Void = {}

    /// The gradient the frame takes, from its top left corner to its bottom right.
    var colors: (NSColor, NSColor) {
        switch choice {
        case .hue(let t):
            let hue = trackHue(t)
            return (
                NSColor(hue: (hue + 0.97).truncatingRemainder(dividingBy: 1), saturation: 0.45, brightness: 1, alpha: 1),
                NSColor(hue: (hue + 0.04).truncatingRemainder(dividingBy: 1), saturation: 0.65, brightness: 0.95, alpha: 1)
            )
        case .website:
            return (websiteColor.blended(withFraction: 0.12, of: .white) ?? websiteColor, websiteColor)
        case .grey:
            return (NSColor(white: 0.36, alpha: 1), NSColor(white: 0.22, alpha: 1))
        }
    }

    /// The page's picture on its frame, like Arc's Portrait Mode.
    var framed: CGImage? {
        shot.flatMap { frameImage($0, colors: colors, signed: signed) }
    }
}

/// The hue at `t` along the track, like Arc's: from pink, through red, orange, yellow, green and
/// cyan, to blue.
private func trackHue(_ t: Double) -> CGFloat {
    CGFloat((0.92 + t * 0.70).truncatingRemainder(dividingBy: 1))
}

/// `image` framed on a gradient from `colors.0` to `colors.1`, rounded and edged in white, with
/// what captured it under it if `signed`.
func frameImage(_ image: CGImage, colors: (NSColor, NSColor), signed: Bool) -> CGImage? {
    let width = CGFloat(image.width)
    let height = CGFloat(image.height)
    let pad = (width * 0.085).rounded()
    let size = CGSize(width: width + 2 * pad, height: height + 2 * pad)
    guard
        let context = CGContext(
            data: nil, width: Int(size.width), height: Int(size.height), bitsPerComponent: 8,
            bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    let gradient = [colors.0, colors.1].map { ($0.usingColorSpace(.sRGB) ?? $0).cgColor } as CFArray
    if let gradient = CGGradient(colorsSpace: nil, colors: gradient, locations: [0, 1]) {
        context.drawLinearGradient(
            gradient, start: CGPoint(x: 0, y: size.height), end: CGPoint(x: size.width, y: 0),
            options: [])
    }
    let rect = CGRect(x: pad, y: pad, width: width, height: height)
    let radius = width * 0.015
    let path = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    context.saveGState()
    context.setShadow(
        offset: CGSize(width: 0, height: -width * 0.004), blur: width * 0.02,
        color: NSColor.black.withAlphaComponent(0.3).cgColor)
    context.addPath(path)
    context.setFillColor(NSColor.black.cgColor)
    context.fillPath()
    context.restoreGState()
    context.saveGState()
    context.addPath(path)
    context.clip()
    context.draw(image, in: rect)
    context.restoreGState()
    // A light edge, like Arc's.
    context.addPath(path)
    context.setStrokeColor(NSColor.white.withAlphaComponent(0.35).cgColor)
    context.setLineWidth(max(1.5, width * 0.003))
    context.strokePath()
    if signed {
        let caption = NSAttributedString(
            string: "Captured with Arbalete",
            attributes: [
                .font: NSFont.systemFont(ofSize: max(10, width * 0.011), weight: .medium),
                .foregroundColor: NSColor.white.withAlphaComponent(0.65),
            ])
        let line = CTLineCreateWithAttributedString(caption)
        let bounds = CTLineGetBoundsWithOptions(line, [])
        context.textPosition = CGPoint(x: (size.width - bounds.width) / 2, y: pad / 2 - bounds.height / 2)
        CTLineDraw(line, context)
    }
    return context.makeImage()
}

/// The panel under the Portrait Mode button, like Arc's, measured: a button capturing the page,
/// a preview of it on its frame, which a badge signs, then its frame's color: the website's, a
/// hue along a track, or grey.
private struct FramingView: View {
    @ObservedObject var model: FramingModel
    private let _hovered = State(initialValue: false)

    var body: some View {
        VStack(spacing: 0) {
            Button(action: model.capture) {
                Text("Capture in Portrait Mode")
                    .font(.system(size: 13.5))
                    .foregroundStyle(Color(white: 0.85))
                    .frame(maxWidth: .infinity)
                    .frame(height: 31)
                    .background(
                        RoundedRectangle(cornerRadius: 8).fill(
                            Color(white: _hovered.wrappedValue ? 0.18 : 0.11)))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { _hovered.wrappedValue = $0 }
            preview
                .padding(.top, 8)
            HStack(spacing: 0) {
                dot(.website, Color(nsColor: model.websiteColor), help: "Use Website Color")
                FrameHueTrack(choice: $model.choice)
                    .padding(.horizontal, 10)
                dot(.grey, Color(white: 0.24), help: "Grey")
            }
            .frame(height: 24)
            .padding(.top, 13)
        }
        .padding(.horizontal, 9)
        .padding(.top, 11)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .environment(\.colorScheme, .dark)
    }

    /// The page, small, centered on its frame's gradient, with the badge at the bottom left.
    private var preview: some View {
        let colors = model.colors
        return ZStack(alignment: .bottomLeading) {
            LinearGradient(
                colors: [Color(nsColor: colors.0), Color(nsColor: colors.1)],
                startPoint: .topLeading, endPoint: .bottomTrailing)
            if let shot = model.shot {
                Image(decorative: shot, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                    .overlay(RoundedRectangle(cornerRadius: 4).stroke(.white.opacity(0.35), lineWidth: 1.5))
                    .padding(.vertical, 14)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            Button {
                model.signed.toggle()
            } label: {
                Image(systemName: "signature")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.white.opacity(model.signed ? 0.9 : 0.4))
                    .frame(width: 32, height: 32)
                    .background(RoundedRectangle(cornerRadius: 7).fill(.white.opacity(0.16)))
            }
            .buttonStyle(.plain)
            .help(model.signed ? "Remove \"Captured with Arbalete\"" : "Add \"Captured with Arbalete\"")
            .padding(4)
        }
        .frame(height: 128)
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    private func dot(_ choice: FramingModel.Choice, _ color: Color, help: String) -> some View {
        Button {
            model.choice = choice
        } label: {
            Circle().fill(color)
                .overlay(Circle().stroke(.white, lineWidth: model.choice == choice ? 3 : 0))
                .frame(width: 18, height: 18)
                .frame(width: 24, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
    }
}

/// The rainbow track, like Arc's: pink to blue, its white knob on the hue picked.
private struct FrameHueTrack: View {
    @Binding var choice: FramingModel.Choice

    var body: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(
                        LinearGradient(
                            colors: stride(from: 0.0, through: 1.0, by: 0.05).map {
                                Color(hue: Double(trackHue($0)), saturation: 0.5, brightness: 1)
                            }, startPoint: .leading, endPoint: .trailing)
                    )
                    .frame(height: 10)
                if case .hue(let t) = choice {
                    Circle().fill(.white).frame(width: 22, height: 22)
                        .shadow(color: .black.opacity(0.3), radius: 2)
                        .offset(x: t * (width - 22))
                }
            }
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0).onChanged { value in
                    choice = .hue(min(max(Double((value.location.x - 11) / (width - 22)), 0), 1))
                })
        }
    }
}

/// The page's blocks that show, as the app lists them: the page's width, and their rects in its
/// points.
struct PageBlocks: Decodable {
    var width: Double
    var height: Double?
    var scrollX: Double?
    var scrollY: Double?
    var rects: [[Double]]
}

@_cdecl("arbalete_ui_captured")
public func captured(
    _ id: UInt32, _ base64: UnsafePointer<CChar>, _ mode: UInt32, _ x: Double, _ y: Double,
    _ width: Double, _ height: Double, _ blocks: UnsafePointer<CChar>
) {
    guard let png = Data(base64Encoded: String(cString: base64)) else { return }
    let blocks = try? JSONDecoder().decode(PageBlocks.self, from: Data(String(cString: blocks).utf8))
    MainActor.assumeIsolated {
        let frame = NSRect(x: x, y: y, width: width, height: height)
        guard let capture = nativeWindows[id]?.capture else { return }
        if mode == 2 {
            capture.copyWhole(png, frame: frame)
        } else {
            capture.show(png, area: mode == 1, frame: frame, blocks: blocks)
        }
    }
}

/// A menu item that runs a closure.
@MainActor
final class ClosureMenuItem: NSMenuItem {
    private let run: () -> Void

    init(_ item: NSMenuItem, run: @escaping () -> Void) {
        self.run = run
        super.init(title: item.title, action: #selector(chosen), keyEquivalent: "")
        target = self
        image = item.image
    }

    required init(coder: NSCoder) { fatalError() }

    @objc private func chosen() { run() }
}
