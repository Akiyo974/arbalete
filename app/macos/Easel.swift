// An Easel's board, like Arc's (decision 0009): in the page's place on the card, the board scrolls
// up and down, scaled to its width, and the capture editor's tools draw on it (MarkCanvas), in
// Arc's colors, faces and shapes. It keeps the Easel it shows and sends it whole to the app after
// each change, which saves it (crates/arbalete-core/src/easel.rs); a picture added goes in first,
// by its own command. A live capture shows its page, which the app loads in a view of the board.

import AppKit
import CoreText
import SwiftUI
import UniformTypeIdentifiers

// MARK: The Easel as the app sends and saves it

/// An Easel (Easel in crates/arbalete-core/src/easel.rs).
struct EaselBoard: Codable {
    var objects: [EaselObject]
}

struct EaselObject: Codable, Equatable {
    var id: Int
    var frame: EaselRect
    var content: EaselContent
}

struct EaselRect: Codable, Equatable {
    var x: Double
    var y: Double
    var width: Double
    var height: Double

    init(_ rect: CGRect) {
        (x, y, width, height) = (rect.minX, rect.minY, rect.width, rect.height)
    }

    var rect: CGRect { CGRect(x: x, y: y, width: width, height: height) }
}

/// What an object shows, by its `type`.
struct EaselContent: Codable, Equatable {
    var type: String
    var image: String?
    var url: String?
    var title: String?
    var favicon: String?
    var page: EaselPage?
    var live: Bool?
    var text: String?
    var font: String?
    var style: String?
    var shape: String?
    var color: String?
    var tail: EaselPoint?
    var head: EaselPoint?
    var points: [EaselPoint]?
}

/// Where a capture was on its page, in the page's CSS pixels (PageView in easel.rs).
struct EaselPage: Codable, Equatable {
    var width: Double
    var height: Double
    var scrollX: Double
    var scrollY: Double
    var part: EaselRect
}

/// A point within an object's frame, as fractions of its width and height.
struct EaselPoint: Codable, Equatable {
    var x: Double
    var y: Double
}

// MARK: Arc's look

/// Arc's colors for Easels, measured from its app.
enum EaselInk {
    static let text = dynamic(light: .black, dark: .white)
    static let selection = srgb(0x6CBAD9)
    static let floating = dynamic(light: .white, dark: .black)
    static let toolbarIcon = dynamic(light: srgb(0x5F5880), dark: srgb(0x85878E))
    static let cardBackground = dynamic(light: .white, dark: .black)
    static let cardEdge = dynamic(light: NSColor(white: 0, alpha: 0.1), dark: srgb(0x686A6B))
    static let barBackground = dynamic(light: .white, dark: srgb(0x252832))
    static let chip = dynamic(
        light: NSColor(white: 9 / 255, alpha: 0.11), dark: NSColor(white: 0.965, alpha: 0.11))
    static let whiteEdge = dynamic(
        light: NSColor(white: 0, alpha: 0.1), dark: NSColor(white: 1, alpha: 0.1))

    /// Arc's "chill" palette, by name, black and white swapping in dark mode.
    static let palette: [(name: String, color: NSColor)] = [
        ("black", dynamic(light: .black, dark: .white)),
        ("white", dynamic(light: .white, dark: .black)),
        ("grey", srgb(0xBBBBBB)),
        ("pink", srgb(0xF2C2AC)),
        ("red", srgb(0xD74807)),
        ("yellow", srgb(0xC2A12A)),
        ("lightGreen", srgb(0xD0D87F)),
        ("darkGreen", srgb(0x1D5914)),
        ("lightBlue", srgb(0x55A2BD)),
        ("darkBlue", srgb(0x3139FB)),
        ("purple", srgb(0xA6729D)),
    ]

    static func color(_ name: String?) -> NSColor {
        palette.first { $0.name == name }?.color ?? palette[0].color
    }

    static func name(_ color: NSColor) -> String {
        palette.first { $0.color === color }?.name ?? "black"
    }

    static let look = ChromeLook(
        bar: Color(nsColor: floating), icon: Color(nsColor: toolbarIcon), selectedIcon: .white,
        selected: Color(nsColor: selection), hovered: Color(nsColor: toolbarIcon).opacity(0.12),
        palette: [Array(palette[0..<6].map(\.color)), Array(palette[6...].map(\.color))],
        face: Color(nsColor: toolbarIcon), step: Color(nsColor: toolbarIcon))

    private static func srgb(_ value: UInt32) -> NSColor {
        NSColor(
            srgbRed: CGFloat((value >> 16) & 0xFF) / 255, green: CGFloat((value >> 8) & 0xFF) / 255,
            blue: CGFloat(value & 0xFF) / 255, alpha: 1)
    }

    private static func dynamic(light: NSColor, dark: NSColor) -> NSColor {
        NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
        }
    }
}

/// Arc's faces, by a text's face (0 to 3): the system's, Inter, Space Mono and EB Garamond; and
/// its sizes, by a text's size (0 to 4): body, then titles from the smallest to "ultra". Each face
/// has its sizes, weights and line height, like Arc's.
private let easelFonts = ["clean", "hip", "mono", "fancy"]
private let easelStyles = ["body", "h2", "h1", "h0", "ultra"]

private struct EaselFace {
    var family: String?
    var lineHeight: CGFloat
    /// By style, from body to ultra: its size and weight.
    var styles: [(CGFloat, NSFont.Weight)]
}

private let easelFaces: [EaselFace] = [
    EaselFace(
        family: nil, lineHeight: 1.2,
        styles: [(15, .regular), (24, .bold), (44, .heavy), (60, .heavy), (80, .heavy)]),
    EaselFace(
        family: "Inter", lineHeight: 1.28,
        styles: [(18, .regular), (21, .bold), (44, .black), (60, .black), (80, .black)]),
    EaselFace(
        family: "Space Mono", lineHeight: 1.48,
        styles: [(15, .regular), (24, .bold), (44, .bold), (60, .bold), (80, .bold)]),
    EaselFace(
        family: "EB Garamond", lineHeight: 1.32,
        styles: [(16, .regular), (28, .regular), (48, .regular), (60, .regular), (80, .regular)]),
]

/// The fonts that ship with Arbalete for Easels, registered once.
@MainActor private let registeredFonts: Void = {
    guard let fonts = Bundle.main.resourceURL?.appendingPathComponent("fonts"),
        let files = try? FileManager.default.contentsOfDirectory(at: fonts, includingPropertiesForKeys: nil)
    else { return }
    for file in files where file.pathExtension == "ttf" {
        CTFontManagerRegisterFontsForURL(file as CFURL, .process, nil)
    }
}()

/// A text's font on the board, `size` times its size in the Easel's points.
@MainActor private func easelFont(_ mark: Mark, scale: CGFloat) -> NSFont {
    _ = registeredFonts
    let face = easelFaces[mark.face % easelFaces.count]
    let (size, weight) = face.styles[min(mark.size, face.styles.count - 1)]
    let points = size * scale
    guard let family = face.family else { return .systemFont(ofSize: points, weight: weight) }
    let descriptor = NSFontDescriptor(fontAttributes: [
        .family: family, .traits: [NSFontDescriptor.TraitKey.weight: weight],
    ])
    return NSFont(descriptor: descriptor, size: points) ?? .systemFont(ofSize: points, weight: weight)
}

/// A text's lines, in its font, its color and its alignment: a body text left, a title centered.
@MainActor private func easelText(_ mark: Mark, font: NSFont) -> NSAttributedString {
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = mark.size == 0 ? .left : .center
    paragraph.lineHeightMultiple = easelFaces[mark.face % easelFaces.count].lineHeight
    return NSAttributedString(
        string: mark.text,
        attributes: [.font: font, .foregroundColor: EaselInk.text, .paragraphStyle: paragraph])
}

/// A text's padding each side, in the Easel's points.
private let textPadding: CGFloat = 5

// MARK: Objects and marks

/// An object as the board draws it, with the pictures of the Easel's folder `images`; none for
/// an object it does not know.
private func boardMark(_ object: EaselObject, images: URL) -> Mark? {
    let frame = object.frame.rect
    let content = object.content
    let inFrame = { (point: EaselPoint) in
        CGPoint(x: frame.minX + point.x * frame.width, y: frame.minY + point.y * frame.height)
    }
    var mark: Mark
    switch content.type {
    case "capture", "image":
        guard let file = content.image,
            let source = CGImageSourceCreateWithURL(images.appendingPathComponent(file) as CFURL, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return nil }
        mark = Mark(kind: .image, rect: frame, color: .clear, image: image)
        mark.file = file
        mark.url = content.url
        mark.title = content.title
        mark.favicon = content.favicon
        mark.page = content.page
        mark.live = content.live ?? false
    case "text":
        mark = Mark(kind: .text, rect: frame, color: EaselInk.text)
        mark.text = content.text ?? ""
        mark.size = easelStyles.firstIndex(of: content.style ?? "") ?? 0
        mark.face = easelFonts.firstIndex(of: content.font ?? "") ?? 0
        mark.plain = true
    case "shape":
        mark = Mark(
            kind: content.shape == "ellipse" ? .ellipse : .rectangle, rect: frame,
            color: EaselInk.color(content.color))
    case "arrow":
        guard let tail = content.tail, let head = content.head else { return nil }
        mark = Mark(
            kind: .arrow, rect: .zero, points: [inFrame(tail), inFrame(head)],
            color: EaselInk.color(content.color))
    case "pen":
        mark = Mark(
            kind: .pen, rect: .zero, points: (content.points ?? []).map(inFrame),
            color: EaselInk.color(content.color))
    default:
        return nil
    }
    mark.id = object.id
    return mark
}

/// A mark as the Easel saves it; none for a picture not saved in its folder.
private func boardObject(_ mark: Mark) -> EaselObject? {
    let frame = mark.bounds
    let inFrame = { (point: CGPoint) in
        EaselPoint(
            x: frame.width > 0 ? (point.x - frame.minX) / frame.width : 0,
            y: frame.height > 0 ? (point.y - frame.minY) / frame.height : 0)
    }
    let color = EaselInk.name(mark.color)
    let content: EaselContent
    switch mark.kind {
    case .image:
        guard let file = mark.file else { return nil }
        content =
            mark.url == nil
            ? EaselContent(type: "image", image: file)
            : EaselContent(
                type: "capture", image: file, url: mark.url, title: mark.title,
                favicon: mark.favicon, page: mark.page, live: mark.live)
    case .text:
        content = EaselContent(
            type: "text", text: mark.text, font: easelFonts[mark.face % easelFonts.count],
            style: easelStyles[min(mark.size, easelStyles.count - 1)])
    case .ellipse, .rectangle:
        content = EaselContent(
            type: "shape", shape: mark.kind == .ellipse ? "ellipse" : "rectangle", color: color)
    case .arrow:
        content = EaselContent(
            type: "arrow", color: color, tail: inFrame(mark.tail), head: inFrame(mark.head))
    case .pen:
        content = EaselContent(type: "pen", color: color, points: mark.points.map(inFrame))
    }
    return EaselObject(id: mark.id, frame: EaselRect(frame), content: content)
}

// MARK: The board

/// The frame a capture `size` points wide and tall takes on a board whose marks take `frames`,
/// like Arc's: the first slot of a grid that no mark touches, row after row, three a row, 272 by
/// 194.3 points, 42 from the sides, 50 apart, from 160 down; filled as much as it can, centered
/// (easel::capture_frame).
func captureFrame(_ frames: [CGRect], size: CGSize) -> CGRect {
    let slot = (0...).lazy.map { n in
        CGRect(x: 42 + CGFloat(n % 3) * 322, y: 160 + CGFloat(n / 3) * 244.3, width: 272, height: 194.3)
    }.first { slot in !frames.contains { $0.intersects(slot) } }!
    let width = max(size.width, 1)
    let height = max(size.height, 1)
    let scale = min(slot.width / width, slot.height / height)
    return CGRect(
        x: slot.midX - width * scale / 2, y: slot.midY - height * scale / 2, width: width * scale,
        height: height * scale)
}

/// What a box dragged on an Easel may snap to, on one axis: its low edge, middle or high edge
/// to `value`; the mark whose it is, if not the board's; and whether it puts the box beside it.
struct SnapTarget {
    enum Edge { case low, middle, high }
    var edge: Edge
    var value: CGFloat
    var other: CGRect?
    var beside = false
}

/// What the app sends of an Easel to show: its number, the folder of its pictures, the board's
/// width in its points, the board, and the object to show first, if any.
struct EaselShown: Decodable {
    var number: Int
    var images: String
    var width: Double
    var easel: EaselBoard
    var focus: Int?
}

/// An Easel's board in a scroll view, which shows in the page's container.
@MainActor
final class EaselView: NSScrollView {
    let canvas: BoardCanvas

    init(_ shown: EaselShown, run: @escaping (String, [Any]) -> Void) {
        canvas = BoardCanvas(shown, run: run)
        super.init(frame: .zero)
        // Like Arc's, the board is the window's color darkened, under light marks and tools, whatever
        // the theme.
        appearance = NSAppearance(named: .darkAqua)
        hasVerticalScroller = true
        autohidesScrollers = true
        drawsBackground = false
        documentView = canvas
        contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(
            self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification,
            object: contentView)
    }

    required init?(coder: NSCoder) { nil }

    @objc private func scrolled() { canvas.refresh() }

    override func tile() {
        super.tile()
        canvas.fit(width: contentSize.width, height: contentSize.height)
        canvas.reveal()
    }
}

/// The board: as tall as its marks reach and a little more, its marks in the Easel's points,
/// scaled to its width, drawn like Arc's.
@MainActor
final class BoardCanvas: MarkCanvas {
    private let number: Int
    private let images: URL
    private let width: CGFloat
    private let run: (String, [Any]) -> Void
    /// The marks as last saved, and the earlier ones, for Undo, and the ones undone, for Redo.
    private var saved: [Mark]
    private var undone: [[Mark]] = []
    private var history: [[Mark]] = []
    private var saving: DispatchWorkItem?
    private var visible = CGSize.zero
    /// The bar over a capture hovered: its page's icon and title, and its buttons.
    private let bar = NSHostingView(rootView: CaptureBar(icon: nil, title: "", live: nil) { _ in })
    /// The mark copied, while the clipboard holds what it put there.
    private var copied: (mark: Mark, change: Int)?
    /// The live captures' views, by their marks' ids.
    private var lives: [Int: LiveCaptureView] = [:]
    private var icons: [String: NSImage] = [:]
    /// The mark to scroll to once the board is laid out, by its id.
    private var focus: Int?

    init(_ shown: EaselShown, run: @escaping (String, [Any]) -> Void) {
        number = shown.number
        images = URL(fileURLWithPath: shown.images)
        width = shown.width
        self.run = run
        saved = []
        let model = EditorModel()
        // Like Arc's: a new rectangle is light green, an ellipse red, an arrow light blue, a line
        // black.
        model.colors = [
            .rectangle: EaselInk.color("lightGreen"), .ellipse: EaselInk.color("red"),
            .arrow: EaselInk.color("lightBlue"), .pen: EaselInk.color("black"),
        ]
        model.sizes = easelStyles.count
        super.init(model: model, look: EaselInk.look)
        model.fit = { [weak self] mark in self?.fitText(&mark) }
        model.tool = .select
        model.marks = shown.easel.objects.compactMap { boardMark($0, images: images) }
        saved = model.marks
        focus = shown.focus
        bar.isHidden = true
        bar.safeAreaRegions = []
        addSubview(bar)
        registerForDraggedTypes([.fileURL, .png, .tiff])
    }

    required init?(coder: NSCoder) { nil }

    override var scale: CGFloat { bounds.width / width }
    override var plainText: Bool { true }

    override var toolsFrame: CGRect {
        let shown = visibleRect
        return CGRect(x: shown.minX + 12, y: shown.maxY - 12 - 44, width: 272, height: 44)
    }

    override func drawBackground(_ context: CGContext) {
        // The window's color at 46%, measured on Arc's.
        NSColor.black.withAlphaComponent(0.54).setFill()
        bounds.fill()
    }

    /// Takes the scroll view's width, and the height its marks reach, some room below them, or
    /// the scroll view's.
    func fit(width: CGFloat, height: CGFloat) {
        visible = CGSize(width: width, height: height)
        let scale = width / self.width
        let bottom = model.marks.map(\.bounds.maxY).max() ?? 0
        let size = CGSize(width: width, height: max(height, (bottom + 400) * scale))
        if frame.size != size { setFrameSize(size) }
        refresh()
    }

    override func refresh() {
        super.refresh()
        placeBar()
        placeLives()
        // Once the marks change, they save soon, as one change: a drag once it ends.
        guard saving == nil, NSEvent.pressedMouseButtons == 0, !same(model.marks, saved) else {
            return
        }
        let work = DispatchWorkItem { [weak self] in self?.save() }
        saving = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4, execute: work)
    }

    /// Saves the marks now, if they changed.
    func save() {
        saving?.cancel()
        saving = nil
        guard !same(model.marks, saved) else { return }
        history.append(saved)
        undone = []
        send()
    }

    private func send() {
        saved = model.marks
        let board = EaselBoard(objects: model.marks.compactMap(boardObject))
        run("save-easel", [number, jsonObject(board)])
        if visible != .zero { fit(width: visible.width, height: visible.height) }
    }

    private func same(_ a: [Mark], _ b: [Mark]) -> Bool {
        a.compactMap(boardObject) == b.compactMap(boardObject)
    }

    /// A mark added takes an id no other has.
    override func add(_ mark: Mark) {
        var mark = mark
        mark.id = (model.marks.map(\.id).max() ?? 0) + 1
        super.add(mark)
    }

    override func mouseUp(with event: NSEvent) {
        let count = model.marks.count
        super.mouseUp(with: event)
        // A mark drawn takes an id no other has.
        if model.marks.count > count, let last = model.marks.indices.last {
            model.marks[last].id = (model.marks.dropLast().map(\.id).max() ?? 0) + 1
        }
        refresh()
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        if newWindow == nil {
            commitText()
            save()
            closeLives()
        }
        super.viewWillMove(toWindow: newWindow)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }

    // MARK: Drawing, like Arc's

    override func drawMark(_ mark: Mark, in context: CGContext) {
        let s = scale
        let r = CGRect(x: mark.rect.minX * s, y: mark.rect.minY * s, width: mark.rect.width * s, height: mark.rect.height * s)
            .standardized
        let line = 2.5 * s
        context.saveGState()
        defer { context.restoreGState() }
        context.setLineCap(.round)
        context.setLineJoin(.round)
        switch mark.kind {
        case .ellipse, .rectangle:
            let path = mark.kind == .ellipse ? CGPath(ellipseIn: r, transform: nil) : CGPath(rect: r, transform: nil)
            context.addPath(path)
            context.setFillColor(mark.color.withAlphaComponent(0.75).cgColor)
            context.fillPath()
            context.addPath(path)
            let white = EaselInk.name(mark.color) == "white"
            context.setStrokeColor((white ? EaselInk.whiteEdge : mark.color).cgColor)
            context.setLineWidth(line)
            context.strokePath()
        case .arrow:
            let tail = CGPoint(x: mark.tail.x * s, y: mark.tail.y * s)
            let head = CGPoint(x: mark.head.x * s, y: mark.head.y * s)
            context.addPath(arrowPath(from: tail, to: head, scale: s))
            context.setStrokeColor(mark.color.cgColor)
            context.setLineWidth(line)
            context.strokePath()
        case .pen:
            context.addPath(smoothPath(mark.points.map { CGPoint(x: $0.x * s, y: $0.y * s) }))
            context.setStrokeColor(mark.color.cgColor)
            context.setLineWidth(line)
            context.strokePath()
        case .text:
            drawText(mark, in: r.insetBy(dx: textPadding * s, dy: 0))
        case .image:
            if mark.url == nil {
                drawPicture(mark.image, in: r, context: context, fill: true)
            } else {
                drawCard(mark, in: r, context: context)
            }
        }
    }

    /// Arc's arrow: a line, and two flares at its head, 45° off it, 32 points long at most.
    private func arrowPath(from tail: CGPoint, to head: CGPoint, scale: CGFloat) -> CGPath {
        let path = CGMutablePath()
        path.move(to: tail)
        path.addLine(to: head)
        let length = hypot(tail.x - head.x, tail.y - head.y)
        guard length > 0 else { return path }
        let flare = min(32 * scale, 0.3 * length) / length
        let back = CGPoint(x: (tail.x - head.x) * flare, y: (tail.y - head.y) * flare)
        for angle in [CGFloat.pi / 4, -CGFloat.pi / 4] {
            let arm = CGPoint(
                x: back.x * cos(angle) - back.y * sin(angle), y: back.x * sin(angle) + back.y * cos(angle))
            path.move(to: CGPoint(x: head.x + arm.x, y: head.y + arm.y))
            path.addLine(to: head)
        }
        return path
    }

    /// A text, shrunk until it fits its box, like Arc's.
    private func drawText(_ mark: Mark, in box: CGRect) {
        var size = scale
        var text = easelText(mark, font: easelFont(mark, scale: size))
        while size > scale * 0.2,
            text.boundingRect(with: CGSize(width: box.width, height: .greatestFiniteMagnitude),
                options: [.usesLineFragmentOrigin]).height > box.height + 1
        {
            size *= 0.92
            text = easelText(mark, font: easelFont(mark, scale: size))
        }
        NSGraphicsContext.saveGraphicsState()
        text.draw(with: box, options: [.usesLineFragmentOrigin])
        NSGraphicsContext.restoreGraphicsState()
    }

    /// A picture in `box`, filling it, or whole within it.
    private func drawPicture(_ image: CGImage?, in box: CGRect, context: CGContext, fill: Bool) {
        guard let image, image.width > 0, image.height > 0 else { return }
        let ratio = CGFloat(image.width) / CGFloat(image.height)
        var size = CGSize(width: box.width, height: box.width / ratio)
        if (size.height < box.height) == fill { size = CGSize(width: box.height * ratio, height: box.height) }
        let drawn = CGRect(
            x: box.midX - size.width / 2, y: box.midY - size.height / 2, width: size.width,
            height: size.height)
        context.saveGState()
        context.clip(to: box)
        context.translateBy(x: drawn.minX, y: drawn.maxY)
        context.scaleBy(x: 1, y: -1)
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(origin: .zero, size: drawn.size))
        context.restoreGState()
    }

    /// A capture, like Arc's: a card with 10-point corners and a hairline edge, its picture whole
    /// in it, or its live page over it.
    private func drawCard(_ mark: Mark, in box: CGRect, context: CGContext) {
        let card = CGPath(roundedRect: box, cornerWidth: 10 * scale, cornerHeight: 10 * scale, transform: nil)
        context.saveGState()
        context.addPath(card)
        context.clip()
        EaselInk.cardBackground.setFill()
        box.fill()
        if !mark.live { drawPicture(mark.image, in: box, context: context, fill: false) }
        context.restoreGState()
        context.addPath(card)
        context.setStrokeColor(EaselInk.cardEdge.cgColor)
        context.setLineWidth(1)
        context.strokePath()
    }

    // MARK: Texts

    override func font(of mark: Mark) -> NSFont { easelFont(mark, scale: scale) }

    override func alignment(of mark: Mark) -> NSTextAlignment { mark.size == 0 ? .left : .center }

    /// A text's box keeps its width and grows to hold its text.
    private func fitText(_ mark: inout Mark) {
        let text = easelText(mark, font: easelFont(mark, scale: 1))
        let width = max(mark.rect.width - 2 * textPadding, 10)
        let height = text.boundingRect(
            with: CGSize(width: width, height: .greatestFiniteMagnitude), options: [.usesLineFragmentOrigin]
        ).height
        mark.rect.size.height = max(mark.rect.height, ceil(height) + 4)
    }

    // MARK: Snapping, like Arc's (its EaselCanvasViewController+SnapGuides, measured in its code)

    /// What a snap draws, in the board's points: near-white guides, and marks in Arc's blue.
    private var guideLines: [(CGPoint, CGPoint)] = []
    private var guideMarks: [(CGPoint, CGPoint)] = []
    /// Where the box dragged was as each axis snapped, which makes the snap stickier the further
    /// the drag goes.
    private var snappedAt: (x: CGFloat?, y: CGFloat?) = (nil, nil)

    /// Snaps a box dragged, unless ⌘ is held: across, to the board's margins and middle; to the
    /// same edges and middles of the marks within 75 points of it; and 24 screen points beside or
    /// under one it lines up with. Within 6 screen points, 12 for a middle across, and up to 3.5
    /// times that once snapped.
    override func snap(_ box: CGRect, moving index: Int, event: NSEvent) -> CGSize {
        guideLines = []
        guideMarks = []
        guard !event.modifierFlags.contains(.command), scale > 0 else {
            snappedAt = (nil, nil)
            return .zero
        }
        let near = model.marks.indices.filter { $0 != index }.map { model.marks[$0].bounds }
            .filter { $0.insetBy(dx: -75, dy: -75).intersects(box) }
        let gap = 24 / scale
        var across: [SnapTarget] = [
            SnapTarget(edge: .low, value: 42), SnapTarget(edge: .middle, value: width / 2),
            SnapTarget(edge: .high, value: width - 42),
        ]
        var down: [SnapTarget] = []
        for other in near {
            across += [
                SnapTarget(edge: .low, value: other.minX, other: other),
                SnapTarget(edge: .middle, value: other.midX, other: other),
                SnapTarget(edge: .high, value: other.maxX, other: other),
            ]
            down += [
                SnapTarget(edge: .low, value: other.minY, other: other),
                SnapTarget(edge: .middle, value: other.midY, other: other),
                SnapTarget(edge: .high, value: other.maxY, other: other),
            ]
            if other.minX < box.maxX, box.minX < other.maxX {
                down += [
                    SnapTarget(edge: .low, value: other.maxY + gap, other: other, beside: true),
                    SnapTarget(edge: .high, value: other.minY - gap, other: other, beside: true),
                ]
            }
            if other.minY < box.maxY, box.minY < other.maxY {
                across += [
                    SnapTarget(edge: .low, value: other.maxX + gap, other: other, beside: true),
                    SnapTarget(edge: .high, value: other.minX - gap, other: other, beside: true),
                ]
            }
        }
        let x = closest(across, low: box.minX, high: box.maxX, middleWider: true, from: &snappedAt.x)
        let y = closest(down, low: box.minY, high: box.maxY, middleWider: false, from: &snappedAt.y)
        let snapped = box.offsetBy(dx: x?.offset ?? 0, dy: y?.offset ?? 0)
        if let x { guide(x.target, snapped, across: true) }
        if let y { guide(y.target, snapped, across: false) }
        return CGSize(width: x?.offset ?? 0, height: y?.offset ?? 0)
    }

    /// The target nearest to the box's edges on one axis, within reach, and how far it is.
    private func closest(
        _ targets: [SnapTarget], low: CGFloat, high: CGFloat, middleWider: Bool,
        from start: inout CGFloat?
    ) -> (target: SnapTarget, offset: CGFloat)? {
        let dragged = start.map { abs(low - $0) * scale } ?? 0
        let sticky = 1 + min(dragged / 300, 2.5)
        let found = targets.compactMap { target -> (target: SnapTarget, offset: CGFloat)? in
            let edge = target.edge == .low ? low : target.edge == .high ? high : (low + high) / 2
            let reach = (target.edge == .middle && middleWider ? 12 : 6) / scale * sticky
            let offset = target.value - edge
            return abs(offset) <= reach ? (target, offset) : nil
        }.min { abs($0.offset) < abs($1.offset) }
        if found == nil {
            start = nil
        } else if start == nil {
            start = low
        }
        return found
    }

    /// The guide of a snap on one axis, across or down, for the box snapped: like Arc's, a line
    /// through both, 15 points past them, or the board's height for its own; for middles, cut
    /// 9.1 points short of the box, with Arc's blue I-beams in the room; for an edge, the box's
    /// edge in blue.
    private func guide(_ target: SnapTarget, _ box: CGRect, across: Bool) {
        // Drawn as if across, then turned for down.
        let flip = { (p: CGPoint) in across ? p : CGPoint(x: p.y, y: p.x) }
        let b = across ? box : CGRect(x: box.minY, y: box.minX, width: box.height, height: box.width)
        let at = target.value
        guard let other = target.other.map({
            across ? $0 : CGRect(x: $0.minY, y: $0.minX, width: $0.height, height: $0.width)
        }) else {
            guideLines.append((flip(CGPoint(x: at, y: 0)), flip(CGPoint(x: at, y: bounds.height / scale))))
            return
        }
        if target.beside {
            // Across the room between them, at the middle of what they share.
            let middle = (max(b.minY, other.minY) + min(b.maxY, other.maxY)) / 2
            let from = target.edge == .low ? other.maxX : b.maxX
            let to = target.edge == .low ? b.minX : other.minX
            guideLines.append((flip(CGPoint(x: from, y: middle)), flip(CGPoint(x: to, y: middle))))
            return
        }
        let top = min(b.minY, other.minY) - 15
        let bottom = max(b.maxY, other.maxY) + 15
        guard target.edge == .middle else {
            guideLines.append((flip(CGPoint(x: at, y: top)), flip(CGPoint(x: at, y: bottom))))
            guideMarks.append((flip(CGPoint(x: at, y: b.minY)), flip(CGPoint(x: at, y: b.maxY))))
            return
        }
        let room: CGFloat = 9.1
        if top < b.minY - room {
            guideLines.append((flip(CGPoint(x: at, y: top)), flip(CGPoint(x: at, y: b.minY - room))))
        }
        if bottom > b.maxY + room {
            guideLines.append((flip(CGPoint(x: at, y: b.maxY + room)), flip(CGPoint(x: at, y: bottom))))
        }
        let stem = 7 / scale
        let cap = 10.5 / scale / 2
        for middle in [b.minY - room / 2, b.maxY + room / 2] {
            let (from, to) = (middle - stem / 2, middle + stem / 2)
            guideMarks += [
                (flip(CGPoint(x: at, y: from)), flip(CGPoint(x: at, y: to))),
                (flip(CGPoint(x: at - cap, y: from)), flip(CGPoint(x: at + cap, y: from))),
                (flip(CGPoint(x: at - cap, y: to)), flip(CGPoint(x: at + cap, y: to))),
            ]
        }
    }

    override func snapEnded() {
        guideLines = []
        guideMarks = []
        snappedAt = (nil, nil)
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        context.setLineWidth(2.5)
        context.setLineCap(.round)
        for (lines, color) in [(guideLines, NSColor(white: 0.92, alpha: 1)), (guideMarks, EaselInk.selection)] {
            guard !lines.isEmpty else { continue }
            context.setStrokeColor(color.cgColor)
            for (from, to) in lines {
                context.move(to: toView(from))
                context.addLine(to: toView(to))
            }
            context.strokePath()
        }
    }

    // MARK: Undo, copy and paste

    @objc func undo(_ sender: Any?) {
        save()
        guard let previous = history.popLast() else { return }
        undone.append(model.marks)
        model.selected = nil
        model.marks = previous
        send()
    }

    @objc func redo(_ sender: Any?) {
        guard let next = undone.popLast() else { return }
        history.append(model.marks)
        model.selected = nil
        model.marks = next
        send()
    }

    @objc func copy(_ sender: Any?) {
        guard let mark = model.selectedMark else { return }
        let board = NSPasteboard.general
        board.clearContents()
        if let image = mark.image {
            board.writeObjects([NSImage(cgImage: image, size: .zero)])
        } else if !mark.text.isEmpty {
            board.setString(mark.text, forType: .string)
        }
        copied = (mark, board.changeCount)
    }

    @objc func paste(_ sender: Any?) {
        let board = NSPasteboard.general
        if let copied, copied.change == board.changeCount {
            var mark = copied.mark
            mark.move(by: CGSize(width: 20, height: 20))
            self.copied = (mark, copied.change)
            return add(mark)
        }
        for picture in pictures(from: board) { insert(picture: picture, from: nil) }
    }

    @objc func delete(_ sender: Any?) { deleteSelected() }

    // MARK: Pictures

    /// A picture added goes into the Easel's folder first, under a name of its own: 400 points
    /// wide at most, at the top of what shows.
    override func insert(picture: CGImage, from file: URL?) {
        insert(picture: picture, at: nil)
    }

    /// Moves a picture into the Easel's folder: its file's name, if it went.
    private func keep(_ picture: CGImage) -> String? {
        guard let png = pngData(picture) else { return nil }
        let name = UUID().uuidString + ".png"
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(name)
        guard (try? png.write(to: file)) != nil else { return nil }
        run("add-easel-image", [number, file.path, name])
        return name
    }

    private func insert(picture: CGImage, at point: CGPoint?) {
        guard let name = keep(picture) else { return }
        let width = min(400, CGFloat(picture.width))
        let height = width * CGFloat(picture.height) / CGFloat(max(picture.width, 1))
        let shown = toMarks(CGPoint(x: visibleRect.midX, y: visibleRect.minY + 40))
        let origin = point ?? CGPoint(x: shown.x - width / 2, y: shown.y)
        var mark = Mark(
            kind: .image, rect: CGRect(x: origin.x, y: origin.y, width: width, height: height),
            color: .clear, image: picture)
        mark.file = name
        add(mark)
    }

    /// A capture of the page at `url`, `size` points, in the board's first free slot, like the app
    /// adds one to an Easel not shown (easel::add_capture), and scrolled to.
    func addCapture(
        _ image: CGImage, size: CGSize, url: String, title: String, icon: NSImage?, page: EaselPage?
    ) {
        guard let name = keep(image) else { return }
        var mark = Mark(
            kind: .image, rect: captureFrame(model.marks.map(\.bounds), size: size), color: .clear,
            image: image)
        mark.file = name
        mark.url = url
        mark.title = title
        mark.page = page
        mark.favicon = icon?.cgImage(forProposedRect: nil, context: nil, hints: nil).flatMap(keep)
        add(mark)
        focus = model.marks.last?.id
        enclosingScrollView?.needsLayout = true
    }

    /// Scrolls to the mark to show first, once the board has its size.
    func reveal() {
        guard let focus, bounds.width > 0, let mark = model.marks.first(where: { $0.id == focus })
        else { return }
        self.focus = nil
        scrollToVisible(toView(mark.bounds).insetBy(dx: 0, dy: -40))
    }

    private func pictures(from board: NSPasteboard) -> [CGImage] {
        let options: [NSPasteboard.ReadingOptionKey: Any] = [
            .urlReadingContentsConformToTypes: [UTType.image.identifier]
        ]
        if let urls = board.readObjects(forClasses: [NSURL.self], options: options) as? [URL],
            !urls.isEmpty
        {
            return urls.compactMap { url in
                CGImageSourceCreateWithURL(url as CFURL, nil)
                    .flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }
            }
        }
        let images = board.readObjects(forClasses: [NSImage.self]) as? [NSImage] ?? []
        return images.compactMap { $0.cgImage(forProposedRect: nil, context: nil, hints: nil) }
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        pictures(from: sender.draggingPasteboard).isEmpty ? [] : .copy
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let point = toMarks(convert(sender.draggingLocation, from: nil))
        let pictures = pictures(from: sender.draggingPasteboard)
        for (index, picture) in pictures.enumerated() {
            let step = CGFloat(index) * 20
            insert(picture: picture, at: CGPoint(x: point.x + step, y: point.y + step))
        }
        return !pictures.isEmpty
    }

    // MARK: Captures

    override func hoverChanged() { placeBar() }

    private func icon(_ file: String?) -> NSImage? {
        guard let file else { return nil }
        if let icon = icons[file] { return icon }
        let icon = NSImage(contentsOf: images.appendingPathComponent(file))
        icons[file] = icon
        return icon
    }

    /// Over a capture hovered, Arc's bar along its bottom: its page's icon and title, a button
    /// opening the page, and one turning it live or back, once it knows where it was on the page.
    private func placeBar() {
        guard let hovered, model.marks.indices.contains(hovered), let url = model.marks[hovered].url
        else { return bar.isHidden = true }
        let mark = model.marks[hovered]
        let box = toView(mark.rect).standardized
        let index = hovered
        bar.rootView = CaptureBar(
            icon: icon(mark.favicon), title: mark.title ?? url, live: mark.page == nil ? nil : mark.live
        ) { [weak self] action in
            switch action {
            case .open: self?.run("open", [url])
            case .live: self?.toggleLive(index)
            }
        }
        bar.frame = CGRect(x: box.minX + 6, y: box.maxY - 6 - 36, width: max(box.width - 12, 60), height: 36)
        bar.isHidden = false
        addSubview(bar, positioned: .above, relativeTo: nil)
    }

    /// Turns a capture live, or back to its picture; the first time, like Arc's, after asking.
    private func toggleLive(_ index: Int) {
        guard model.marks.indices.contains(index) else { return }
        let asked = "easelLiveCaptureAsked"
        if !model.marks[index].live, !UserDefaults.standard.bool(forKey: asked) {
            let alert = NSAlert()
            alert.messageText = "Show the live page instead of this capture?"
            alert.informativeText =
                "The page loads each time the Easel shows, cut to the part you captured, so it stays up to date."
            alert.addButton(withTitle: "Show Live")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return }
            UserDefaults.standard.set(true, forKey: asked)
        }
        model.marks[index].live.toggle()
    }

    // MARK: Live captures

    /// Gives each live capture its page's view where its card is, and takes away the others':
    /// none while the board is out of sight.
    private func placeLives() {
        let shown = window != nil && !isHiddenOrHasHiddenAncestor
        let marks = shown ? model.marks.filter { $0.live && $0.page != nil && $0.url != nil } : []
        for (id, view) in lives where !marks.contains(where: { $0.id == id }) {
            view.close()
            lives[id] = nil
        }
        for mark in marks {
            guard let page = mark.page, let url = mark.url else { continue }
            let view = lives[mark.id] ?? {
                let view = LiveCaptureView(easel: number, id: mark.id, page: page)
                lives[mark.id] = view
                addSubview(view, positioned: .below, relativeTo: tools)
                // Not while the app shows the board: its page is made once that is done.
                let (number, id, run) = (number, mark.id, run)
                DispatchQueue.main.async {
                    guard let view = liveCaptures["\(number)/\(id)"] else { return }
                    view.scaled = view.scale
                    run("show-live-capture", [number, id, url, jsonObject(page), Double(view.scale)])
                }
                return view
            }()
            view.frame = toView(mark.rect).standardized
            view.layer?.cornerRadius = 10 * scale
            if view.scaled != nil, view.scaled != view.scale {
                view.scaled = view.scale
                run("scale-live-capture", [number, mark.id, Double(view.scale)])
            }
        }
        if !bar.isHidden { addSubview(bar, positioned: .above, relativeTo: nil) }
    }

    private func closeLives() {
        for view in lives.values { view.close() }
        lives = [:]
    }

    override func viewDidHide() {
        super.viewDidHide()
        closeLives()
    }

    override func viewDidUnhide() {
        super.viewDidUnhide()
        placeLives()
    }
}

/// What a capture's bar does.
enum CaptureAction { case open, live }

/// A capture's bar, like Arc's: its page's icon and title, and its buttons, 36 points tall on a
/// floating white card.
struct CaptureBar: View {
    let icon: NSImage?
    let title: String
    /// Whether it shows live, if it can.
    let live: Bool?
    let act: (CaptureAction) -> Void

    var body: some View {
        HStack(spacing: 6) {
            Group {
                if let icon {
                    Image(nsImage: icon).resizable().interpolation(.high)
                } else {
                    Image(systemName: "globe").resizable().foregroundStyle(.secondary)
                }
            }
            .frame(width: 16, height: 16)
            Text(title).font(.system(size: 14)).lineLimit(1).truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let live {
                chip(live ? "dot.radiowaves.left.and.right" : "dot.radiowaves.right", live: live,
                    help: live ? "Show the Capture" : "Show Live") { act(.live) }
            }
            chip("arrow.up.right", live: false, help: "Open the Page") { act(.open) }
        }
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: EaselInk.barBackground))
                .shadow(color: .black.opacity(0.17), radius: 0.75, y: 1))
    }

    private func chip(_ symbol: String, live: Bool, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 11, weight: .semibold))
                .foregroundStyle(live ? Color.white : Color.primary)
                .frame(width: 23, height: 23)
                .background(
                    RoundedRectangle(cornerRadius: 5).fill(
                        live ? Color(nsColor: EaselInk.selection) : Color(nsColor: EaselInk.chip)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
    }
}

// MARK: Live captures

/// The views live captures' pages are drawn in, by their Easel's number and their own id.
@MainActor private var liveCaptures: [String: LiveCaptureView] = [:]

/// A live capture's page, cut to the part captured and scaled to its card: CEF draws the page in
/// `host`, which the app has Chromium draw `scale` times its size, as large as the page's view
/// was so scaled. It only shows: clicks go to the board under it.
@MainActor
final class LiveCaptureView: NSView {
    /// Fits CEF's view to it, like a tab's page.
    let host = PageHost()
    private let key: String
    private let page: EaselPage
    /// The scale the app last drew the page at, once it was asked to.
    var scaled: CGFloat?

    init(easel: Int, id: Int, page: EaselPage) {
        key = "\(easel)/\(id)"
        self.page = page
        super.init(frame: .zero)
        wantsLayer = true
        layer?.masksToBounds = true
        addSubview(host)
        liveCaptures[key] = self
    }

    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    /// The card's points per pixel of the page.
    var scale: CGFloat { page.part.width > 0 ? bounds.width / page.part.width : 1 }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        let s = scale
        host.frame = CGRect(
            x: -page.part.x * s, y: -page.part.y * s, width: page.width * s, height: page.height * s)
    }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// Asks the app to close its page, whose view goes once it closes (liveCaptureClosed).
    func close() {
        removeFromSuperview()
        let parts = key.split(separator: "/").compactMap { Int($0) }
        nativeWindows.values.first?.model.run("close-live-capture", parts[0], parts[1])
    }

    /// Whether CEF's view of its page is `view`.
    func holds(_ view: UnsafeMutableRawPointer) -> Bool {
        host.subviews.contains { Unmanaged.passUnretained($0).toOpaque() == view }
    }

    func forget() { liveCaptures[key] = nil }
}

/// The view a live capture's page is drawn in, `id` on the Easel `easel`: CEF makes the page's
/// browser in it.
@_cdecl("arbalete_ui_live_capture_view")
public func liveCaptureView(_ easel: Int64, _ id: Int64) -> UnsafeMutableRawPointer? {
    let address: UInt = MainActor.assumeIsolated {
        guard let view = liveCaptures["\(easel)/\(id)"] else { return 0 }
        return UInt(bitPattern: Unmanaged.passUnretained(view.host).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}

/// Takes away a live capture's page's view, `view`, its browser closing: the view goes, which
/// destroys the browser.
@_cdecl("arbalete_ui_live_capture_closed")
public func liveCaptureClosed(_ view: UnsafeMutableRawPointer?) {
    MainActor.assumeIsolated {
        guard let view, let live = liveCaptures.values.first(where: { $0.holds(view) }) else { return }
        live.host.subviews.forEach { $0.removeFromSuperview() }
        live.forget()
    }
}

// MARK: The app's side

/// Shows an Easel's board (JSON, EaselShown) in the view of the tab `tab`, made in the window
/// `id` the first time: a board already shown keeps its own. False while the window cannot hold
/// it yet.
@_cdecl("arbalete_ui_easel")
public func showEasel(_ id: UInt32, _ tab: Int64, _ json: UnsafePointer<CChar>) -> Bool {
    MainActor.assumeIsolated {
        let data = Data(String(cString: json).utf8)
        guard let shown = try? JSONDecoder().decode(EaselShown.self, from: data),
            let container = nativeWindows[id]?.pages.container(for: Int(tab))
        else { return false }
        if container.board != nil { return true }
        guard let window = nativeWindows[id] else { return false }
        let view = EaselView(shown) { name, args in
            let model = nativeWindows[id]?.model ?? nativeWindows.values.first?.model
            model?.run(command: command(name, args: args))
        }
        container.host.addSubview(view)
        return true
    }
}

/// Shows an Easel's board (JSON, EaselShown) in Peek's card in the window `id`, its bar titled
/// `title`: the tab `tab`'s board if it has one, borrowed until Peek closes (PeekChrome.pageClosed),
/// or one of its own. False while the card cannot hold it.
@_cdecl("arbalete_ui_peek_easel")
public func peekEasel(
    _ id: UInt32, _ tab: Int64, _ title: UnsafePointer<CChar>, _ json: UnsafePointer<CChar>
) -> Bool {
    MainActor.assumeIsolated {
        guard let window = nativeWindows[id] else { return false }
        let peek = window.peek
        let board: EaselView
        if let own = pageContainers[Int(tab)]?.board {
            board = own
            peek.lender = Int(tab)
        } else {
            let data = Data(String(cString: json).utf8)
            guard let shown = try? JSONDecoder().decode(EaselShown.self, from: data) else {
                return false
            }
            board = EaselView(shown) { name, args in
                let model = nativeWindows[id]?.model ?? nativeWindows.values.first?.model
                model?.run(command: command(name, args: args))
            }
        }
        // Over the Space's color, which shows through the board as in the window.
        let model = window.model
        peek.page.wantsLayer = true
        peek.page.layer?.backgroundColor =
            NSColor(color(model.space?.theme.colors.first ?? model.cardColor)).cgColor
        peek.page.addSubview(board)
        peek.state.url = String(cString: title)
        peek.state.color = ""
        peek.state.back = false
        peek.state.forward = false
        peek.state.loading = false
        board.window?.makeFirstResponder(board.canvas)
        return true
    }
}

/// Takes away the view of an Easel's tab, closed, once its board is saved.
@_cdecl("arbalete_ui_easel_closed")
public func easelClosed(_ tab: Int64) {
    MainActor.assumeIsolated {
        guard let container = pageContainers.removeValue(forKey: Int(tab)) else { return }
        container.board?.canvas.save()
        container.removeFromSuperview()
    }
}

extension PageContainer {
    /// The Easel's board it shows, if it shows one instead of a page.
    var board: EaselView? { host.subviews.first { $0 is EaselView } as? EaselView }
}
