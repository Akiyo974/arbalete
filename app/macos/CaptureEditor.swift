// The editor of a part of the page captured, like Arc's, measured on a recording: the part zooms
// from its place to fit the window, which dims, with a bar of tools at its bottom left: select,
// add a picture, text, an ellipse, a rectangle, an arrow and a pen. A shape, an arrow or a line
// selected shows a palette above it; a text, the "+ Aa –" control at its left, which sizes it and
// changes its face. A click out of the part, or Escape, zooms it back with its marks.

import AppKit
import Combine
import SwiftUI
import UniformTypeIdentifiers

/// Arc's editor's colors, measured.
private enum EditorInk {
    static let accent = NSColor(srgbRed: 0x62 / 255, green: 0xB1 / 255, blue: 0xD4 / 255, alpha: 1)
    static let bar = Color(red: 0x04 / 255, green: 0x06 / 255, blue: 0x21 / 255)
    static let selected = Color(red: 0x35 / 255, green: 0x3B / 255, blue: 0xD5 / 255)
    static let hovered = Color(red: 0x29 / 255, green: 0x2C / 255, blue: 0x86 / 255)
    static let icon = Color(red: 0xE9 / 255, green: 0xEA / 255, blue: 0xFE / 255)
    static let blackEdge = NSColor(white: 0x22 / 255, alpha: 1)
    static let dim: CGFloat = 0.64
    static let fill: CGFloat = 0.78
}

private func hex(_ value: UInt32) -> NSColor {
    NSColor(
        srgbRed: CGFloat((value >> 16) & 0xFF) / 255, green: CGFloat((value >> 8) & 0xFF) / 255,
        blue: CGFloat(value & 0xFF) / 255, alpha: 1)
}

/// The palette, like Arc's: six colors, then five offset by half a step.
private let paletteRows: [[NSColor]] = [
    [hex(0x000000), hex(0xFFCC2D), hex(0xFE8E44), hex(0xF32F13), hex(0x00E881), hex(0x106E00)],
    [hex(0xFFFFFF), hex(0xB2B2B2), hex(0xB80095), hex(0x00C4F7), hex(0x2A30FA)],
]

/// How the tools' bar, the palette and the text's control look: the editor's, dark, or an Easel's
/// (Easel.swift), with their palette.
struct ChromeLook {
    var bar: Color
    var icon: Color
    var selectedIcon: Color
    var selected: Color
    var hovered: Color
    var palette: [[NSColor]]
    /// The text control's "Aa" and its steps.
    var face: Color
    var step: Color

    static let editor = ChromeLook(
        bar: EditorInk.bar, icon: EditorInk.icon, selectedIcon: .white, selected: EditorInk.selected,
        hovered: EditorInk.hovered, palette: paletteRows, face: Color(white: 0xD2 / 255),
        step: Color(white: 0x55 / 255))
}

enum EditorTool: CaseIterable {
    case select, image, text, ellipse, rectangle, arrow, pen

    var symbol: String {
        switch self {
        case .select: "cursorarrow"
        case .image: "photo"
        case .text: "character"
        case .ellipse: "circle"
        case .rectangle: "square"
        case .arrow: "arrow.up.right"
        case .pen: "scribble"
        }
    }

    /// The key that picks it, shown on its button, like Arc's.
    var key: String? {
        switch self {
        case .select: nil
        case .image: "i"
        case .text: "t"
        case .ellipse: "o"
        case .rectangle: "r"
        case .arrow: "a"
        case .pen: "p"
        }
    }

    var label: String {
        switch self {
        case .select: "Select"
        case .image: "Image"
        case .text: "Text"
        case .ellipse: "Ellipse"
        case .rectangle: "Rectangle"
        case .arrow: "Arrow"
        case .pen: "Pen"
        }
    }
}

/// A text's sizes on the screen, which "+" and "–" step through, and its faces, which "Aa"
/// cycles through, like Arc's.
let textSizes: [CGFloat] = [14, 24, 46, 80]

private func textFont(_ face: Int, size: CGFloat) -> NSFont {
    switch face % 4 {
    case 1: NSFont(name: "Hoefler Text", size: size) ?? NSFont(name: "Georgia", size: size)
        ?? .systemFont(ofSize: size)
    case 2: .systemFont(ofSize: size, weight: .black, width: .expanded)
    case 3: .monospacedSystemFont(ofSize: size, weight: .bold)
    default: .systemFont(ofSize: size, weight: .bold)
    }
}

/// A mark on the picture, in its pixels from its top left corner.
struct Mark {
    enum Kind { case text, ellipse, rectangle, arrow, pen, image }
    var kind: Kind
    /// Its box: a shape's, a text's or a picture's.
    var rect: CGRect
    /// A line's points; an arrow's tail and head.
    var points: [CGPoint] = []
    var color: NSColor
    /// Its line's width, in the picture's pixels.
    var weight: CGFloat = 2
    var text = ""
    /// Its text's size, among `textSizes`, and face.
    var size = 1
    var face = 0
    /// The picture's pixels per point of the screen, as the editor shows it: a text's size is
    /// the screen's.
    var pixelsPerPoint: CGFloat = 1
    var image: CGImage?
    /// A text in its color, without the editor's pill, as an Easel's are.
    var plain = false
    /// A picture's file, on an Easel.
    var file: String?
    /// The page a capture comes from, on an Easel, its title and its icon's file; where the
    /// capture was on it, and whether it shows the page live.
    var url: String?
    var title: String?
    var favicon: String?
    var page: EaselPage?
    var live = false
    /// Unique on an Easel.
    var id = 0

    var tail: CGPoint { points.first ?? .zero }
    var head: CGPoint { points.last ?? .zero }

    /// The box it takes.
    var bounds: CGRect {
        guard kind == .pen || kind == .arrow else { return rect.standardized }
        guard let first = points.first else { return .zero }
        return points.reduce(CGRect(origin: first, size: .zero)) {
            $0.union(CGRect(origin: $1, size: .zero))
        }
    }

    var font: NSFont { textFont(face, size: textSizes[size] * pixelsPerPoint) }

    mutating func move(by delta: CGSize) {
        rect = rect.offsetBy(dx: delta.width, dy: delta.height)
        points = points.map { CGPoint(x: $0.x + delta.width, y: $0.y + delta.height) }
    }

    /// Fits a text's pill to its text, about its middle, like Arc's: 8 points each side, a line
    /// 1.2 times its size tall.
    mutating func fitText() {
        let width = ((text.isEmpty ? " " : text) as NSString).size(withAttributes: [.font: font]).width
        let height = textSizes[size] * pixelsPerPoint * 1.2
        let middle = CGPoint(x: rect.midX, y: rect.midY)
        let padded = width + 16 * pixelsPerPoint
        rect = CGRect(x: middle.x - padded / 2, y: middle.y - height / 2, width: padded, height: height)
    }

    /// Whether `point` is on it, within `slop`.
    func contains(_ point: CGPoint, slop: CGFloat) -> Bool {
        switch kind {
        case .arrow:
            return distance(point, tail, head) < slop + weight
        case .pen:
            return zip(points, points.dropFirst()).contains { distance(point, $0, $1) < slop + weight }
        default:
            return bounds.insetBy(dx: -slop, dy: -slop).contains(point)
        }
    }

    /// Draws it in `context`, flipped, its pixels `scale` points each.
    func draw(in context: CGContext, scale: CGFloat) {
        let r = CGRect(
            x: rect.minX * scale, y: rect.minY * scale, width: rect.width * scale,
            height: rect.height * scale)
        let line = weight * scale
        let black = color.whiteComponentIfGrey == 0
        context.saveGState()
        defer { context.restoreGState() }
        context.setLineCap(.round)
        context.setLineJoin(.round)
        switch kind {
        case .ellipse, .rectangle:
            // Translucent, with an edge of its color, like Arc's: a black one's edge is grey.
            let shape = r.standardized
            let path =
                kind == .ellipse
                ? CGPath(ellipseIn: shape, transform: nil) : CGPath(rect: shape, transform: nil)
            context.addPath(path)
            context.setFillColor(color.withAlphaComponent(EditorInk.fill).cgColor)
            context.fillPath()
            context.addPath(path)
            context.setStrokeColor((black ? EditorInk.blackEdge : color).cgColor)
            context.setLineWidth(line)
            context.strokePath()
        case .arrow:
            drawArrow(
                in: context, from: CGPoint(x: tail.x * scale, y: tail.y * scale),
                to: CGPoint(x: head.x * scale, y: head.y * scale), width: line, color: color.cgColor,
                arms: 33 * pixelsPerPoint * scale)
        case .pen:
            context.addPath(smoothPath(points.map { CGPoint(x: $0.x * scale, y: $0.y * scale) }))
            context.setStrokeColor(color.cgColor)
            context.setLineWidth(line)
            context.strokePath()
        case .text:
            if !plain {
                let radius = 8 * pixelsPerPoint * scale
                context.addPath(
                    CGPath(roundedRect: r, cornerWidth: radius, cornerHeight: radius, transform: nil))
                context.setFillColor(NSColor.black.cgColor)
                context.fillPath()
            }
            let font = textFont(face, size: textSizes[size] * pixelsPerPoint * scale)
            let string = NSAttributedString(
                string: text, attributes: [.font: font, .foregroundColor: plain ? color : NSColor.white])
            let typeset = CTLineCreateWithAttributedString(string)
            let width = CTLineGetTypographicBounds(typeset, nil, nil, nil)
            context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
            context.textPosition = CGPoint(
                x: r.midX - width / 2, y: r.midY + (font.ascender + font.descender) / 2)
            CTLineDraw(typeset, context)
        case .image:
            guard let image else { return }
            context.translateBy(x: r.minX, y: r.maxY)
            context.scaleBy(x: 1, y: -1)
            context.interpolationQuality = .high
            context.draw(image, in: CGRect(origin: .zero, size: r.size))
        }
    }
}

extension NSColor {
    /// Light enough for a black mark on it, as shown now.
    var isLight: Bool {
        guard let rgb = usingColorSpace(.sRGB) else { return false }
        return 0.299 * rgb.redComponent + 0.587 * rgb.greenComponent + 0.114 * rgb.blueComponent > 0.6
    }

    /// Its brightness, for a grey; nil for a color.
    var whiteComponentIfGrey: CGFloat? {
        guard let rgb = usingColorSpace(.sRGB) else { return nil }
        guard abs(rgb.redComponent - rgb.greenComponent) < 0.01,
            abs(rgb.greenComponent - rgb.blueComponent) < 0.01
        else { return nil }
        return rgb.redComponent
    }
}

/// How far `point` is from the segment from `a` to `b`.
private func distance(_ point: CGPoint, _ a: CGPoint, _ b: CGPoint) -> CGFloat {
    let dx = b.x - a.x
    let dy = b.y - a.y
    let length = dx * dx + dy * dy
    let t = length == 0 ? 0 : max(0, min(1, ((point.x - a.x) * dx + (point.y - a.y) * dy) / length))
    return hypot(point.x - (a.x + t * dx), point.y - (a.y + t * dy))
}

/// A line through `points`, smoothed: curves through their middles.
func smoothPath(_ points: [CGPoint]) -> CGPath {
    let path = CGMutablePath()
    guard let first = points.first else { return path }
    path.move(to: first)
    guard points.count > 2 else {
        points.dropFirst().forEach { path.addLine(to: $0) }
        return path
    }
    for (a, b) in zip(points.dropFirst(), points.dropFirst(2)) {
        path.addQuadCurve(to: CGPoint(x: (a.x + b.x) / 2, y: (a.y + b.y) / 2), control: a)
    }
    path.addLine(to: points[points.count - 1])
    return path
}

/// An arrow, like Arc's: its shaft and an open head of two arms at 45°.
private func drawArrow(
    in context: CGContext, from tail: CGPoint, to head: CGPoint, width: CGFloat, color: CGColor,
    arms: CGFloat
) {
    let length = hypot(head.x - tail.x, head.y - tail.y)
    let arm = min(arms, length * 0.35)
    let angle = atan2(head.y - tail.y, head.x - tail.x)
    context.setStrokeColor(color)
    context.setLineWidth(width)
    context.setLineCap(.round)
    context.setLineJoin(.round)
    context.move(to: tail)
    context.addLine(to: head)
    for side in [CGFloat.pi * 0.75, -CGFloat.pi * 0.75] {
        context.move(to: head)
        context.addLine(to: CGPoint(x: head.x + arm * cos(angle + side), y: head.y + arm * sin(angle + side)))
    }
    context.strokePath()
}

/// Draws `marks` on `image`.
func flatten(_ image: CGImage, _ marks: [Mark]) -> CGImage? {
    guard !marks.isEmpty else { return nil }
    let width = image.width
    let height = image.height
    guard
        let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { return nil }
    context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    // Flipped, as the marks count from the top left corner.
    context.translateBy(x: 0, y: CGFloat(height))
    context.scaleBy(x: 1, y: -1)
    for mark in marks { mark.draw(in: context, scale: 1) }
    return context.makeImage()
}

@MainActor
final class EditorModel: ObservableObject {
    /// Like Arc's, the editor opens on the text tool.
    @Published var tool = EditorTool.text
    @Published var selected: Int?
    @Published var marks: [Mark] = []
    /// The tools that took a color last, which the next mark takes, like Arc's first ones.
    var colors: [EditorTool: NSColor] = [
        .ellipse: hex(0xF32F13), .rectangle: hex(0x00E881), .arrow: hex(0x00C4F7),
        .pen: hex(0xF32F13),
    ]

    /// How many text sizes "+" and "–" step through, and how a text's box fits its text.
    var sizes = textSizes.count
    var fit: (inout Mark) -> Void = { $0.fitText() }

    var selectedMark: Mark? {
        guard let selected, marks.indices.contains(selected) else { return nil }
        return marks[selected]
    }

    func recolor(_ color: NSColor) {
        guard let selected, marks.indices.contains(selected) else { return }
        marks[selected].color = color
        switch marks[selected].kind {
        case .ellipse: colors[.ellipse] = color
        case .rectangle: colors[.rectangle] = color
        case .arrow: colors[.arrow] = color
        case .pen: colors[.pen] = color
        default: break
        }
    }

    func resizeText(by step: Int) {
        guard let selected, selectedMark?.kind == .text else { return }
        marks[selected].size = max(0, min(sizes - 1, marks[selected].size + step))
        fit(&marks[selected])
    }

    func cycleFace() {
        guard let selected, selectedMark?.kind == .text else { return }
        marks[selected].face = (marks[selected].face + 1) % 4
        fit(&marks[selected])
    }
}

/// The editor, over the whole window, which gives back the marks drawn on the part.
@MainActor
final class CaptureEditor {
    let overlay: Overlay
    private let canvas: EditorCanvas

    /// Edits `image`, shown at `from` in the window, with `marks` already on it; `done` gets the
    /// marks once the editor has zoomed back.
    init(image: CGImage, marks: [Mark], from: CGRect, done: @escaping ([Mark]) -> Void) {
        canvas = EditorCanvas(image: image, from: from)
        canvas.model.marks = marks
        canvas.done = done
        overlay = Overlay(view: canvas)
    }

    func open() {
        canvas.open()
    }
}

/// Where marks are drawn with the tools, with the tools' bar, the palette and the text's control
/// over it: the editor's picture, or an Easel's board (Easel.swift). The marks count in the
/// units of `area`, `scale` view points each.
@MainActor
class MarkCanvas: NSView, NSTextFieldDelegate {
    let model: EditorModel
    let tools: NSHostingView<EditorToolsView>
    private let palette: NSHostingView<PaletteView>
    private let textControl: NSHostingView<TextControlView>
    private var dragStart: CGPoint?
    private var lastPoint: CGPoint?
    /// A drawing under way: its kind and box, in the marks' units.
    private var drawing: Mark?
    /// The handle dragged: a box's corner, from its top left clockwise, or an arrow's tail (0)
    /// or head (1).
    private var handle: Int?
    /// The mark moved: its index, as it was, and where the drag began, in the marks' units.
    private var moving: (index: Int, mark: Mark, start: CGPoint)?
    private(set) var hovered: Int?
    private var field: NSTextField?
    private var editing: Int?
    private var observer: AnyCancellable?

    private let look: ChromeLook

    init(model: EditorModel? = nil, look: ChromeLook = .editor) {
        let model = model ?? EditorModel()
        self.model = model
        self.look = look
        tools = NSHostingView(rootView: EditorToolsView(model: model, look: look, addImage: {}))
        palette = NSHostingView(rootView: PaletteView(model: model, look: look))
        textControl = NSHostingView(rootView: TextControlView(model: model, look: look))
        super.init(frame: .zero)
        for view in [tools, palette, textControl] as [NSView] { addSubview(view) }
        tools.safeAreaRegions = []
        palette.safeAreaRegions = []
        textControl.safeAreaRegions = []
        tools.rootView = EditorToolsView(
            model: model, look: look, addImage: { [weak self] in self?.addImage() })
        observer = model.objectWillChange.sink { [weak self] _ in
            DispatchQueue.main.async { self?.refresh() }
        }
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: .zero,
                options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect, .cursorUpdate],
                owner: self))
    }

    // MARK: What the canvas makes of it

    /// Where the marks' origin and units are, in the view.
    var area: CGRect { bounds }
    var scale: CGFloat { 1 }
    /// The marks' units per point of the screen, for the marks drawn now.
    var pixelsPerPoint: CGFloat { 1 }
    /// The tools work: not while the editor zooms.
    var editable: Bool { true }
    /// Texts are white on a black pill, like the editor's; or plain, in their color.
    var plainText: Bool { false }

    /// Draws a mark, the area's origin at the context's.
    func drawMark(_ mark: Mark, in context: CGContext) {
        mark.draw(in: context, scale: scale)
    }

    /// A text's font, and its alignment, as shown now.
    func font(of mark: Mark) -> NSFont {
        textFont(mark.face, size: textSizes[mark.size] * mark.pixelsPerPoint * scale)
    }

    func alignment(of mark: Mark) -> NSTextAlignment { .center }

    /// A click out of `area`.
    func pressedOutside() {}
    func escape() { model.selected = nil }
    /// Draws what is under the marks.
    func drawBackground(_ context: CGContext) {}
    /// The tools' bar, 12 points from the area's bottom left corner, like Arc's.
    var toolsFrame: CGRect {
        CGRect(x: area.minX + 12, y: area.maxY - 12 - 44, width: 272, height: 44)
    }
    /// A picture added: by default, at the area's top left corner, about two thirds of its
    /// height, like Arc's editor.
    func insert(picture: CGImage, from file: URL?) {
        let height = area.height / scale * 0.65
        let width = height * CGFloat(picture.width) / CGFloat(picture.height)
        add(Mark(kind: .image, rect: CGRect(x: 0, y: 0, width: width, height: height), color: .clear,
            image: picture))
    }

    /// Adds a mark, selected.
    func add(_ mark: Mark) {
        model.marks.append(mark)
        model.selected = model.marks.count - 1
        model.tool = .select
        window?.makeFirstResponder(self)
    }

    func toMarks(_ point: CGPoint) -> CGPoint {
        let rect = area
        return CGPoint(x: (point.x - rect.minX) / scale, y: (point.y - rect.minY) / scale)
    }

    func toView(_ point: CGPoint) -> CGPoint {
        let rect = area
        return CGPoint(x: rect.minX + point.x * scale, y: rect.minY + point.y * scale)
    }

    func toView(_ rect: CGRect) -> CGRect {
        let area = area
        return CGRect(
            x: area.minX + rect.minX * scale, y: area.minY + rect.minY * scale,
            width: rect.width * scale, height: rect.height * scale)
    }

    // MARK: Drawing

    /// Places the tools' bar, the palette and the text's control, and draws again.
    func refresh() {
        tools.frame = toolsFrame
        tools.isHidden = !editable
        let mark = model.selectedMark
        if let mark, editable, [.ellipse, .rectangle, .arrow, .pen].contains(mark.kind) {
            let box = toView(mark.bounds)
            palette.frame = CGRect(x: box.midX - 91, y: box.minY - 13.5 - 77, width: 182, height: 77)
            palette.isHidden = false
        } else {
            palette.isHidden = true
        }
        if let mark, editable, mark.kind == .text {
            let box = toView(mark.rect)
            textControl.frame = CGRect(x: box.minX - 12 - 42, y: box.midY - 40, width: 42, height: 80)
            textControl.isHidden = false
        } else {
            textControl.isHidden = true
        }
        if let field, let editing, model.marks.indices.contains(editing) {
            let mark = model.marks[editing]
            field.frame = toView(mark.rect)
            field.font = font(of: mark)
        }
        needsDisplay = true
    }

    override func layout() {
        super.layout()
        refresh()
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        drawBackground(context)
        let area = area
        context.saveGState()
        context.clip(to: area)
        context.translateBy(x: area.minX, y: area.minY)
        for (index, mark) in model.marks.enumerated() where index != editing {
            drawMark(mark, in: context)
        }
        context.restoreGState()
        guard editable else { return }
        drawPreview(context)
        if let hovered, hovered != model.selected, model.marks.indices.contains(hovered) {
            outline(model.marks[hovered], width: 2)
        }
        if let mark = model.selectedMark { select(mark) }
    }

    /// A drawing under way, in Arc's blue, thin.
    private func drawPreview(_ context: CGContext) {
        guard let drawing else { return }
        let accent = EditorInk.accent.cgColor
        context.saveGState()
        context.setStrokeColor(accent)
        context.setLineWidth(1.5)
        switch drawing.kind {
        case .ellipse: context.strokeEllipse(in: toView(drawing.rect.standardized))
        case .rectangle: context.stroke(toView(drawing.rect.standardized))
        case .text:
            let box = toView(drawing.rect.standardized)
            context.addPath(CGPath(roundedRect: box, cornerWidth: 6, cornerHeight: 6, transform: nil))
            context.strokePath()
        case .arrow:
            drawArrow(in: context, from: toView(drawing.tail), to: toView(drawing.head), width: 2,
                color: accent, arms: 44)
        case .pen:
            context.setLineWidth(2)
            context.setLineCap(.round)
            context.setLineJoin(.round)
            context.addPath(smoothPath(drawing.points.map(toView)))
            context.strokePath()
        case .image: break
        }
        context.restoreGState()
    }

    /// Its outline in Arc's blue, following its shape.
    private func outline(_ mark: Mark, width: CGFloat) {
        EditorInk.accent.setStroke()
        let path: NSBezierPath
        switch mark.kind {
        case .ellipse: path = NSBezierPath(ovalIn: toView(mark.rect.standardized))
        case .text: path = NSBezierPath(roundedRect: toView(mark.rect).insetBy(dx: -2, dy: -2), xRadius: 8, yRadius: 8)
        case .arrow:
            path = NSBezierPath()
            path.move(to: toView(mark.tail))
            path.line(to: toView(mark.head))
        case .pen:
            path = NSBezierPath(cgPath: smoothPath(mark.points.map(toView)))
        default: path = NSBezierPath(rect: toView(mark.bounds))
        }
        path.lineWidth = width
        path.lineCapStyle = .round
        path.stroke()
    }

    /// Its outline and its handles, hollow rings at its corners, or an arrow's ends, like Arc's.
    private func select(_ mark: Mark) {
        switch mark.kind {
        case .image: outline(mark, width: 1)
        case .pen: break
        case .text: if plainText { outline(mark, width: 1) }
        default: outline(mark, width: mark.kind == .arrow ? 1 : 2)
        }
        for point in handles(of: mark) {
            let ring = NSBezierPath(ovalIn: CGRect(x: point.x - 4, y: point.y - 4, width: 8, height: 8))
            ring.lineWidth = 2
            EditorInk.accent.setStroke()
            ring.stroke()
        }
    }

    private func handles(of mark: Mark) -> [CGPoint] {
        switch mark.kind {
        case .arrow: return [toView(mark.tail), toView(mark.head)]
        case .pen: return []
        default:
            let box = toView(mark.bounds)
            return [
                CGPoint(x: box.minX, y: box.minY), CGPoint(x: box.maxX, y: box.minY),
                CGPoint(x: box.maxX, y: box.maxY), CGPoint(x: box.minX, y: box.maxY),
            ]
        }
    }

    func mark(at point: CGPoint) -> Int? {
        let slop = 6 / scale
        return model.marks.indices.last { model.marks[$0].contains(point, slop: slop) }
    }

    // MARK: Pointer

    override func cursorUpdate(with event: NSEvent) { updateCursor(event) }

    override func mouseMoved(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let over = model.tool == .select ? mark(at: toMarks(point)) : nil
        if over != hovered {
            hovered = over
            hoverChanged()
            needsDisplay = true
        }
        updateCursor(event)
    }

    override func mouseExited(with event: NSEvent) {
        guard hovered != nil else { return }
        hovered = nil
        hoverChanged()
        needsDisplay = true
    }

    /// The mark under the pointer changed.
    func hoverChanged() {}

    /// How far to move the mark at `index`, dragged to `box`, to snap it: none by default.
    func snap(_ box: CGRect, moving index: Int, event: NSEvent) -> CGSize { .zero }
    /// The drag that snapped ended.
    func snapEnded() {}

    private func updateCursor(_ event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        guard area.contains(point) else { return NSCursor.arrow.set() }
        switch model.tool {
        case .select, .image:
            if let mark = model.selectedMark,
                handles(of: mark).contains(where: { hypot($0.x - point.x, $0.y - point.y) < 7 })
            {
                NSCursor.crosshair.set()
            } else if hovered != nil {
                NSCursor.openHand.set()
            } else {
                NSCursor.arrow.set()
            }
        case .text: NSCursor.iBeam.set()
        default: NSCursor.crosshair.set()
        }
    }

    override func mouseDown(with event: NSEvent) {
        guard editable else { return }
        let point = convert(event.locationInWindow, from: nil)
        commitText()
        window?.makeFirstResponder(self)
        guard area.contains(point) else { return pressedOutside() }
        let at = toMarks(point)
        dragStart = at
        lastPoint = at
        handle = nil
        switch model.tool {
        case .select, .image:
            if let mark = model.selectedMark {
                handle = handles(of: mark).firstIndex { hypot($0.x - point.x, $0.y - point.y) < 7 }
            }
            if handle == nil {
                model.selected = mark(at: at)
                if let selected = model.selected {
                    if event.clickCount == 2, model.marks[selected].kind == .text {
                        editText(selected)
                    } else {
                        NSCursor.closedHand.set()
                        moving = (selected, model.marks[selected], at)
                    }
                }
            }
        case .text, .ellipse, .rectangle:
            let kind: Mark.Kind =
                switch model.tool {
                case .text: .text
                case .ellipse: .ellipse
                default: .rectangle
                }
            drawing = Mark(kind: kind, rect: CGRect(origin: at, size: .zero), color: .clear)
        case .pen:
            drawing = Mark(kind: .pen, rect: .zero, points: [at], color: .clear)
        case .arrow:
            drawing = Mark(kind: .arrow, rect: .zero, points: [at, at], color: .clear)
        }
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        let at = toMarks(convert(event.locationInWindow, from: nil))
        guard let start = dragStart, let last = lastPoint else { return }
        lastPoint = at
        if var drawing {
            switch drawing.kind {
            case .pen: drawing.points.append(at)
            case .arrow:
                drawing.points = [start, at]
            default:
                drawing.rect = CGRect(
                    x: min(start.x, at.x), y: min(start.y, at.y), width: abs(at.x - start.x),
                    height: abs(at.y - start.y))
            }
            self.drawing = drawing
        } else if let selected = model.selected, model.marks.indices.contains(selected) {
            if let handle {
                resize(selected, handle: handle, to: at)
            } else if let moving, moving.index == selected {
                // From where it was, so that snapping does not add up.
                var mark = moving.mark
                mark.move(by: CGSize(width: at.x - moving.start.x, height: at.y - moving.start.y))
                mark.move(by: snap(mark.bounds, moving: selected, event: event))
                model.marks[selected] = mark
            } else {
                model.marks[selected].move(by: CGSize(width: at.x - last.x, height: at.y - last.y))
            }
        }
        needsDisplay = true
    }

    /// Drags a handle of a mark: an arrow's end, or a box's corner, a picture keeping its shape.
    private func resize(_ index: Int, handle: Int, to at: CGPoint) {
        var mark = model.marks[index]
        if mark.kind == .arrow {
            mark.points = [handle == 0 ? at : mark.tail, handle == 1 ? at : mark.head]
        } else {
            let box = mark.rect.standardized
            let fixed = [
                CGPoint(x: box.maxX, y: box.maxY), CGPoint(x: box.minX, y: box.maxY),
                CGPoint(x: box.minX, y: box.minY), CGPoint(x: box.maxX, y: box.minY),
            ][handle]
            var corner = at
            if mark.kind == .image, box.height > 0 {
                let ratio = box.width / box.height
                let width = abs(at.x - fixed.x)
                corner.y = fixed.y + (at.y >= fixed.y ? 1 : -1) * width / ratio
            }
            mark.rect = CGRect(
                x: min(fixed.x, corner.x), y: min(fixed.y, corner.y), width: abs(corner.x - fixed.x),
                height: abs(corner.y - fixed.y))
            if mark.kind == .text { model.fit(&mark) }
        }
        model.marks[index] = mark
    }

    override func mouseUp(with event: NSEvent) {
        dragStart = nil
        handle = nil
        if moving != nil {
            moving = nil
            snapEnded()
        }
        guard var mark = drawing else { return needsDisplay = true }
        drawing = nil
        let tool = model.tool
        mark.pixelsPerPoint = pixelsPerPoint
        switch mark.kind {
        case .text:
            // A click places a box from it, a drag the box dragged, like Arc's.
            if mark.rect.width < 4 {
                mark.rect = CGRect(
                    x: mark.rect.minX, y: mark.rect.minY - 17 * pixelsPerPoint,
                    width: 270 * pixelsPerPoint, height: 34 * pixelsPerPoint)
            }
            mark.plain = plainText
            mark.color = plainText ? .black : .white
            model.marks.append(mark)
            model.tool = .select
            model.selected = model.marks.count - 1
            editText(model.marks.count - 1)
        case .pen:
            guard mark.points.count > 1 else { return needsDisplay = true }
            mark.color = model.colors[.pen] ?? hex(0xF32F13)
            mark.weight = 2.75 * pixelsPerPoint
            model.marks.append(mark)
        default:
            guard hypot(mark.bounds.width, mark.bounds.height) > 3 * pixelsPerPoint else {
                return needsDisplay = true
            }
            mark.color = model.colors[tool] ?? hex(0xF32F13)
            mark.weight = (mark.kind == .arrow ? 3 : 2) * pixelsPerPoint
            model.marks.append(mark)
            // Like Arc's, back to selecting once a shape is drawn, which is not selected.
            model.tool = .select
        }
        needsDisplay = true
    }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 53: escape()
        case 51, 117: deleteSelected()
        default:
            let key = event.charactersIgnoringModifiers?.lowercased()
            if event.modifierFlags.intersection([.command, .control, .option]).isEmpty,
                let tool = EditorTool.allCases.first(where: { $0.key != nil && $0.key == key })
            {
                if tool == .image { addImage() } else { model.tool = tool }
            } else {
                super.keyDown(with: event)
            }
        }
    }

    func deleteSelected() {
        guard let selected = model.selected, model.marks.indices.contains(selected) else { return }
        model.marks.remove(at: selected)
        model.selected = nil
    }

    // MARK: Text

    private func editText(_ index: Int) {
        let mark = model.marks[index]
        let field = NSTextField(frame: toView(mark.rect))
        field.stringValue = mark.text
        field.placeholderString = "Start typing to enter text"
        field.isBordered = false
        field.drawsBackground = true
        field.backgroundColor = EditorInk.accent.withAlphaComponent(0.08)
        field.wantsLayer = true
        field.layer?.cornerRadius = 9
        field.textColor = mark.plain ? mark.color : .white
        field.alignment = alignment(of: mark)
        field.cell?.wraps = mark.plain
        field.cell?.isScrollable = !mark.plain
        field.focusRingType = .none
        field.font = font(of: mark)
        field.target = self
        field.action = #selector(textEntered)
        field.delegate = self
        addSubview(field, positioned: .below, relativeTo: tools)
        window?.makeFirstResponder(field)
        field.currentEditor()?.selectAll(nil)
        self.field = field
        editing = index
        needsDisplay = true
    }

    @objc private func textEntered() {
        commitText()
        window?.makeFirstResponder(self)
    }

    /// Like Arc's, Escape ends the text and lets it go.
    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        guard selector == #selector(cancelOperation(_:)) else { return false }
        textEntered()
        escape()
        return true
    }

    func commitText() {
        guard let field, let index = editing else { return }
        self.field = nil
        editing = nil
        field.removeFromSuperview()
        guard model.marks.indices.contains(index) else { return }
        let text = field.stringValue.trimmingCharacters(in: .whitespaces)
        if text.isEmpty {
            model.marks.remove(at: index)
            model.selected = nil
        } else {
            model.marks[index].text = text
            model.fit(&model.marks[index])
        }
        needsDisplay = true
    }

    // MARK: Pictures

    /// Adds a picture the user picks.
    func addImage() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url,
            let source = CGImageSourceCreateWithURL(url as CFURL, nil),
            let picture = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return }
        insert(picture: picture, from: url)
    }
}

/// The dimmed window, the picture and its marks, where the tools draw: the part zooms from the
/// page into it, and back once a click out of it or Escape closes it.
@MainActor
final class EditorCanvas: MarkCanvas {
    var done: ([Mark]) -> Void = { _ in }
    private let image: CGImage
    /// Where the part shows on the page, from the window's top left corner, which it zooms from.
    private let from: CGRect
    /// How far the zoom is, from the page's part (0) to the editor's (1).
    private var progress: CGFloat = 0
    private var animation: Timer?
    private var closing = false

    init(image: CGImage, from: CGRect) {
        self.image = image
        self.from = from
        super.init()
    }

    required init?(coder: NSCoder) { nil }

    // MARK: Zoom

    func open() {
        window?.makeFirstResponder(self)
        animate(to: 1)
    }

    private func close() {
        guard !closing else { return }
        closing = true
        commitText()
        model.selected = nil
        animate(to: 0) { [weak self] in
            guard let self else { return }
            self.done(self.model.marks)
        }
    }

    // Like Arc's, a click out of the picture leaves the editor, as Escape does.
    override func pressedOutside() { close() }
    override func escape() { close() }

    /// Zooms to `target` with an ease out, like Arc's, in about 0.22 s.
    private func animate(to target: CGFloat, then: @escaping () -> Void = {}) {
        animation?.invalidate()
        let start = progress
        let began = CACurrentMediaTime()
        let duration = 0.22
        animation = Timer.scheduledTimer(withTimeInterval: 1 / 120, repeats: true) { [weak self] timer in
            MainActor.assumeIsolated {
                guard let self else { return timer.invalidate() }
                let t = min(1, (CACurrentMediaTime() - began) / duration)
                let eased = 1 - pow(1 - t, 3)
                self.progress = start + (target - start) * eased
                self.refresh()
                if t >= 1 {
                    timer.invalidate()
                    then()
                }
            }
        }
    }

    /// Where the picture fits the window, 20 points from its top and bottom, centered.
    private var fitted: CGRect {
        let room = bounds.insetBy(dx: 20, dy: 20)
        let width = CGFloat(image.width)
        let height = CGFloat(image.height)
        let scale = min(room.width / width, room.height / height)
        let size = CGSize(width: width * scale, height: height * scale)
        return CGRect(
            x: bounds.midX - size.width / 2, y: bounds.midY - size.height / 2, width: size.width,
            height: size.height)
    }

    /// Where the picture shows now, on its way between the page and the editor.
    override var area: CGRect {
        let end = fitted
        let p = progress
        return CGRect(
            x: from.minX + (end.minX - from.minX) * p, y: from.minY + (end.minY - from.minY) * p,
            width: from.width + (end.width - from.width) * p,
            height: from.height + (end.height - from.height) * p)
    }

    override var scale: CGFloat { area.width / CGFloat(image.width) }
    /// The picture's pixels per point, once zoomed.
    override var pixelsPerPoint: CGFloat { CGFloat(image.width) / fitted.width }
    override var editable: Bool { progress == 1 }

    override func refresh() {
        super.refresh()
        tools.isHidden = progress == 0
    }

    override func drawBackground(_ context: CGContext) {
        NSColor.black.withAlphaComponent(EditorInk.dim * progress).setFill()
        bounds.fill()
        let picture = area
        context.saveGState()
        context.translateBy(x: picture.minX, y: picture.maxY)
        context.scaleBy(x: 1, y: -1)
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(origin: .zero, size: picture.size))
        context.restoreGState()
    }
}

// MARK: The tools' bar, the palette and the text's control

/// The tools' bar, like Arc's: seven buttons 36 points square in a 272 × 44 bar.
struct EditorToolsView: View {
    @ObservedObject var model: EditorModel
    let look: ChromeLook
    let addImage: () -> Void

    var body: some View {
        HStack(spacing: 2) {
            ForEach(EditorTool.allCases, id: \.self) { tool in
                ToolButton(tool: tool, look: look, selected: model.tool == tool) {
                    if tool == .image { addImage() } else { model.tool = tool }
                }
            }
        }
        .padding(4)
        .background(RoundedRectangle(cornerRadius: 6, style: .continuous).fill(look.bar))
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private struct ToolButton: View {
    let tool: EditorTool
    let look: ChromeLook
    let selected: Bool
    let action: () -> Void
    private let _hovered = State(initialValue: false)

    var body: some View {
        let hovered = _hovered.wrappedValue
        Button(action: action) {
            Image(systemName: tool.symbol)
                .font(.system(size: 15))
                .foregroundStyle(selected ? look.selectedIcon : look.icon)
                .frame(width: 36, height: 36)
                .background(
                    RoundedRectangle(cornerRadius: 5).fill(
                        selected ? look.selected : hovered ? look.hovered : .clear))
                // Its key, at its bottom right, while it is hovered or selected, like Arc's.
                .overlay(alignment: .bottomTrailing) {
                    if let key = tool.key, hovered || selected {
                        Text(key).font(.system(size: 7.5, weight: .bold))
                            .foregroundStyle(selected ? look.selectedIcon : look.icon)
                            .padding(.trailing, 3).padding(.bottom, 2)
                    }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(tool.label)
        .onHover { over in withAnimation(.easeOut(duration: 0.15)) { _hovered.wrappedValue = over } }
    }
}

/// The palette above a shape, an arrow or a line selected, like Arc's: black, six colors then
/// five offset by half a step, the current one checked; it stays open as it recolors.
struct PaletteView: View {
    @ObservedObject var model: EditorModel
    let look: ChromeLook

    var body: some View {
        let current = model.selectedMark?.color
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 5.5) {
                ForEach(Array(look.palette.enumerated()), id: \.offset) { index, row in
                    HStack(spacing: 6) {
                        ForEach(Array(row.enumerated()), id: \.offset) { _, color in
                            Button {
                                model.recolor(color)
                            } label: {
                                Circle().fill(Color(nsColor: color))
                                    .overlay(
                                        Circle().stroke(Color(nsColor: EditorInk.blackEdge),
                                            lineWidth: color.whiteComponentIfGrey == 0 ? 1 : 0)
                                    )
                                    .overlay {
                                        if let current, current.isEqual(color) {
                                            Image(systemName: "checkmark")
                                                .font(.system(size: 9, weight: .bold))
                                                .foregroundStyle(
                                                    color.isLight ? Color.black : Color.white)
                                        }
                                    }
                                    .frame(width: 20, height: 20)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.leading, index == 1 ? 13 : 0)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .frame(width: 182, height: 71, alignment: .topLeading)
            .background(
                RoundedRectangle(cornerRadius: 10).fill(look.bar)
                    .shadow(color: .black.opacity(0.15), radius: 6, y: 1))
            Notch(down: true).fill(look.bar).frame(width: 12, height: 6)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

/// The control at a text's left, like Arc's: "+" and "–" size it, "Aa" changes its face.
struct TextControlView: View {
    @ObservedObject var model: EditorModel
    let look: ChromeLook

    var body: some View {
        HStack(spacing: 0) {
            VStack(spacing: 0) {
                step("plus", by: 1)
                Button(action: model.cycleFace) {
                    Text("Aa").font(.custom("Hoefler Text", size: 15))
                        .foregroundStyle(look.face)
                        .frame(width: 30, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Font")
                step("minus", by: -1)
            }
            .padding(.vertical, 4)
            .frame(width: 36, height: 80)
            .background(
                RoundedRectangle(cornerRadius: 8).fill(look.bar)
                    .shadow(color: .black.opacity(0.15), radius: 6, y: 1))
            Notch(down: false).fill(look.bar).frame(width: 6, height: 12)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
    }

    private func step(_ symbol: String, by step: Int) -> some View {
        Button { model.resizeText(by: step) } label: {
            Image(systemName: symbol).font(.system(size: 12, weight: .regular))
                .foregroundStyle(look.step)
                .frame(width: 30, height: 24)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(step > 0 ? "Larger" : "Smaller")
    }
}

/// A small triangle pointing down, or right.
private struct Notch: Shape {
    let down: Bool

    func path(in rect: CGRect) -> Path {
        var path = Path()
        if down {
            path.move(to: CGPoint(x: rect.minX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
        } else {
            path.move(to: CGPoint(x: rect.minX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
            path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY))
        }
        path.closeSubpath()
        return path
    }
}
