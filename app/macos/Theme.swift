// A Space's colors over the window (ui/src/theme.ts).

import AppKit
import SwiftUI

/// Whether a theme shows light text: as its mode forces; in automatic appearance, like Arc's, as
/// the system's appearance is, unless its leading color is nearly white or nearly black.
@MainActor func isDark(_ theme: Theme?) -> Bool {
    guard let theme else { return systemDark() }
    if theme.mode != "auto" { return theme.mode == "dark" }
    guard let lead = theme.colors.first.map(luminance) else { return systemDark() }
    if lead > 0.5 { return false }
    if lead < 0.03 { return true }
    return systemDark()
}

@MainActor func systemDark() -> Bool {
    NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
}

/// A #rrggbb color's red, green and blue, from 0 to 1.
func channels(_ hex: String) -> [Double] {
    let value = UInt32(hex.dropFirst(), radix: 16) ?? 0
    return [16, 8, 0].map { Double((value >> $0) & 0xff) / 255 }
}

/// The relative luminance of a #rrggbb color, from 0 to 1.
func luminance(_ hex: String) -> Double {
    let linear = channels(hex).map { $0 <= 0.04045 ? $0 / 12.92 : pow(($0 + 0.055) / 1.055, 2.4) }
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
}

/// `share` of `color` mixed into `base`, both #rrggbb.
func mix(_ color: String, _ base: String, _ share: Double) -> String {
    let (from, to) = (channels(color), channels(base))
    let mixed = zip(from, to).map { Int((($0 * share + $1 * (1 - share)) * 255).rounded()) }
    return "#" + mixed.map { String(format: "%02x", $0) }.joined()
}

func color(_ hex: String) -> Color {
    let rgb = channels(hex)
    return Color(red: rgb[0], green: rgb[1], blue: rgb[2])
}

/// A theme's colors over the blur, like Arc's: black over a dark blur, at half, or white over a
/// light one, at 35%, then a gradient from the left edge to the right one and a third color rising from
/// the bottom edge, from faint to half opaque as the theme is more intense.
struct Tint: View {
    let theme: Theme?
    @Environment(\.controlActiveState) private var active

    var body: some View {
        ZStack {
            isDark(theme) ? Color.black.opacity(0.5) : Color.white.opacity(0.35)
            colors
            // In the background, the blur falls flat: without colors, Arc's window shows a deep
            // violet there, measured, rather than the blur's gray.
            if active == .inactive, isDark(theme), theme?.colors.isEmpty ?? true {
                color("#100918")
            }
        }
    }

    @ViewBuilder private var colors: some View {
        if let theme, let first = theme.colors.first {
            let second = theme.colors.count > 1 ? theme.colors[1] : first
            ZStack {
                LinearGradient(
                    colors: [color(first), color(second)], startPoint: .leading,
                    endPoint: .trailing)
                if theme.colors.count > 2 {
                    // Whole from four fifths of the way down, as much as Arc's shows.
                    LinearGradient(
                        stops: [
                            .init(color: color(theme.colors[2]).opacity(0), location: 0),
                            .init(color: color(theme.colors[2]), location: 0.8),
                        ], startPoint: .top, endPoint: .bottom)
                }
            }
            .opacity(min(0.2 + 0.55 * theme.intensity, 0.5))
        }
    }
}
