// The sidebar's colors and sizes, like Arc's: each color has a light and a dark ink, which the
// theme's appearance picks (SidebarHost sets it on the sidebar).

import AppKit
import SwiftUI

extension Color {
    /// A color whose light and dark inks are an RGB hex and an opacity each.
    static func ink(_ light: (UInt32, Double), _ dark: (UInt32, Double)) -> Color {
        Color(
            nsColor: NSColor(name: nil) { appearance in
                let (rgb, alpha) =
                    appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
                return NSColor(
                    srgbRed: CGFloat(rgb >> 16 & 0xff) / 255, green: CGFloat(rgb >> 8 & 0xff) / 255,
                    blue: CGFloat(rgb & 0xff) / 255, alpha: alpha)
            })
    }
}

enum Ink {
    /// A row or a pill at rest, like the address's, and a row hovered.
    static let item = Color.ink((0x0E0F10, 0.05), (0xFAFBFF, 0.10))
    static let itemHovered = Color.ink((0x0E0F10, 0.10), (0xFAFBFF, 0.15))
    /// The selected tab's row, and its shadow in light themes.
    static let selected = Color.ink((0xFFFFFF, 0.85), (0xFAFBFF, 0.20))
    static let selectedBehind = Color.ink((0xFFFFFF, 0.85), (0xFAFBFF, 0.10))
    static let selectedShadow = Color.ink((0x000000, 0.20), (0x000000, 0))
    /// A tab among several selected at once.
    static let marked = Color.ink((0x000000, 0.10), (0xFAFBFF, 0.10))
    /// A tab of a split, or of a row inside another.
    static let child = Color.ink((0x000000, 0.07), (0xFFFFFF, 0.07))
    /// Where a tab resting on another's middle would go in their split.
    static let splitDrop = Color.ink((0xFFFFFF, 0.85), (0xFFFFFF, 0.45))
    static let separator = Color.ink((0x000000, 0.15), (0xFFFFFF, 0.15))
    /// Secondary icons and labels.
    static let secondary = Color.ink((0x000000, 0.30), (0xFFFFFF, 0.45))
    /// A tab's title.
    static let primary = Color.ink((0x0E0F10, 0.90), (0xFFFFFF, 0.80))
}

/// The sidebar's measures, in points.
enum Measure {
    /// A row's height, of which its background takes `rowFill`, inset by `inset` at its sides.
    static let row: CGFloat = 40
    static let rowFill: CGFloat = 36
    static let inset: CGFloat = 8
    /// Rows', tiles' and pills' corners.
    static let radius: CGFloat = 12
    /// How far each folder level indents what it holds.
    static let indent: CGFloat = 16
    /// A favorite's tile.
    static let tile: CGFloat = 47
}
