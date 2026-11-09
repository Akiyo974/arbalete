// The Boost editor, like Arc's, measured from a recording of it (decision 0010): a light panel
// against the page's right edge, its header's menu over a color square, the colors' buttons, a
// grid of fonts, the text size and case, Zap and Code, which shows the CSS and JavaScript in the
// panel's place. The app keeps the Boost and its undo (app/src/shell/boosts.rs); the panel sends
// it each change.

import AppKit
import SwiftUI
import WebKit

/// A Boost, as the app sends it (arbalete_core::boosts::Boost).
struct Boost: Codable, Equatable {
    struct Colors: Codable, Equatable {
        var hue: Double
        var saturation: Double
        var invert: Bool
        var contrast: Double
        var brightness: Double
        var original: Double

        static let none = Colors(
            hue: 0, saturation: 0, invert: false, contrast: 1, brightness: 1, original: 0.3)
    }
    enum TextCase: String, Codable {
        case capitalize, uppercase, lowercase
    }
    enum TextSize: String, Codable {
        case small, medium, large, larger
    }
    var id: UInt64
    var site: String
    var title: String
    var active: Bool
    var zaps: [String]
    var colors: Colors?
    var font: String?
    var textCase: TextCase?
    var textSize: TextSize?
    var css: String
    var js: String
}

/// The editor's state, from the app's events, and the panel showing it.
@MainActor
final class BoostEditorModel: ObservableObject {
    struct Editing: Decodable {
        var boost: Boost
        var canUndo: Bool
        var canRedo: Bool
        var zapping: Bool
    }
    @Published private(set) var editing: Editing?
    /// The code shows in the panel's place.
    @Published var coding = false
    @Published var tab = 0
    /// Where the user dragged the panel, from its place against the page's right edge.
    var offset = CGSize.zero
    private var dragStart: CGSize?
    /// The panel's height, as it measures itself.
    var height: CGFloat = 480 {
        didSet { if height != oldValue { place() } }
    }
    let app: AppModel
    private var surface: Surface?
    private var page: NSRect?
    static let width: CGFloat = 183
    static let codeWidth: CGFloat = 420

    init(in window: NSWindow, app: AppModel) {
        self.app = app
        let surface = Surface(in: window, radius: 12) {
            BoostEditorView(model: self).environment(\.colorScheme, .light)
        }
        // Opaque, like Arc's.
        surface.tint = NSColor(white: 0.97, alpha: 1)
        surface.panelEdge()
        self.surface = surface
    }

    func handle(_ name: String, _ detail: Data) {
        switch name {
        case "arbalete:boost-editor":
            let before = editing?.boost.id
            editing = try? JSONDecoder().decode(Editing.self, from: detail)
            if editing?.boost.id != before {
                coding = false
            }
            place()
        case "arbalete:boost-frame":
            page = (try? JSONDecoder().decode(PageFrame.self, from: detail))?.rect
            place()
        case "arbalete:boost-pick":
            (try? JSONDecoder().decode(String.self, from: detail)).map(code.insert)
        case "arbalete:boost-globals":
            (try? JSONDecoder().decode([String].self, from: detail)).map(code.globals)
        default: break
        }
    }

    func place() {
        guard let surface else { return }
        guard editing != nil, let page else {
            surface.place(false, .zero)
            return
        }
        let width = coding ? Self.codeWidth : Self.width
        let height = coding ? min(560, page.height - 40) : min(height, page.height - 16)
        // Like Arc's, against the page's right edge, halfway down.
        var frame = NSRect(
            x: page.maxX - width - 6 + offset.width,
            y: page.minY + max(8, (page.height - height) / 2) + offset.height,
            width: width, height: height)
        // Kept on the page.
        frame.origin.x = min(max(frame.minX, page.minX + 6), page.maxX - width - 6)
        frame.origin.y = min(max(frame.minY, page.minY + 6), page.maxY - height - 6)
        surface.place(true, frame)
    }

    /// Moves the panel as it is dragged, from where the drag started.
    func drag(_ translation: CGSize?) {
        guard let translation else {
            dragStart = nil
            return
        }
        let start = dragStart ?? offset
        dragStart = start
        offset = CGSize(width: start.width + translation.width, height: start.height + translation.height)
        place()
    }

    /// Sends the app a change to the Boost; a gesture's changes undo as one.
    func change(_ change: (inout Boost) -> Void, gesture: Bool = false) {
        guard var boost = editing?.boost else { return }
        change(&boost)
        guard boost != editing?.boost,
            let data = try? JSONEncoder().encode(boost),
            let object = try? JSONSerialization.jsonObject(with: data)
        else { return }
        editing?.boost = boost
        app.run("change-boost", object, gesture)
    }

    func action(_ name: String) { app.run("boost-editor", name) }

    func zap(_ on: Bool) { app.run("boost-editor", ["zap": on]) }

    /// Monaco, made the first time the code shows.
    lazy var code = BoostCode(model: self)

    func showCode(_ shown: Bool) {
        coding = shown
        place()
        guard shown, let boost = editing?.boost else { return }
        code.show(css: boost.css, js: boost.js, tab: tab)
        // The page's own globals, which its JavaScript may use.
        action("globals")
    }

    func close() { surface?.close() }
}

/// Where the page is, in the window's points from its top left corner.
private struct PageFrame: Decodable {
    var x, y, width, height: Double
    var rect: NSRect { NSRect(x: x, y: y, width: width, height: height) }
}

/// The fonts of the grid, like Arc's twenty, by family; a click on the one picked gives the
/// site's own back.
private let boostFonts: [(name: String, family: String)] = [
    ("System", "-apple-system"), ("Rounded", "ui-rounded"), ("Helvetica Neue", "Helvetica Neue"),
    ("Avenir Next", "Avenir Next"), ("Futura", "Futura"), ("Gill Sans", "Gill Sans"),
    ("Optima", "Optima"), ("Verdana", "Verdana"), ("Trebuchet MS", "Trebuchet MS"),
    ("Arial Rounded", "Arial Rounded MT Bold"), ("New York", "ui-serif"), ("Georgia", "Georgia"),
    ("Palatino", "Palatino"), ("Baskerville", "Baskerville"), ("Didot", "Didot"),
    ("Snell Roundhand", "Snell Roundhand"), ("Monospace", "ui-monospace"),
    ("American Typewriter", "American Typewriter"), ("Copperplate", "Copperplate"),
    ("Zapfino", "Zapfino"),
]

private let yellow = [Color(red: 1, green: 0.8, blue: 0.2), Color(red: 0.98, green: 0.7, blue: 0.08)]
private let orange = [Color(red: 1, green: 0.62, blue: 0.2), Color(red: 0.97, green: 0.5, blue: 0.1)]
private let red = [Color(red: 1, green: 0.33, blue: 0.2), Color(red: 0.93, green: 0.18, blue: 0.12)]
private let blue = [Color(red: 0.3, green: 0.6, blue: 1), Color(red: 0.15, green: 0.45, blue: 0.95)]

/// The text sizes the Size button steps through, from the site's own, what it reads and its color.
private let boostSizes: [(Boost.TextSize, String, [Color])] = [
    (.medium, "110 %", yellow), (.large, "125 %", orange), (.larger, "150 %", red),
    (.small, "90 %", blue),
]
private let boostCases: [(Boost.TextCase, String, [Color])] = [
    (.uppercase, "AA", yellow), (.lowercase, "aa", orange), (.capitalize, "Aa", red),
]

/// The next of a button's values, the site's own after the last.
private func next<T: Equatable>(_ value: T?, in values: [(T, String, [Color])]) -> T? {
    guard let value, let index = values.firstIndex(where: { $0.0 == value }) else {
        return values.first?.0
    }
    return index + 1 < values.count ? values[index + 1].0 : nil
}

struct BoostEditorView: View {
    @ObservedObject var model: BoostEditorModel

    var body: some View {
        if let editing = model.editing {
            Group {
                if model.coding {
                    BoostCodeView(model: model)
                } else {
                    BoostPanel(model: model, editing: editing)
                        .fixedSize(horizontal: false, vertical: true)
                        .background(
                            GeometryReader { geometry in
                                Color.clear.onAppear { model.height = geometry.size.height }
                                    .onChange(of: geometry.size.height) { _, height in
                                        model.height = height
                                    }
                            })
                        .frame(maxHeight: .infinity, alignment: .top)
                }
            }
            .foregroundStyle(Color(white: 0.15))
            // Its surface rounds only its blur: what is drawn opaque over it is rounded here.
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            // Like Arc's, the panel moves where it is dragged, but from its controls.
            .gesture(
                DragGesture(minimumDistance: 3, coordinateSpace: .global)
                    .onChanged { drag in model.drag(drag.translation) }
                    .onEnded { _ in model.drag(nil) })
        }
    }
}

struct BoostPanel: View {
    let model: BoostEditorModel
    let editing: BoostEditorModel.Editing
    private let renaming = State(initialValue: false)
    private let title = State(initialValue: "")
    private let advanced = State(initialValue: false)

    var body: some View {
        let boost = editing.boost
        let colors = boost.colors ?? .none
        VStack(spacing: 0) {
            header(boost)
            VStack(spacing: 14) {
                BoostColorSquare(colors: boost.colors) { colors, gesture in
                    model.change({ $0.colors = colors }, gesture: gesture)
                }
                HStack(spacing: 10) {
                    BoostIconButton(
                        symbol: "lightbulb", label: "Invert Lightness", on: colors.invert
                    ) {
                        model.change { boost in
                            var colors = boost.colors ?? .none
                            colors.invert.toggle()
                            boost.colors = colors == .none ? nil : colors
                        }
                    }
                    BoostIconButton(
                        symbol: "slider.horizontal.3", label: "Advanced Color Controls",
                        on: advanced.wrappedValue
                    ) { advanced.wrappedValue.toggle() }
                    .popover(isPresented: advanced.projectedValue, arrowEdge: .bottom) {
                        BoostAdvancedColors(colors: colors) { colors, gesture in
                            model.change({ $0.colors = colors == .none ? nil : colors }, gesture: gesture)
                        }
                    }
                    BoostIconButton(symbol: "nosign", label: "Reset to Original Colors", on: false) {
                        model.change { $0.colors = nil }
                    }
                    .disabled(boost.colors == nil)
                }
                BoostFontGrid(font: boost.font) { family in model.change { $0.font = family } }
                HStack(spacing: 10) {
                    let size = boostSizes.first { $0.0 == boost.textSize }
                    BoostWideButton(title: size?.1 ?? "Size", fill: size?.2) {
                        model.change { $0.textSize = next($0.textSize, in: boostSizes) }
                    }
                    .help("Text Size")
                    let textCase = boostCases.first { $0.0 == boost.textCase }
                    BoostWideButton(title: textCase?.1 ?? "Case", fill: textCase?.2) {
                        model.change { $0.textCase = next($0.textCase, in: boostCases) }
                    }
                    .help("Text Case")
                }
                BoostRowButton(
                    title: "Zap", dark: editing.zapping || !boost.zaps.isEmpty,
                    trailing: boost.zaps.isEmpty ? nil : "\(boost.zaps.count)", symbol: "bolt.fill"
                ) { model.zap(!editing.zapping) }
                BoostRowButton(title: "Code", dark: false, trailing: nil, symbol: "curlybraces") {
                    model.showCode(true)
                }
            }
            .padding(.horizontal, 19)
            .padding(.top, 14)
            .padding(.bottom, 18)
        }
        .frame(width: BoostEditorModel.width)
    }

    /// Like Arc's: close, the Boost's name over its menu, and undo.
    private func header(_ boost: Boost) -> some View {
        HStack(spacing: 0) {
            Button { model.action("close") } label: {
                Image(systemName: "xmark.circle.fill").font(.system(size: 14))
                    .foregroundStyle(Color(white: 0.6))
            }
            .buttonStyle(.plain).help("Done")
            Spacer(minLength: 4)
            if renaming.wrappedValue {
                TextField("Name", text: title.projectedValue)
                    .textFieldStyle(.plain).multilineTextAlignment(.center)
                    .font(.system(size: 13, weight: .semibold))
                    .onSubmit {
                        model.app.run("rename-boost", boost.id, title.wrappedValue)
                        renaming.wrappedValue = false
                    }
            } else {
                Menu {
                    Button("Rename this Boost…") {
                        title.wrappedValue = boost.title
                        renaming.wrappedValue = true
                    }
                    Button("Shuffle") {
                        model.change {
                            var colors = $0.colors ?? .none
                            colors.hue = Double.random(in: 0..<360).rounded()
                            colors.saturation = Double.random(in: 0.45...1)
                            $0.colors = colors
                        }
                    }
                    Button("Reset all Edits") { model.action("reset") }
                    Button("Delete this Boost") { model.app.run("delete-boost", boost.id) }
                    Divider()
                    Button("All Boosts…") {
                        model.action("close")
                        model.app.openLibrary(.boosts)
                    }
                } label: {
                    HStack(spacing: 5) {
                        Text(boost.title).font(.system(size: 13, weight: .semibold)).lineLimit(1)
                        Image(systemName: "chevron.down").font(.system(size: 9, weight: .bold))
                    }
                }
                .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
            }
            Spacer(minLength: 4)
            Button { model.action("undo") } label: {
                Image(systemName: "arrow.counterclockwise").font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Color(white: editing.canUndo ? 0.35 : 0.7))
            }
            .buttonStyle(.plain).help("Undo")
            .disabled(!editing.canUndo)
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        .background(Color(white: 0.96))
        .overlay(alignment: .bottom) { Rectangle().fill(Color(white: 0.88)).frame(height: 0.5) }
    }
}

/// The color square, like Arc's: a rainbow edge round a pale square, the accent's puck on it
/// and its complement's opposite; dragged to the middle, the site keeps its colors. Grey while
/// it keeps them.
struct BoostColorSquare: View {
    let colors: Boost.Colors?
    let changed: (Boost.Colors?, Bool) -> Void
    private let size: CGFloat = 145

    var body: some View {
        let half = size / 2
        let tinted = (colors?.saturation ?? 0) > 0
        let edge = AngularGradient(
            colors: tinted
                ? stride(from: 0.0, through: 360, by: 30).map {
                    Color(hue: $0 / 360, saturation: 0.85, brightness: 1)
                } : [Color(white: 0.35), Color(white: 0.85), Color(white: 0.35)],
            center: .center, angle: .degrees(-90))
        ZStack {
            RoundedRectangle(cornerRadius: 16, style: .continuous).fill(edge)
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(Color(white: 0.97))
                .padding(7)
            dots.padding(12)
            if let colors, tinted {
                puck(
                    Color(hue: (colors.hue + 180).truncatingRemainder(dividingBy: 360) / 360,
                        saturation: 0.15 + 0.5 * colors.saturation, brightness: 0.95),
                    at: point(colors.hue + 180, colors.saturation, half), size: 18)
                puck(
                    Color(hue: colors.hue / 360, saturation: 0.75, brightness: 0.85),
                    at: point(colors.hue, colors.saturation, half), size: 26)
            } else {
                puck(.black, at: CGPoint(x: half - 30, y: half - 6), size: 18)
                puck(Color(white: 0.5), at: CGPoint(x: half + 30, y: half - 10), size: 26)
            }
        }
        .frame(width: size, height: size)
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { drag in changed(colors(at: drag.location, half), true) }
                .onEnded { drag in changed(colors(at: drag.location, half), false) }
        )
        .help("Drag to color the site; the middle gives its own colors back")
    }

    private var dots: some View {
        Canvas { context, size in
            for x in stride(from: 0, through: size.width, by: 6) {
                for y in stride(from: 0, through: size.height, by: 6) {
                    context.fill(
                        Path(ellipseIn: CGRect(x: x, y: y, width: 1, height: 1)),
                        with: .color(Color(white: 0.85)))
                }
            }
        }
    }

    private func puck(_ color: Color, at point: CGPoint, size: CGFloat) -> some View {
        Circle().fill(color)
            .frame(width: size, height: size)
            .overlay(Circle().strokeBorder(.white, lineWidth: 2.5))
            .shadow(color: .black.opacity(0.3), radius: 2, y: 1)
            .position(point)
    }

    /// Where a hue and an amount of color stand on the square, from its middle.
    private func point(_ hue: Double, _ amount: Double, _ half: CGFloat) -> CGPoint {
        let distance = min(1, max(0, amount)) * (half - 20)
        let angle = (hue - 90) * .pi / 180
        return CGPoint(x: half + cos(angle) * distance, y: half + sin(angle) * distance)
    }

    private func colors(at location: CGPoint, _ half: CGFloat) -> Boost.Colors? {
        let dx = location.x - half
        let dy = location.y - half
        let amount = min(1, hypot(dx, dy) / (half - 20))
        var colors = self.colors ?? .none
        if amount < 0.12 {
            colors.saturation = 0
        } else {
            let hue = atan2(dy, dx) * 180 / .pi + 90
            colors.hue = (hue < 0 ? hue + 360 : hue).rounded()
            colors.saturation = (amount * 100).rounded() / 100
        }
        return colors == .none ? nil : colors
    }
}

/// The advanced color controls, like Arc's: contrast, brightness and how much of the site's own
/// colors stays.
struct BoostAdvancedColors: View {
    let colors: Boost.Colors
    let changed: (Boost.Colors, Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            slider("Contrast", \.contrast, 0.5...1.5)
            slider("Brightness", \.brightness, 0.5...1.5)
            slider("Original Saturation", \.original, 0...1)
        }
        .padding(14)
        .frame(width: 210)
    }

    private func slider(
        _ title: String, _ key: WritableKeyPath<Boost.Colors, Double>, _ range: ClosedRange<Double>
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.system(size: 12, weight: .semibold))
            Slider(
                value: Binding(
                    get: { colors[keyPath: key] },
                    set: { value in
                        var colors = colors
                        colors[keyPath: key] = (value * 100).rounded() / 100
                        changed(colors, true)
                    }),
                in: range
            ) { editing in
                if !editing { changed(colors, false) }
            }
            .controlSize(.small)
        }
    }
}

/// The fonts, like Arc's: twenty "Aa" in their faces, the one picked in a grey circle.
struct BoostFontGrid: View {
    let font: String?
    let picked: (String?) -> Void

    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.fixed(22), spacing: 4), count: 5), spacing: 5) {
            ForEach(boostFonts, id: \.family) { option in
                let selected = option.family == font
                Button {
                    picked(selected ? nil : option.family)
                } label: {
                    Text("Aa")
                        .font(sample(option.family))
                        .foregroundStyle(Color(white: selected ? 0.2 : 0.45))
                        .frame(width: 22, height: 22)
                        .background(
                            Circle().fill(Color(white: 0.82).opacity(selected ? 1 : 0))
                                .frame(width: 25, height: 25))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(option.name)
            }
        }
        .padding(.vertical, 9)
        .frame(width: 145)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous).fill(.white)
                .shadow(color: .black.opacity(0.1), radius: 3, y: 1))
    }

    private func sample(_ family: String) -> Font {
        switch family {
        case "-apple-system": .system(size: 12)
        case "ui-rounded": .system(size: 12, design: .rounded)
        case "ui-serif": .system(size: 12, design: .serif)
        case "ui-monospace": .system(size: 11, design: .monospaced)
        default: .custom(family, size: family == "Zapfino" ? 8 : 12)
        }
    }
}

/// A square button of the colors' row, dark while its setting is on.
struct BoostIconButton: View {
    let symbol: String
    let label: String
    let on: Bool
    let action: () -> Void
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 14))
                .foregroundStyle(on ? Color.white : Color(white: enabled ? 0.25 : 0.65))
                .frame(width: 42, height: 36)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(on ? Color(white: 0.17) : Color(white: 0.91)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(label)
    }
}

/// Size or Case, grey, or filled with its color once it changes the site.
struct BoostWideButton: View {
    let title: String
    let fill: [Color]?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title).font(.system(size: 13, weight: .semibold))
                .foregroundStyle(fill == nil ? Color(white: 0.2) : Color.white)
                .frame(maxWidth: .infinity, minHeight: 36)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(
                            LinearGradient(
                                colors: fill ?? [Color(white: 0.91)], startPoint: .top,
                                endPoint: .bottom)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Zap or Code: its name, and a symbol or a count at its end; Zap dark once it has zapped.
struct BoostRowButton: View {
    let title: String
    let dark: Bool
    let trailing: String?
    let symbol: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Text(title).font(.system(size: 13, weight: .semibold))
                Spacer()
                if let trailing {
                    Text(trailing).font(.system(size: 13, weight: .semibold))
                } else {
                    Image(systemName: symbol).font(.system(size: 13, weight: .medium))
                }
            }
            .foregroundStyle(dark ? Color.white : Color(white: 0.2))
            .padding(.horizontal, 12)
            .frame(maxWidth: .infinity, minHeight: 36)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(dark ? Color(white: 0.17) : Color(white: 0.91)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// The Boost's code in the panel's place, like Arc's: back, CSS or JS, the code in Monaco, and
/// the selector picker and the developer tools under it; the CSS applies as it is typed, the
/// JavaScript once the page reloads.
struct BoostCodeView: View {
    @ObservedObject var model: BoostEditorModel

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                Picker("", selection: $model.tab) {
                    Text("CSS").tag(0)
                    Text("JS").tag(1)
                }
                .pickerStyle(.segmented).labelsHidden().fixedSize()
                .onChange(of: model.tab) { _, tab in model.code.show(tab: tab) }
                HStack {
                    Button { model.showCode(false) } label: {
                        HStack(spacing: 4) {
                            Image(systemName: "arrow.left").font(.system(size: 11, weight: .bold))
                            Text("Back").font(.system(size: 13, weight: .semibold))
                        }
                    }
                    .buttonStyle(.plain)
                    Spacer()
                }
            }
            .padding(.horizontal, 12)
            .frame(height: 40)
            .background(Color(white: 0.96))
            BoostMonaco(code: model.code)
            if model.tab == 1 {
                Text("This JavaScript runs on the site with the site's own rights. Add only code you trust.")
                    .font(.system(size: 11, weight: .semibold)).foregroundStyle(Color.white)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12).padding(.vertical, 7)
                    .background(Color(red: 0.93, green: 0.25, blue: 0.35))
            }
            HStack(spacing: 8) {
                BoostCodeTool(symbol: "eyedropper", label: "Pick an Element's Selector") {
                    model.action("pick")
                }
                BoostCodeTool(symbol: "hammer", label: "Developer Tools") {
                    model.app.run("dev-tools")
                }
                Spacer()
                if model.tab == 1 {
                    Button {
                        model.action("reload")
                    } label: {
                        HStack(spacing: 8) {
                            Text("Refresh to Run").font(.system(size: 12, weight: .medium))
                                .foregroundStyle(Color(white: 0.45))
                            Image(systemName: "arrow.clockwise")
                                .font(.system(size: 12, weight: .semibold))
                                .frame(width: 30, height: 30)
                                .background(RoundedRectangle(cornerRadius: 7).fill(Color(white: 0.91)))
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(8)
            .background(Color.white)
        }
        .frame(width: BoostEditorModel.codeWidth)
        .frame(maxHeight: .infinity)
    }
}

struct BoostCodeTool: View {
    let symbol: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 13))
                .frame(width: 30, height: 30)
                .background(RoundedRectangle(cornerRadius: 7).fill(Color(white: 0.91)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(label)
    }
}

/// Monaco (app/editor), which the app bundles, in a web view: its files come from the bundle
/// through a scheme of their own, which lets its workers load.
@MainActor
final class BoostCode: NSObject, WKScriptMessageHandler {
    let view: WKWebView
    private weak var model: BoostEditorModel?
    private var ready = false
    /// What to do once Monaco is ready.
    private var waiting: [String] = []

    init(model: BoostEditorModel) {
        self.model = model
        let configuration = WKWebViewConfiguration()
        let files = Bundle.main.resourceURL?.appendingPathComponent("editor") ?? URL(fileURLWithPath: "/")
        configuration.setURLSchemeHandler(EditorFiles(root: files), forURLScheme: "arbalete-editor")
        view = WKWebView(frame: .zero, configuration: configuration)
        super.init()
        configuration.userContentController.add(self, name: "boost")
        view.setValue(false, forKey: "drawsBackground")
        view.load(URLRequest(url: URL(string: "arbalete-editor://code/index.html")!))
    }

    func show(css: String, js: String, tab: Int) {
        run("boost.set('css', \(quoted(css))); boost.set('js', \(quoted(js)))")
        show(tab: tab)
    }

    func show(tab: Int) { run("boost.show('\(tab == 0 ? "css" : "js")')") }

    func insert(_ text: String) { run("boost.insert(\(quoted(text)))") }

    func globals(_ names: [String]) { run("boost.globals(\(quoted(names)))") }

    private func run(_ script: String) {
        guard ready else { return waiting.append(script) }
        view.evaluateJavaScript(script)
    }

    private func quoted(_ value: Any) -> String {
        let data = try? JSONSerialization.data(withJSONObject: [value], options: [.fragmentsAllowed])
        let array = data.flatMap { String(data: $0, encoding: .utf8) } ?? "[\"\"]"
        return "\(array)[0]"
    }

    nonisolated func userContentController(
        _ controller: WKUserContentController, didReceive message: WKScriptMessage
    ) {
        let body = message.body as? [String: Any]
        MainActor.assumeIsolated {
            if body?["ready"] as? Bool == true {
                ready = true
                waiting.forEach { view.evaluateJavaScript($0) }
                waiting = []
            } else if let kind = body?["kind"] as? String, let text = body?["text"] as? String {
                // Saved as it is typed; the JavaScript runs once the page reloads.
                if kind == "css" {
                    model?.change({ $0.css = text }, gesture: true)
                } else {
                    model?.change({ $0.js = text }, gesture: true)
                }
            }
        }
    }
}

/// Serves Monaco's files from the bundle, and nothing outside its folder.
final class EditorFiles: NSObject, WKURLSchemeHandler {
    private let root: URL

    init(root: URL) {
        self.root = root.standardizedFileURL
    }

    func webView(_ webView: WKWebView, start task: any WKURLSchemeTask) {
        guard let url = task.request.url else { return }
        let file = root.appendingPathComponent(String(url.path.dropFirst())).standardizedFileURL
        guard file.path.hasPrefix(root.path + "/"), let data = try? Data(contentsOf: file) else {
            task.didFailWithError(URLError(.fileDoesNotExist))
            return
        }
        let types = ["html": "text/html", "js": "text/javascript", "css": "text/css", "ttf": "font/ttf"]
        let headers = [
            "Content-Type": types[file.pathExtension] ?? "application/octet-stream",
            "Content-Length": String(data.count),
        ]
        if let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: headers) {
            task.didReceive(response)
        }
        task.didReceive(data)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: any WKURLSchemeTask) {}
}

/// The code's web view in the panel.
struct BoostMonaco: NSViewRepresentable {
    let code: BoostCode

    func makeNSView(context: Context) -> WKWebView { code.view }

    func updateNSView(_ view: WKWebView, context: Context) {}
}
