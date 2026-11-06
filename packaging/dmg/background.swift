// Draws the disk image window's background, "Trajectory": the app leaves like a bolt along a
// dotted arc onto Applications. The Finder draws the icons and their
// names over it, where packaging/dmg/settings.py puts them.
//
//   swift packaging/dmg/background.swift packaging/dmg
//
// writes background.png and background@2x.png, joined into background.tiff by
// `tiffutil -cathidpicheck`.

import AppKit

let size = NSSize(width: 660, height: 400)

func color(_ hex: UInt32) -> NSColor {
    NSColor(
        srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255,
        blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}

/// Strokes a path with round caps and joins.
func stroke(_ path: NSBezierPath, _ hex: UInt32, width: CGFloat, dash: [CGFloat] = []) {
    color(hex).setStroke()
    path.lineWidth = width
    path.lineCapStyle = .round
    path.lineJoinStyle = .round
    if !dash.isEmpty { path.setLineDash(dash, count: dash.count, phase: 0) }
    path.stroke()
}

func text(_ string: String, size: CGFloat, weight: NSFont.Weight, hex: UInt32, kern: CGFloat, y: CGFloat) {
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: color(hex),
        .kern: kern,
    ]
    let line = NSAttributedString(string: string, attributes: attributes)
    let width = line.size().width
    line.draw(at: NSPoint(x: (660 - width) / 2, y: y))
}

func draw() {
    color(0xF6F5FA).setFill()
    NSRect(origin: .zero, size: size).fill()

    // The arc, from over the app's icon to over Applications'.
    let arc = NSBezierPath()
    // A few dots back towards the app's icon, along the arc's start.
    arc.move(to: NSPoint(x: 186, y: 162))
    arc.line(to: NSPoint(x: 200, y: 150))
    arc.curve(
        to: NSPoint(x: 460, y: 150), controlPoint1: NSPoint(x: 286.7, y: 76.7),
        controlPoint2: NSPoint(x: 373.3, y: 76.7))
    stroke(arc, 0xC9C4D8, width: 2, dash: [0.01, 8])

    // Its head, landing on Applications.
    let head = NSAffineTransform()
    head.translateX(by: 460, yBy: 150)
    head.rotate(byDegrees: 40)
    let arrow = NSBezierPath()
    arrow.move(to: NSPoint(x: -14, y: -9))
    arrow.line(to: .zero)
    arrow.line(to: NSPoint(x: -14, y: 9))
    stroke(head.transform(arrow), 0x1D1B22, width: 3)

    text("Take aim.", size: 15, weight: .semibold, hex: 0x1D1B22, kern: 0.3, y: 38)
    text("Drag Arbalete onto Applications", size: 12, weight: .regular, hex: 0x5E596E, kern: 0, y: 352)
}

let folder = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? ".")
for scale in [1, 2] {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: 660 * scale, pixelsHigh: 400 * scale, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    // Points, not pixels: the 2x image says it is for Retina screens.
    rep.size = size
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = context
    // Top-left, like the design.
    context.cgContext.translateBy(x: 0, y: size.height)
    context.cgContext.scaleBy(x: 1, y: -1)
    let flipped = NSGraphicsContext(cgContext: context.cgContext, flipped: true)
    NSGraphicsContext.current = flipped
    draw()
    NSGraphicsContext.restoreGraphicsState()
    let name = scale == 1 ? "background.png" : "background@2x.png"
    try! rep.representation(using: .png, properties: [:])!.write(to: folder.appending(path: name))
}
