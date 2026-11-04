// Page captures, like Arc's: a veil over the page, where a drag picks the part to capture (capture
// in app/src/shell.rs), and a short note once a capture or a link is copied.

import AppKit
import SwiftUI

@MainActor
final class CaptureChrome {
    private let parent: NSWindow
    private let veil = Veil()
    private let veilPanel: Overlay
    private let note: Overlay
    private let noteModel = NoteModel()
    private var hiding: DispatchWorkItem?

    init(in parent: NSWindow, model: AppModel) {
        self.parent = parent
        veilPanel = Overlay(view: veil)
        let host = NSHostingView(rootView: NoteView(model: noteModel))
        host.sizingOptions = []
        note = Overlay(view: host)
        veil.picked = { [weak self] rect in
            self?.veilPanel.place(in: parent, nil)
            guard let rect else { return }
            model.run(
                "page/capture/area/\(Int(rect.minX))/\(Int(rect.minY))/\(Int(rect.width))/\(Int(rect.height))"
            )
        }
    }

    /// Shows the veil over the page at `frame`, in the window's points from its top left corner,
    /// or takes it away.
    func place(_ frame: NSRect?) {
        veil.reset()
        veilPanel.place(in: parent, frame)
        if frame != nil { veilPanel.makeKey() }
    }

    /// Shows `text` a moment, its top right corner at `corner`, in the window's points from its
    /// top left corner.
    func tell(_ text: String, at corner: NSPoint) {
        noteModel.text = text
        let size = NSSize(width: 260, height: 36)
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
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.6, execute: hide)
    }

    func close() {
        veilPanel.close()
        note.close()
    }
}

/// Dims the page; a drag draws the part to capture, which Escape or a mere click gives up.
private final class Veil: NSView {
    var picked: (NSRect?) -> Void = { _ in }
    private var start: NSPoint?
    private var rect: NSRect?

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    func reset() {
        start = nil
        rect = nil
        needsDisplay = true
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .crosshair)
    }

    override func mouseDown(with event: NSEvent) {
        start = convert(event.locationInWindow, from: nil)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start else { return }
        let point = convert(event.locationInWindow, from: nil)
        rect = NSRect(
            x: min(start.x, point.x), y: min(start.y, point.y), width: abs(point.x - start.x),
            height: abs(point.y - start.y))
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        let picked = rect.flatMap { $0.width > 4 && $0.height > 4 ? $0 : nil }
        reset()
        self.picked(picked)
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {
            reset()
            picked(nil)
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        let dim = NSBezierPath(rect: bounds)
        if let rect { dim.append(NSBezierPath(rect: rect).reversed) }
        NSColor.black.withAlphaComponent(0.35).setFill()
        dim.fill()
        if let rect {
            NSColor.white.setStroke()
            NSBezierPath(rect: rect.insetBy(dx: 0.5, dy: 0.5)).stroke()
            return
        }
        // Until a drag, how to capture, at the bottom.
        let text = "Drag over the page to capture part of it" as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 13, weight: .medium), .foregroundColor: NSColor.black,
        ]
        let size = text.size(withAttributes: attributes)
        let pill = NSRect(
            x: bounds.midX - size.width / 2 - 14, y: bounds.maxY - size.height - 44,
            width: size.width + 28, height: size.height + 14)
        NSColor.white.setFill()
        NSBezierPath(roundedRect: pill, xRadius: pill.height / 2, yRadius: pill.height / 2).fill()
        text.draw(at: NSPoint(x: pill.minX + 14, y: pill.minY + 7), withAttributes: attributes)
    }
}

@MainActor
private final class NoteModel: ObservableObject {
    @Published var text = ""
}

/// A short note, like "Copied Screenshot", in a capsule at the panel's right.
private struct NoteView: View {
    @ObservedObject var model: NoteModel

    var body: some View {
        HStack {
            Spacer()
            Text(model.text)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.white)
                .padding(.horizontal, 14)
                .frame(height: 30)
                .background(Capsule().fill(Color.black.opacity(0.75)))
        }
        .padding(.trailing, 8)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

@_cdecl("arbalete_ui_capture")
public func placeCapture(
    _ id: UInt32, _ shown: Bool, _ x: Double, _ y: Double, _ width: Double, _ height: Double
) {
    MainActor.assumeIsolated {
        nativeWindows[id]?.capture.place(
            shown ? NSRect(x: x, y: y, width: width, height: height) : nil)
    }
}

@_cdecl("arbalete_ui_note")
public func showNote(_ id: UInt32, _ text: UnsafePointer<CChar>, _ x: Double, _ y: Double) {
    let text = String(cString: text)
    MainActor.assumeIsolated {
        nativeWindows[id]?.capture.tell(text, at: NSPoint(x: x, y: y))
    }
}

/// Puts a PNG picture, in base64, on the clipboard: only the part at `x`, `y`, `width` and
/// `height` of it, in CSS pixels of a page `viewport` wide, unless `viewport` is 0.
@_cdecl("arbalete_ui_copy_png")
public func copyPNG(
    _ base64: UnsafePointer<CChar>, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ viewport: Double
) {
    guard let data = Data(base64Encoded: String(cString: base64)),
        var image = NSBitmapImageRep(data: data)?.cgImage
    else { return }
    if viewport > 0 {
        // The picture's pixels per CSS pixel.
        let scale = Double(image.width) / viewport
        let part = CGRect(x: x * scale, y: y * scale, width: width * scale, height: height * scale)
            .integral
        guard let cut = image.cropping(to: part) else { return }
        image = cut
    }
    guard let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
    else { return }
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.setData(png, forType: .png)
}
