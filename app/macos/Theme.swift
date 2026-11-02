// A Space's colors over the window (ui/src/theme.ts).

import AppKit
import SwiftUI

/// Whether a theme shows light text, like Arc's: as its mode forces; in automatic appearance, as
/// its leading color holds the palette's dark title better than its light one; without colors, as
/// the system's appearance is.
@MainActor func isDark(_ theme: Theme?) -> Bool {
    guard let theme, let lead = theme.colors.first else { return systemDark() }
    if theme.mode != "auto" { return theme.mode == "dark" }
    let titles = ThemePalette(lead).titles
    return contrast(lead, titles.dark) >= contrast(lead, titles.light)
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

/// The WCAG contrast between two #rrggbb colors, from 1 to 21.
func contrast(_ a: String, _ b: String) -> Double {
    let (x, y) = (luminance(a), luminance(b))
    return (max(x, y) + 0.05) / (min(x, y) + 0.05)
}

/// A theme's palette from its leading color, like Arc's (PrimaryColorPalette): the color made
/// lighter or darker, by steps of 0.01 of its Display P3 lightness, until its luminance falls in
/// a band.
struct ThemePalette {
    var tintedLight: String
    var shadedDark: String

    init(_ lead: String) {
        tintedLight = ThemePalette.within(lead, 0.85, 0.95)
        shadedDark = ThemePalette.within(lead, 0, 0.1)
    }

    /// The tabs' titles, over a light and over a dark background (SemanticColorPalette.title):
    /// the dark shade at 30% over black, and at 15% over white.
    var titles: (light: String, dark: String) {
        (mix(shadedDark, "#000000", 0.3), mix(shadedDark, "#ffffff", 0.15))
    }

    /// The color that stands out most on the theme's background (maxContrastColor), which marks
    /// where a drop lands.
    func strongest(dark: Bool) -> String { dark ? tintedLight : shadedDark }

    private static func within(_ hex: String, _ low: Double, _ high: Double) -> String {
        var hsl = toHsl(hex)
        // Luminance as Arc measures it, on the Display P3 components.
        func lum(_ hsl: Hsl) -> Double {
            let p3 = p3Channels(hsl).map {
                $0 <= 0.03928 ? $0 / 12.92 : pow(($0 + 0.055) / 1.055, 2.4)
            }
            return 0.2126 * p3[0] + 0.7152 * p3[1] + 0.0722 * p3[2]
        }
        if lum(hsl) < low {
            while lum(hsl) < min(low, 1), hsl.l < 1 { hsl.l = min(1, hsl.l + 0.01) }
        } else if lum(hsl) > high {
            while lum(hsl) > max(high, 0), hsl.l > 0 { hsl.l = max(0, hsl.l - 0.01) }
        } else {
            return hex
        }
        return fromHsl(hsl)
    }
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

/// Arc's dark window without theme colors.
private let defaultViolet = "#100a1c"

/// A theme's colors over the blur, like Arc's: the colors at 20% to 75% as the theme is more
/// intense, over a wash as opaque up to half, white, or black at a third of that in a dark
/// appearance; and grain, multiplied, fainter while the app is in the background. Two colors
/// make a gradient from the top left corner, whole from three quarters of the way; a third one
/// rises from the bottom edge. Without colors, a dark window is Arc's deep violet.
struct Tint: View {
    let theme: Theme?
    /// Whether the app is in front, which shows the blur.
    var active = true

    var body: some View {
        if let theme, !theme.colors.isEmpty {
            let alpha = 0.2 + 0.55 * theme.intensity
            let under = min(alpha, 0.5)
            ZStack {
                systemDark()
                    ? Color.black.opacity(0.35 * under) : Color.white.opacity(under)
                Self.colors(theme.colors)
                    .drawingGroup(colorMode: .extendedLinear)
                    .opacity(alpha)
                if theme.noise > 0 {
                    Image(nsImage: grain).resizable(resizingMode: .tile)
                        .blendMode(.multiply)
                        .opacity(theme.noise * 0.8 * (active ? 1 : 0.25))
                }
            }
        } else if isDark(theme) {
            // Three quarters over the blur, measured, and whole once it falls flat.
            color(defaultViolet).opacity(active ? 0.75 : 1)
        }
    }

    @ViewBuilder static func colors(_ colors: [String]) -> some View {
        if colors.count == 1 {
            color(colors[0])
        } else {
            ZStack {
                LinearGradient(
                    stops: [
                        .init(color: color(colors[0]), location: 0),
                        .init(color: color(colors[1]), location: 0.75),
                    ], startPoint: .topLeading, endPoint: .bottomTrailing)
                if colors.count > 2 {
                    LinearGradient(
                        colors: [color(colors[2]).opacity(0), color(colors[2])], startPoint: .top,
                        endPoint: .bottom)
                }
            }
        }
    }
}

/// Grain of our own, a tile of gray noise, which multiplies the colors like Arc's.
private let grain: NSImage = {
    let size = 128
    var generator = SystemRandomNumberGenerator()
    let pixels = (0..<size * size).map { _ in UInt8.random(in: 180...255, using: &generator) }
    let provider = CGDataProvider(data: Data(pixels) as CFData)!
    let image = CGImage(
        width: size, height: size, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: size,
        space: CGColorSpaceCreateDeviceGray(), bitmapInfo: [], provider: provider, decode: nil,
        shouldInterpolate: false, intent: .defaultIntent)!
    return NSImage(cgImage: image, size: NSSize(width: size, height: size))
}()
