// Draws the website's touch icon (180 × 180) and social image (1200 × 630) from the logo's
// geometry (assets/icon.svg), into website/src/assets.
//
//   swift website/scripts/images.swift website/src/assets

import AppKit

func color(_ hex: UInt32) -> NSColor {
    NSColor(
        srgbRed: CGFloat(hex >> 16 & 0xFF) / 255, green: CGFloat(hex >> 8 & 0xFF) / 255,
        blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}

/// The mark in its 100-unit square, outlined in white, at a point and a size.
func mark(at origin: NSPoint, size: CGFloat) {
    let scale = size / 100
    let ends = [NSPoint(x: 50, y: 19), NSPoint(x: 76.8, y: 65.5), NSPoint(x: 23.2, y: 65.5)]
    let point = { (p: NSPoint) in NSPoint(x: origin.x + p.x * scale, y: origin.y + (p.y + 6) * scale) }
    for (width, colors) in [(CGFloat(29), [UInt32(0xFFFFFF), 0xFFFFFF, 0xFFFFFF]), (17, [0xFF5A5A, 0xA35BFF, 0x3D7BFF])] {
        for (end, hex) in zip(ends, colors) {
            let vane = NSBezierPath()
            vane.move(to: point(NSPoint(x: 50, y: 50)))
            vane.line(to: point(end))
            vane.lineWidth = width * scale
            vane.lineCapStyle = .round
            color(hex).setStroke()
            vane.stroke()
        }
    }
    color(0xFFFFFF).setFill()
    let center = point(NSPoint(x: 50, y: 50))
    NSBezierPath(ovalIn: NSRect(x: center.x - 5.5 * scale, y: center.y - 5.5 * scale, width: 11 * scale, height: 11 * scale)).fill()
}

func image(_ name: String, width: Int, height: Int, folder: URL, draw: () -> Void) {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    // Top-left, like the SVG.
    context.cgContext.translateBy(x: 0, y: CGFloat(height))
    context.cgContext.scaleBy(x: 1, y: -1)
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context.cgContext, flipped: true)
    draw()
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: folder.appending(path: name))
}

let folder = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? ".")

// iOS rounds the corners itself: a full square, the icon's ground.
image("apple-touch-icon.png", width: 180, height: 180, folder: folder) {
    color(0x14111F).setFill()
    NSRect(x: 0, y: 0, width: 180, height: 180).fill()
    mark(at: NSPoint(x: 27, y: 22), size: 126)
}

image("og-image.png", width: 1200, height: 630, folder: folder) {
    color(0x0B0A1C).setFill()
    NSRect(x: 0, y: 0, width: 1200, height: 630).fill()
    for (index, hex) in [UInt32(0xFF5A5A), 0xA35BFF, 0x3D7BFF].enumerated() {
        color(hex).setFill()
        NSRect(x: CGFloat(index) * 400, y: 0, width: 400, height: 10).fill()
    }
    mark(at: NSPoint(x: 96, y: 150), size: 200)
    let title: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: 84, weight: .bold), .foregroundColor: color(0xECEAFF), .kern: -2,
    ]
    NSAttributedString(string: "Arbalete", attributes: title).draw(at: NSPoint(x: 96, y: 372))
    let line: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: 40, weight: .regular), .foregroundColor: color(0xA9A8C9),
    ]
    NSAttributedString(string: "A calm, open-source browser for macOS.", attributes: line)
        .draw(at: NSPoint(x: 96, y: 480))
}
