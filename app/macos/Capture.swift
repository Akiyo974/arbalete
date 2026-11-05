// Page captures, like Arc's (capture in app/src/shell.rs hands the page's picture over).
// - A part of the page: the page turns grey under a veil, where a drag picks the part, in color,
//   then a menu copies it, saves it, edits it (CaptureEditor.swift) or picks another part.
// - The whole page, from the developer bar: framed on a color, copied at once, with a panel under
//   the button to pick the color, and a note to save it.

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
    private let note: Overlay
    private let noteModel = NoteModel()
    private var hiding: DispatchWorkItem?
    private let framing = FramingModel()
    private let framePanel: Surface
    private var editor: CaptureEditor?
    /// The marks the editor drew on the part picked, which it edits again.
    private var marks: [Mark] = []
    /// The page's frame and its picture, while a capture shows.
    private var frame = NSRect.zero
    /// Where the developer bar's capture button is, from the window's top left corner: the
    /// panel framing the page shows under it.
    var button: NSRect?

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        self.model = model
        veilOverlay = Overlay(view: veil)
        let menu = NSHostingView(rootView: CaptureMenu(veil: veil))
        menu.safeAreaRegions = []
        menu.sizingOptions = []
        actions = Overlay(view: menu)
        let host = NSHostingView(rootView: NoteView(model: noteModel))
        host.safeAreaRegions = []
        host.sizingOptions = []
        note = Overlay(view: host)
        let framing = framing
        framePanel = Surface(in: parent, radius: 12) { FramingView(model: framing) }
        framePanel.takesKeyboard = true
        framePanel.resigned = { [weak self] in self?.closeFraming() }
        framing.changed = { [weak self] in self?.copyFramed() }
        noteModel.save = { [weak self] in self?.saveFramed() }
        veil.picked = { [weak self] rect in self?.picked(rect) }
        veil.done = { [weak self] in self?.closeVeil() }
        veil.copy = { [weak self] in self?.copyPart() }
        veil.save = { [weak self] in self?.savePart() }
        veil.edit = { [weak self] in self?.edit() }
    }

    /// The page's title, which a saved capture is named after.
    private var title: String { model.activeTab?.label ?? "Capture" }

    /// Shows the page's picture, `png`, over the page at `frame`: in the veil to pick a part of,
    /// or framed on a color, copied at once.
    func show(_ png: Data, area: Bool, frame: NSRect) {
        guard let image = NSBitmapImageRep(data: png)?.cgImage else { return }
        self.frame = frame
        if area {
            closeFraming()
            veil.show(image)
            veilOverlay.place(in: parent, frame)
            veilOverlay.makeKey()
        } else {
            closeVeil()
            framing.shot = image
            framing.websiteColor = topColor(image)
            framing.hue = nil
            copyFramed()
            let size = NSSize(width: 250, height: 206)
            let anchor = button ?? NSRect(x: frame.maxX - 60, y: frame.minY, width: 28, height: 28)
            let x = min(max(frame.minX + 8, anchor.midX - size.width / 2), frame.maxX - size.width - 8)
            framePanel.place(
                true, NSRect(x: x, y: anchor.maxY + 6, width: size.width, height: size.height))
            tell(
                "Screenshot Copied to Clipboard", saving: true,
                at: NSPoint(x: frame.maxX, y: frame.minY + 6))
        }
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
        tell("Copied Image", saving: false, at: NSPoint(x: frame.maxX, y: frame.minY + 6))
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

    private func copyFramed() {
        if let image = framing.framed { copyImage(image) }
    }

    private func saveFramed() {
        guard let image = framing.framed else { return }
        saveImage(image, named: title, in: parent)
    }

    private func closeFraming() {
        framePanel.place(false, .zero)
    }

    // MARK: Notes

    /// Shows `text` a moment, with a button saving the framed capture if `saving`, its top right
    /// corner at `corner`, in the window's points from its top left corner.
    func tell(_ text: String, saving: Bool = false, at corner: NSPoint) {
        noteModel.text = text
        noteModel.saving = saving
        let size = NSSize(width: 380, height: 42)
        note.place(
            in: parent,
            NSRect(x: corner.x - size.width, y: corner.y, width: size.width, height: size.height),
            raise: true)
        hiding?.cancel()
        let hide = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.note.place(in: self.parent, nil)
        }
        hiding = hide
        DispatchQueue.main.asyncAfter(deadline: .now() + 4, execute: hide)
    }

    func close() {
        closeVeil()
        veilOverlay.close()
        actions.close()
        note.close()
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
    var edit: () -> Void = {}
    private var shot: CGImage?
    private var grey: CGImage?
    private var start: NSPoint?
    /// The part picked, from the veil's top left corner.
    private(set) var selection: NSRect?
    /// The part picked once edited, which shows and copies instead.
    var edited: CGImage? {
        didSet { needsDisplay = true }
    }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    func show(_ image: CGImage) {
        shot = image
        grey = greyed(image)
        clear()
    }

    func clear() {
        start = nil
        selection = nil
        edited = nil
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
        picked(nil)
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start else { return }
        let point = convert(event.locationInWindow, from: nil)
        selection = NSRect(
            x: min(start.x, point.x), y: min(start.y, point.y), width: abs(point.x - start.x),
            height: abs(point.y - start.y)
        ).intersection(bounds)
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        start = nil
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
        let text = "Drag to capture a portion of this page" as NSString
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
    static let size = NSSize(width: 272, height: 2 * 6 + 4 * 40 + 9)
    let veil: Veil

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            CaptureMenuRow(symbol: "doc.on.doc", title: "Copy", action: veil.copy)
            CaptureMenuRow(symbol: "square.and.arrow.down", title: "Save…", action: veil.save)
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
    /// The frame's hue, from 0 to 1, or the website's color without.
    @Published var hue: Double? {
        didSet { changed() }
    }
    var websiteColor = NSColor.systemIndigo
    var changed: () -> Void = {}

    var color: NSColor {
        guard let hue else { return websiteColor }
        return NSColor(hue: hue, saturation: 0.55, brightness: 0.85, alpha: 1)
    }

    /// The page's picture on its frame's color, like Arc's captures.
    var framed: CGImage? {
        shot.flatMap { frameImage($0, color: color) }
    }
}

/// `image` framed on a gradient of `color`, rounded, with what captured it under it.
func frameImage(_ image: CGImage, color: NSColor) -> CGImage? {
    let width = CGFloat(image.width)
    let height = CGFloat(image.height)
    let pad = (width * 0.06).rounded()
    let size = CGSize(width: width + 2 * pad, height: height + 2 * pad)
    guard
        let context = CGContext(
            data: nil, width: Int(size.width), height: Int(size.height), bitsPerComponent: 8,
            bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    let base = color.usingColorSpace(.sRGB) ?? color
    let light = base.blended(withFraction: 0.35, of: .white) ?? base
    let colors = [light.cgColor, base.cgColor] as CFArray
    if let gradient = CGGradient(colorsSpace: nil, colors: colors, locations: [0, 1]) {
        context.drawLinearGradient(
            gradient, start: CGPoint(x: 0, y: size.height), end: CGPoint(x: size.width, y: 0),
            options: [])
    }
    let rect = CGRect(x: pad, y: pad, width: width, height: height)
    let radius = width * 0.012
    let path = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -width * 0.004), blur: width * 0.02,
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
    context.addPath(path)
    context.setStrokeColor(NSColor.white.withAlphaComponent(0.15).cgColor)
    context.setLineWidth(max(1, width * 0.001))
    context.strokePath()
    // What captured it, small, centered under it.
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
    return context.makeImage()
}

/// The panel under the capture button, like Arc's: the framed page, and its frame's color, from
/// a hue slider or the website's.
private struct FramingView: View {
    @ObservedObject var model: FramingModel

    var body: some View {
        VStack(spacing: 8) {
            Text("Capture in Portrait Mode").font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
            if let framed = model.framed {
                Image(decorative: framed, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(maxWidth: .infinity, maxHeight: 120)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            HStack(spacing: 8) {
                Button {
                    model.hue = nil
                } label: {
                    Circle().fill(Color(nsColor: model.websiteColor))
                        .overlay(Circle().stroke(.white, lineWidth: model.hue == nil ? 2 : 0))
                        .frame(width: 16, height: 16)
                }
                .buttonStyle(.plain)
                .help("Use Website Color")
                HueSlider(hue: Binding(get: { model.hue ?? -1 }, set: { model.hue = $0 }))
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

/// A rainbow track whose knob picks a hue.
private struct HueSlider: View {
    @Binding var hue: Double

    var body: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(
                        LinearGradient(
                            colors: stride(from: 0.0, through: 1.0, by: 0.1).map {
                                Color(hue: $0, saturation: 0.55, brightness: 0.85)
                            }, startPoint: .leading, endPoint: .trailing)
                    )
                    .frame(height: 6)
                if hue >= 0 {
                    Circle().fill(.white).frame(width: 14, height: 14)
                        .shadow(color: .black.opacity(0.3), radius: 2)
                        .offset(x: hue * (width - 14))
                }
            }
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0).onChanged { value in
                    hue = min(max(Double(value.location.x / width), 0), 1)
                })
        }
        .frame(height: 16)
    }
}

// MARK: Notes

@MainActor
private final class NoteModel: ObservableObject {
    @Published var text = ""
    @Published var saving = false
    var save: () -> Void = {}
}

/// A short note, like "Copied Image", at the page's top right, like Arc's: white on dark purple,
/// with a button saving the framed capture.
private struct NoteView: View {
    @ObservedObject var model: NoteModel

    var body: some View {
        HStack {
            Spacer()
            HStack(spacing: 10) {
                Text(model.text)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.white)
                if model.saving {
                    Button("Save", action: model.save)
                        .buttonStyle(.plain)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 8)
                        .frame(height: 24)
                        .background(RoundedRectangle(cornerRadius: 6).fill(.white.opacity(0.18)))
                }
            }
            .padding(.horizontal, 16)
            .frame(height: 42)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(red: 0x22 / 255, green: 0x0F / 255, blue: 0x41 / 255)))
        }
        .padding(.trailing, 6)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

@_cdecl("arbalete_ui_captured")
public func captured(
    _ id: UInt32, _ base64: UnsafePointer<CChar>, _ area: Bool, _ x: Double, _ y: Double,
    _ width: Double, _ height: Double
) {
    guard let png = Data(base64Encoded: String(cString: base64)) else { return }
    MainActor.assumeIsolated {
        nativeWindows[id]?.capture.show(
            png, area: area, frame: NSRect(x: x, y: y, width: width, height: height))
    }
}
