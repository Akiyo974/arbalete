// A Space's theme, edited like Arc's (ui/src/ThemePicker.tsx, show_theme_picker in
// app/src/shell.rs): dots on a pad, whose angle is a color's hue and distance its lightness, the
// first one leading; the system's, a light or a dark appearance; a palette to start from; the
// colors' intensity on a wavy slider; and their grain on a dial. Every change shows at once in
// the window. Escape and losing the keyboard close it.

import AppKit
import SwiftUI

/// The palette's pages, nine colors each.
let palettes: [[String]] = [
    ["#f3efe8", "#ec9cbd", "#9a72aa", "#e5525a", "#f47c45", "#f3d14b", "#38d27f", "#62b4d8", "#5a5d84"],
    ["#fdfbf8", "#fbe8f1", "#ead3ea", "#f6cdd2", "#fae0d3", "#fdf8e5", "#e2fbea", "#ddf1fb", "#c6c7e3"],
    ["#4a3b58", "#623855", "#844545", "#a6704a", "#d0b46b", "#cfcca8", "#84a985", "#365f4b", "#2e4467"],
    ["#ffffff", "#e6e6e6", "#cccccc", "#b3b3b3", "#808080", "#666666", "#333333", "#1a1a1a", "#000000"],
]

let maxColors = 3

/// A color's hue, from 0 to 360, saturation and lightness, from 0 to 1, in Display P3, like Arc's.
struct Hsl {
    var h: Double
    var s: Double
    var l: Double
}

func toHsl(_ hex: String) -> Hsl {
    let srgb = channels(hex)
    let p3 = NSColor(srgbRed: srgb[0], green: srgb[1], blue: srgb[2], alpha: 1)
        .usingColorSpace(.displayP3)
    let (r, g, b) = (
        p3.map { Double($0.redComponent) } ?? srgb[0], p3.map { Double($0.greenComponent) } ?? srgb[1],
        p3.map { Double($0.blueComponent) } ?? srgb[2])
    let (high, low) = (max(r, g, b), min(r, g, b))
    let l = (high + low) / 2
    if high == low { return Hsl(h: 0, s: 0, l: l) }
    let d = high - low
    let s = l > 0.5 ? d / (2 - high - low) : d / (high + low)
    let h =
        high == r ? (g - b) / d + (g < b ? 6 : 0) : high == g ? (b - r) / d + 2 : (r - g) / d + 4
    return Hsl(h: h * 60, s: s, l: l)
}

/// A color from its Display P3 hue, saturation and lightness, as the #rrggbb in sRGB nearest to it.
func fromHsl(_ hsl: Hsl) -> String {
    let a = hsl.s * min(hsl.l, 1 - hsl.l)
    func channel(_ n: Double) -> Double {
        let k = (n + hsl.h / 30).truncatingRemainder(dividingBy: 12)
        return hsl.l - a * max(-1, min(k - 3, 9 - k, 1))
    }
    let p3 = NSColor(displayP3Red: channel(0), green: channel(8), blue: channel(4), alpha: 1)
    let srgb = p3.usingColorSpace(.sRGB) ?? p3
    let byte = { (value: CGFloat) in Int((min(1, max(0, value)) * 255).rounded()) }
    return String(
        format: "#%02x%02x%02x", byte(srgb.redComponent), byte(srgb.greenComponent),
        byte(srgb.blueComponent))
}

/// The pad: a dot's angle around the center is the color's hue, clockwise from the right, and
/// its distance from the center its lightness, across `range`, at one saturation.
struct Wheel {
    var saturation: Double
    var range: (Double, Double)
}

/// A dot at the pad's edge sits this far from the center, its radius being 1.
private let edge = 0.99

/// The pad for colors, from the leading one, as Arc's opens: as saturated, its lightness ±0.25.
/// Without colors, the whole range.
func wheelFor(_ colors: [String]) -> Wheel {
    let (lowest, highest) = (0.01, 0.97)
    guard let lead = colors.first else {
        return Wheel(saturation: 0.8, range: (lowest, highest))
    }
    let hsl = toHsl(lead)
    return Wheel(
        saturation: max(0.01, hsl.s), range: (max(lowest, hsl.l - 0.25), min(highest, hsl.l + 0.25)))
}

/// The lightness a light or a dark appearance keeps the pad to.
func forcedRange(dark: Bool) -> (Double, Double) {
    dark ? (0.01, 0.425) : (0.575, 0.97)
}

/// The color at a point of the pad, from its center, its radius being 1.
func colorAt(_ wheel: Wheel, _ x: Double, _ y: Double) -> String {
    var hue = atan2(y, x) * 180 / .pi
    if hue < 0 { hue += 360 }
    let t = min(1, max(0, hypot(x, y) / edge))
    let (low, high) = wheel.range
    return fromHsl(Hsl(h: hue, s: wheel.saturation, l: low + t * (high - low)))
}

/// Where a color sits on the pad, from its center, its radius being 1.
func pointOf(_ wheel: Wheel, _ color: String) -> CGPoint {
    let hsl = toHsl(color)
    let (low, high) = wheel.range
    let t = high > low ? min(1, max(0, (hsl.l - low) / (high - low))) : 0
    let angle = hsl.h * .pi / 180
    return CGPoint(x: cos(angle) * t * edge, y: sin(angle) * t * edge)
}

/// The hues, from the leading color's, of a theme's colors: its complement for two, both sides of
/// it for three.
private let harmonies: [[Double]] = [[], [0], [0, 180], [0, 150, 210]]

/// The colors that go with a leading one, as many as the theme holds, as far from the center.
func harmony(_ color: String, _ count: Int) -> [String] {
    let hsl = toHsl(color)
    return harmonies[min(count, maxColors)].map { offset in
        offset == 0
            ? color
            : fromHsl(Hsl(h: (hsl.h + offset).truncatingRemainder(dividingBy: 360), s: hsl.s, l: hsl.l))
    }
}

@MainActor
final class ThemePickerModel: ObservableObject {
    @Published var space = 0
    @Published var theme = Theme(colors: [], mode: "auto", intensity: 1, noise: 0)
    @Published var wheel = Wheel(saturation: 0.8, range: (0.01, 0.97))
    /// Where the dots sit, one for each color. A drag places them where the pointer takes them,
    /// as a color's #rrggbb, rounded, would not: a grayish one barely keeps its hue.
    @Published var points: [CGPoint] = []
    @Published var page = 0
    let app: AppModel
    /// The theme to send next, at most one every frame's time while a dot or a knob moves.
    private var pending: Theme?

    init(app: AppModel) {
        self.app = app
    }

    func handle(_ name: String, _ detail: Data) {
        guard name == "arbalete:theme" else { return }
        struct Shown: Decodable {
            var space: Int
            var theme: Theme
        }
        guard let shown = try? JSONDecoder().decode(Shown.self, from: detail) else { return }
        space = shown.space
        theme = shown.theme
        wheel = wheelFor(shown.theme.colors)
        points = shown.theme.colors.map { pointOf(wheel, $0) }
    }

    /// Like Arc's, a light or a dark appearance keeps the pad to its half of the lightness, the
    /// dots staying where they are and their colors following; the automatic one centers it on
    /// the leading color again.
    func setMode(_ mode: String) {
        guard mode != "auto" else {
            change({ $0.mode = mode }, rewheel: true)
            return
        }
        wheel.range = forcedRange(dark: mode == "dark")
        let (wheel, points) = (wheel, points)
        change(
            {
                $0.mode = mode
                $0.colors = points.map { colorAt(wheel, $0.x, $0.y) }
            }, at: points)
    }

    /// Changes the theme; dots `at` points give its colors, other new colors place their dots.
    func change(
        _ update: (inout Theme) -> Void, rewheel: Bool = false, at points: [CGPoint]? = nil
    ) {
        let colors = theme.colors
        update(&theme)
        if rewheel { wheel = wheelFor(theme.colors) }
        if let points {
            self.points = points
        } else if rewheel || theme.colors != colors {
            self.points = theme.colors.map { pointOf(wheel, $0) }
        }
        if pending == nil {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.016) { [weak self] in
                guard let self, let sent = self.pending else { return }
                self.pending = nil
                let json = (try? JSONEncoder().encode(sent)).flatMap { String(data: $0, encoding: .utf8) }
                self.app.run("space/theme/\(self.space)/\(percentEncoded(json ?? ""))")
            }
        }
        pending = theme
    }
}

struct ThemePickerView: View {
    @ObservedObject var picker: ThemePickerModel

    var body: some View {
        let theme = picker.theme
        let dark = isDark(theme)
        VStack(spacing: 10) {
            Pad(picker: picker)
            Palette(picker: picker)
            HStack {
                SineSlider(value: theme.intensity, enabled: !theme.colors.isEmpty) { intensity in
                    picker.change { $0.intensity = intensity }
                }
                Spacer()
                Dial(value: theme.noise, enabled: !theme.colors.isEmpty) { noise in
                    picker.change { $0.noise = noise }
                }
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        // The window's colors, through the panel's blur, darkened for its light controls.
        .background(
            ZStack {
                Tint(theme: theme)
                Color.black.opacity(dark ? 0.45 : 0.3)
            })
        .foregroundStyle(.white)
        .environment(\.colorScheme, .dark)
    }
}

/// The dots of `count` colors that go with a leading one at `lead`, like `harmony`: turned by
/// their hues, as far from the center.
func arranged(_ lead: CGPoint, _ count: Int) -> [CGPoint] {
    harmonies[min(count, maxColors)].map { offset in
        let (c, s) = (cos(offset * .pi / 180), sin(offset * .pi / 180))
        return CGPoint(x: lead.x * c - lead.y * s, y: lead.x * s + lead.y * c)
    }
}

/// The pad's dots, which keep in harmony, like Arc's: a click on an empty pad picks the first
/// color, and the dot a drag takes leads, the others turning around with it; − and + remove a
/// color and add one.
struct Pad: View {
    @ObservedObject var picker: ThemePickerModel
    /// Where the dot a drag took was, which moves by the drag's distance; none elsewhere.
    private let taken = State<CGPoint?>(initialValue: nil)

    var body: some View {
        let colors = picker.theme.colors
        GeometryReader { geometry in
            let size = geometry.size
            let radius = min(size.width, size.height) / 2
            let point = { (location: CGPoint) in
                CGPoint(
                    x: (location.x - size.width / 2) / radius,
                    y: (location.y - size.height / 2) / radius)
            }
            ZStack {
                RoundedRectangle(cornerRadius: 16).fill(.white.opacity(0.05))
                DotGrid().clipShape(RoundedRectangle(cornerRadius: 16))
                if colors.isEmpty {
                    Text("Tap to pick a color for this space")
                        .font(.system(size: 13))
                        .foregroundStyle(.white.opacity(0.7))
                }
                ForEach(Array(zip(colors, picker.points).enumerated()), id: \.offset) { index, dot in
                    let (hex, at) = dot
                    Circle()
                        .fill(color(hex))
                        .overlay(Circle().strokeBorder(.white, lineWidth: index == 0 ? 5 : 3))
                        .frame(width: index == 0 ? 40 : 24, height: index == 0 ? 40 : 24)
                        .shadow(radius: 2)
                        .position(
                            x: size.width / 2 + at.x * radius, y: size.height / 2 + at.y * radius)
                }
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { drag in
                        // The dot taken leads, moving from where it was; elsewhere, the leading
                        // one comes under the pointer.
                        if drag.translation == .zero {
                            let start = point(drag.startLocation)
                            taken.wrappedValue = picker.points.last { dot in
                                hypot(dot.x - start.x, dot.y - start.y) * radius < 20
                            }
                        }
                        let at = taken.wrappedValue.map { dot in
                            CGPoint(
                                x: dot.x + drag.translation.width / radius,
                                y: dot.y + drag.translation.height / radius)
                        } ?? point(drag.location)
                        arrange(onPad(at), max(1, colors.count))
                    }
                    .onEnded { _ in taken.wrappedValue = nil })
            .overlay(alignment: .top) {
                ModeButtons(picker: picker).padding(8)
            }
            .overlay(alignment: .bottom) {
                HStack(spacing: 6) {
                    PickerButton(symbol: "minus", label: "Remove a color") {
                        if colors.count == 1 {
                            picker.change({ $0.colors = [] }, at: [])
                        } else {
                            arrange(picker.points[0], colors.count - 1)
                        }
                    }
                    .disabled(colors.isEmpty)
                    PickerButton(symbol: "plus", label: "Add a color") {
                        arrange(picker.points.first ?? CGPoint(x: 0, y: -0.5), colors.count + 1)
                    }
                    .disabled(colors.count == maxColors)
                }
                .padding(8)
            }
        }
        .aspectRatio(1, contentMode: .fit)
    }

    /// Places `count` dots in harmony, the leading one at `lead`.
    private func arrange(_ lead: CGPoint, _ count: Int) {
        let wheel = picker.wheel
        let points = arranged(lead, count)
        picker.change({ $0.colors = points.map { colorAt(wheel, $0.x, $0.y) } }, at: points)
    }
}

/// A point of the pad, from its center, brought within its edge.
func onPad(_ point: CGPoint) -> CGPoint {
    let length = hypot(point.x, point.y)
    return length > edge ? CGPoint(x: point.x / length * edge, y: point.y / length * edge) : point
}

/// The pad's grid of faint dots.
struct DotGrid: View {
    var body: some View {
        Canvas { context, size in
            var x: CGFloat = 3
            while x < size.width {
                var y: CGFloat = 3
                while y < size.height {
                    context.fill(
                        Path(ellipseIn: CGRect(x: x - 0.8, y: y - 0.8, width: 1.6, height: 1.6)),
                        with: .color(.white.opacity(0.2)))
                    y += 6
                }
                x += 6
            }
        }
    }
}

struct PickerButton: View {
    let symbol: String
    let label: String
    var pressed = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .frame(width: 32, height: 32)
                .background(Circle().fill(.white.opacity(pressed ? 0.3 : 0.12)))
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .help(label)
        .accessibilityLabel(label)
    }
}

struct ModeButtons: View {
    @ObservedObject var picker: ThemePickerModel

    var body: some View {
        HStack(spacing: 8) {
            ForEach(
                [("auto", "Automatic appearance", "sparkles"), ("light", "Light appearance", "sun.max"),
                 ("dark", "Dark appearance", "moon.stars")], id: \.0
            ) { mode, label, symbol in
                PickerButton(symbol: symbol, label: label, pressed: picker.theme.mode == mode) {
                    picker.setMode(mode)
                }
            }
        }
    }
}

struct Palette: View {
    @ObservedObject var picker: ThemePickerModel

    var body: some View {
        HStack(spacing: 6) {
            Button {
                picker.page -= 1
            } label: {
                Image(systemName: "chevron.left").frame(width: 18, height: 24)
            }
            .buttonStyle(.plain)
            .disabled(picker.page == 0)
            .help("Previous colors")
            ForEach(palettes[picker.page], id: \.self) { hex in
                Button {
                    let colors = harmony(hex, max(1, picker.theme.colors.count))
                    // A light or a dark appearance keeps its lightness, like Arc's.
                    picker.change({ $0.colors = colors }, rewheel: picker.theme.mode == "auto")
                } label: {
                    Circle().fill(color(hex)).frame(width: 24, height: 24)
                        .overlay(Circle().strokeBorder(.white.opacity(0.3)))
                }
                .buttonStyle(.plain)
                .help(hex)
            }
            Button {
                picker.page += 1
            } label: {
                Image(systemName: "chevron.right").frame(width: 18, height: 24)
            }
            .buttonStyle(.plain)
            .disabled(picker.page == palettes.count - 1)
            .help("More colors")
        }
    }
}

/// The colors' intensity, from 0 to 1: a wave up to the thumb, as tall as the value, and its
/// rest dimmed past it.
struct SineSlider: View {
    let value: Double
    let enabled: Bool
    let onChange: (Double) -> Void
    private let size = CGSize(width: 228, height: 64)
    private let waves = 6.0
    private let amplitude = 13.0

    var body: some View {
        Canvas { context, size in
            let thumb = 14 + value * (size.width - 28)
            let track = Path(
                roundedRect: CGRect(x: 0, y: size.height / 2 - 16, width: size.width, height: 32),
                cornerRadius: 16)
            context.fill(track, with: .color(.white.opacity(0.1)))
            func wave(_ from: Double, _ to: Double) -> Path {
                var path = Path()
                var x = from
                while x <= to {
                    let y = size.height / 2 - sin(x / size.width * waves * 2 * .pi) * amplitude * value
                    if x == from { path.move(to: CGPoint(x: x, y: y)) } else { path.addLine(to: CGPoint(x: x, y: y)) }
                    x += 2
                }
                return path
            }
            context.stroke(wave(thumb, size.width - 12), with: .color(.white.opacity(0.25)), lineWidth: 3)
            context.stroke(wave(12, thumb), with: .color(.white), lineWidth: 3)
            context.fill(
                Path(roundedRect: CGRect(x: thumb - 14, y: 4, width: 28, height: size.height - 8), cornerRadius: 14),
                with: .color(.white))
        }
        .frame(width: size.width, height: size.height)
        .opacity(enabled ? 1 : 0.4)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0).onChanged { drag in
                guard enabled else { return }
                onChange(min(1, max(0, (drag.location.x - 14) / (size.width - 28))))
            })
        .accessibilityElement()
        .accessibilityLabel("Intensity")
        .accessibilityValue("\(Int(value * 100))")
        .accessibilityAdjustableAction { direction in
            guard enabled else { return }
            onChange(min(1, max(0, value + (direction == .increment ? 0.05 : -0.05))))
        }
    }
}

/// The grain, from 0 to 1, on a dial: the dots up to its handle light up, and the disk shows it.
struct Dial: View {
    let value: Double
    let enabled: Bool
    let onChange: (Double) -> Void
    private let dots = 24
    /// The turn the dots span from the top, clockwise, leaving a gap before the top.
    private let span = 2 * Double.pi - 0.25
    private let size = 92.0

    var body: some View {
        Canvas { context, canvas in
            let center = CGPoint(x: size / 2, y: size / 2)
            let ring = size / 2 - 6
            let angle = { (fraction: Double) in fraction * span - .pi / 2 }
            for i in 0..<dots {
                let fraction = Double(i) / Double(dots - 1)
                let a = angle(fraction)
                let lit = enabled && value > 0 && fraction <= value + 1e-6
                let at = CGPoint(x: center.x + cos(a) * ring, y: center.y + sin(a) * ring)
                context.fill(
                    Path(ellipseIn: CGRect(x: at.x - 2.6, y: at.y - 2.6, width: 5.2, height: 5.2)),
                    with: .color(.white.opacity(lit ? 1 : 0.25)))
            }
            let disk = CGRect(x: center.x - (ring - 12), y: center.y - (ring - 12), width: 2 * (ring - 12), height: 2 * (ring - 12))
            context.fill(Path(ellipseIn: disk), with: .color(.white.opacity(0.12)))
            // The grain on the disk: specks, the more the grainier.
            var generator = SplitMix(seed: 7)
            for _ in 0..<Int(400 * value) {
                let r = sqrt(Double(generator.next() % 1000) / 1000) * (ring - 12)
                let t = Double(generator.next() % 1000) / 1000 * 2 * .pi
                context.fill(
                    Path(CGRect(x: center.x + cos(t) * r, y: center.y + sin(t) * r, width: 1, height: 1)),
                    with: .color(.white.opacity(0.5)))
            }
            let handle = angle(value)
            var line = Path()
            line.move(to: CGPoint(x: center.x + cos(handle) * (ring - 6), y: center.y + sin(handle) * (ring - 6)))
            line.addLine(to: CGPoint(x: center.x + cos(handle) * (ring + 4), y: center.y + sin(handle) * (ring + 4)))
            context.stroke(line, with: .color(.white), style: StrokeStyle(lineWidth: 3, lineCap: .round))
        }
        .frame(width: size, height: size)
        .opacity(enabled ? 1 : 0.4)
        .contentShape(Circle())
        .gesture(
            DragGesture(minimumDistance: 0).onChanged { drag in
                guard enabled else { return }
                let x = drag.location.x - size / 2
                let y = drag.location.y - size / 2
                let from = (atan2(y, x) + .pi / 2 + 2 * .pi).truncatingRemainder(dividingBy: 2 * .pi)
                onChange(min(1, from / span))
            })
        .accessibilityElement()
        .accessibilityLabel("Grain")
        .accessibilityValue("\(Int(value * 100))")
        .accessibilityAdjustableAction { direction in
            guard enabled else { return }
            onChange(min(1, max(0, value + (direction == .increment ? 0.05 : -0.05))))
        }
    }
}

/// A small seeded generator, so that the dial's grain stays still as it redraws.
struct SplitMix {
    var state: UInt64

    init(seed: UInt64) {
        state = seed
    }

    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}
