// The welcome flow, in a borderless window of its own in front of the main one, which the app
// opens at the first launch and from Help › Welcome to Arbalete… (show_onboarding in
// app/src/shell/onboarding.rs): a few steps, each with its own skip, over the Space's theme as the
// main window shows it, a new Space's being the logo's colors
// (docs/decisions/0007-first-launch-onboarding.md). The steps change the app through its own
// commands: the import, a Space's name, icon and theme.

import AppKit
import SwiftUI

/// The window's size, which the steps are laid out for.
private let windowSize = NSSize(width: 1000, height: 660)

/// The bar at the window's bottom, with Back and the steps' dots, which the steps stay clear of.
private let footerHeight: CGFloat = 64

/// The shortcuts the last step has the user try, by their menu items' actions, and what they do.
private let tried: [(action: String, label: String)] = [
    ("arbaleteNewTab:", "Open a page or search"),
    ("arbaleteToggleSidebar:", "Hide the sidebar"),
    ("arbaleteNextSpace:", "Go to the next Space"),
    ("arbaleteCopyUrl:", "Copy the page’s URL"),
]

@MainActor
final class OnboardingModel: ObservableObject {
    enum Step {
        case welcome, importing, imported, extensions, spaces, browser, keys
    }

    /// What an import made (send_imported in app/src/shell/onboarding.rs).
    struct Imported: Decodable {
        var spaces: [Int]
        var pages: Int
        /// The Chrome Web Store extensions found, to add.
        var extensions: [FoundExtension]
    }

    @Published var step = Step.welcome
    /// Which way the steps slide: forward, or back.
    @Published var forward = true
    @Published var sources: Sources?
    @Published var choices: [ImportChoice] = []
    @Published var importing = false
    @Published var imported: Imported?
    /// The extensions found to add.
    @Published var adding: Set<FoundExtension> = []
    /// Only Arc's profiles were imported: they bring their Spaces' colors, so no Spaces step.
    private var arcOnly = false
    /// The Space the Spaces step shows, among those it sets up.
    @Published var space: Int?
    @Published var work = false
    /// Arbalete was the default browser as the flow opened.
    private var wasDefault = false
    /// The shortcuts tried, by their rank in `tried`.
    @Published var done: Set<Int> = []
    let app: AppModel
    let picker: ThemePickerModel
    private var window: NSWindow?
    private var closing: OnboardingClosing?
    private var keys: Any?
    private var moves: Any?
    /// How far the fingers moved sideways in the trackpad gesture going on, and whether it moved a
    /// step already: a gesture moves one step at most.
    private var swipe: (distance: CGFloat, moved: Bool) = (0, false)
    private var front: NSObjectProtocol?

    init(app: AppModel) {
        self.app = app
        picker = ThemePickerModel(app: app)
    }

    /// The steps this run goes through, in order.
    var steps: [Step] {
        var steps: [Step] = [.welcome]
        if !(sources?.browsers.isEmpty ?? true) || imported != nil { steps.append(.importing) }
        if imported != nil { steps.append(.imported) }
        if !(imported?.extensions.isEmpty ?? true) { steps.append(.extensions) }
        if imported == nil || !arcOnly { steps.append(.spaces) }
        if !wasDefault { steps.append(.browser) }
        steps.append(.keys)
        return steps
    }

    /// The Spaces the Spaces step sets up: those an import made, or else the displayed one.
    var spaces: [Space] {
        let all = app.state?.spaces ?? []
        if let imported, !imported.spaces.isEmpty {
            return imported.spaces.compactMap { id in all.first { $0.id == id } }
        }
        return all.filter { $0.id == app.state?.space }
    }


    func show(_ shown: Bool) {
        guard shown else { return hide() }
        if let window {
            window.makeKeyAndOrderFront(nil)
            return
        }
        (imported, arcOnly, space, work, done, importing) = (nil, false, nil, false, [], false)
        wasDefault = isDefaultBrowser()
        step = .welcome
        // The window shows the displayed Space's theme from the start, as the main window would.
        if let shown = spaces.first { pick(shown) }
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: windowSize),
            styleMask: [.titled, .closable, .fullSizeContentView], backing: .buffered, defer: false)
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        // The desktop shows through it, under a Space's theme.
        window.isOpaque = false
        window.backgroundColor = .clear
        window.isReleasedWhenClosed = false
        window.tabbingMode = .disallowed
        window.standardWindowButton(.miniaturizeButton)?.isHidden = true
        window.standardWindowButton(.zoomButton)?.isHidden = true
        let host = NSHostingView(rootView: OnboardingView(model: self, app: app, picker: picker))
        // The steps reach under the title bar, which shows only the close button.
        host.safeAreaRegions = []
        window.contentView = host
        // Closed before its end, it shows again at the next launch.
        let closing = OnboardingClosing { [weak self] in
            self?.stopWatching()
            self?.window = nil
            self?.app.run("close-onboarding")
        }
        window.delegate = closing
        self.closing = closing
        // Back from System Settings, Safari's data may be readable.
        front = NotificationCenter.default.addObserver(
            forName: NSWindow.didBecomeKeyNotification, object: window, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.app.run("onboarding-ready") }
        }
        keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, self.tries(event) else { return event }
            return nil
        }
        // Like the pages: the mouse's back and forward buttons, and swipes, move between steps.
        moves = NSEvent.addLocalMonitorForEvents(matching: [.otherMouseDown, .swipe, .scrollWheel]) {
            [weak self] event in
            guard let self, event.window === self.window, !self.importing,
                let command = self.swiped(event) ?? historyCommand(event)
            else { return event }
            // Forward never ends the flow: only its last button does.
            if command == "back" {
                self.back()
            } else if self.step != self.steps.last {
                self.next()
            }
            return event.type == .scrollWheel ? event : nil
        }
        window.center()
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        self.window = window
    }

    /// Fades the window out over the main one, which the browser takes over.
    private func hide() {
        guard let window else { return }
        stopWatching()
        self.window = nil
        window.delegate = nil
        let still = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        NSAnimationContext.runAnimationGroup { context in
            context.duration = still ? 0.2 : 0.6
            window.animator().alphaValue = 0
        } completionHandler: {
            MainActor.assumeIsolated { window.close() }
        }
    }

    private func stopWatching() {
        if let keys { NSEvent.removeMonitor(keys) }
        if let moves { NSEvent.removeMonitor(moves) }
        if let front { NotificationCenter.default.removeObserver(front) }
        (keys, moves, front) = (nil, nil, nil)
    }

    func handle(_ name: String, _ detail: Data) {
        switch name {
        case "arbalete:onboarding":
            guard let sources = try? JSONDecoder().decode(Sources.self, from: detail) else { return }
            let before = choices
            self.sources = sources
            // Like the import assistant, Arc is ticked at first; ticks stay as the list comes again.
            choices = sources.browsers.enumerated().flatMap { b, browser in
                browser.profiles.enumerated().map { p, profile in
                    let kept = before.first { $0.browser == b && $0.profile == p }
                    let readable = !profile.needsFullDiskAccess
                    return ImportChoice(
                        browser: b, profile: p,
                        checked: readable && (kept?.checked ?? browser.arc), into: kept?.into,
                        bar: "favorites")
                }
            }
        case "arbalete:imported":
            guard let imported = try? JSONDecoder().decode(Imported.self, from: detail) else { return }
            self.imported = imported
            adding = Set(imported.extensions)
            // Arc's Spaces skip the Spaces step: the first one's theme shows from the next.
            if let first = spaces.first { pick(first) }
            next()
            // The import step slides away still showing the import running, not its list again.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                MainActor.assumeIsolated { self?.importing = false }
            }
        default: break
        }
    }

    func runImport() {
        let picked = choices.filter(\.checked)
        // The step still shows as it slides away after a quick import: a click then is not another.
        guard !importing, step == .importing, !picked.isEmpty, let sources else { return }
        arcOnly = picked.allSatisfy { sources.browsers[$0.browser].arc }
        importing = true
        runAfterShowing { $0.app.run("import", jsonObject(picked)) }
    }

    /// Adds the extensions picked once the flow ends, in the main window, where Chromium asks to
    /// confirm each one; moves on meanwhile.
    func addExtensions() {
        let picked = imported?.extensions.filter(adding.contains) ?? []
        if !picked.isEmpty { app.run("install-extensions", jsonObject(picked)) }
        next()
    }

    /// Shows a Space in the Spaces step, its theme in the picker.
    func pick(_ space: Space) {
        self.space = space.id
        struct Shown: Encodable {
            var space: Int
            var theme: Theme
        }
        guard let detail = try? JSONEncoder().encode(Shown(space: space.id, theme: space.theme)) else { return }
        picker.handle("arbalete:theme", detail)
    }

    func next() {
        let steps = steps
        if step == .spaces, work {
            app.run("add-work-space")
            work = false
        }
        guard let index = steps.firstIndex(of: step), index + 1 < steps.count else {
            return finish()
        }
        go(to: steps[index + 1], forward: true)
    }

    func back() {
        let steps = steps
        guard let index = steps.firstIndex(of: step), index > 0 else { return }
        go(to: steps[index - 1], forward: false)
    }

    /// Ends the flow: it no longer shows at launch.
    func finish() {
        app.run("finish-onboarding")
    }

    /// Goes to a step from its dot, either way.
    func jump(to step: Step) {
        let steps = steps
        guard step != self.step, let to = steps.firstIndex(of: step),
            let from = steps.firstIndex(of: self.step)
        else { return }
        go(to: step, forward: to > from)
    }

    private func go(to step: Step, forward: Bool) {
        self.forward = forward
        if step == .spaces, let first = spaces.first(where: { $0.id == space }) ?? spaces.first {
            pick(first)
        }
        let still = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        withAnimation(still ? .easeInOut(duration: 0.2) : .timingCurve(0.2, 0.8, 0.2, 1, duration: 0.45)) {
            self.step = step
        }
    }

    /// Each shortcut tried, as its menu item has it: the settings may have changed its keys.
    var shortcuts: [Keys] {
        tried.map { tried in
            guard let item = menuItem(NSApp.mainMenu, action: tried.action) else {
                return Keys(key: "", modifiers: 0)
            }
            return Keys(key: item.keyEquivalent, modifiers: item.keyEquivalentModifierMask.rawValue)
        }
    }

    /// A two-finger swipe across the trackpad, like Safari's between pages: to the right goes back,
    /// once the fingers moved far enough sideways rather than up or down. Momentum is ignored.
    private func swiped(_ event: NSEvent) -> String? {
        guard event.type == .scrollWheel, event.hasPreciseScrollingDeltas, event.momentumPhase == [] else {
            return nil
        }
        if event.phase == .began { swipe = (0, false) }
        guard event.phase == .changed, !swipe.moved, abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY)
        else { return nil }
        // The fingers' way, whichever way the system scrolls content.
        swipe.distance += event.isDirectionInvertedFromDevice ? event.scrollingDeltaX : -event.scrollingDeltaX
        guard abs(swipe.distance) > 80 else { return nil }
        swipe.moved = true
        return swipe.distance > 0 ? "back" : "forward"
    }

    /// On the last step, a shortcut tried lights its tile and does nothing else.
    private func tries(_ event: NSEvent) -> Bool {
        guard step == .keys, event.window === window else { return false }
        let flags = event.modifierFlags.intersection([.command, .shift, .option, .control])
        let pressed = Keys(key: event.charactersIgnoringModifiers ?? "", modifiers: flags.rawValue)
        guard let index = shortcuts.firstIndex(where: { $0.same(pressed) }) else { return false }
        withAnimation(.spring(duration: 0.2)) { _ = done.insert(index) }
        return true
    }
}

extension ObservableObject {
    /// Runs `action` once the change just made has shown: the import's last part, opening the
    /// profiles imported, holds the main thread for a moment, which would freeze the window before
    /// it shows the import running.
    func runAfterShowing(_ action: @escaping @MainActor (Self) -> Void) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
            MainActor.assumeIsolated {
                if let self { action(self) }
            }
        }
    }
}

/// The menu item that runs `action`, in `menu` or its submenus.
@MainActor private func menuItem(_ menu: NSMenu?, action: String) -> NSMenuItem? {
    for item in menu?.items ?? [] {
        if item.action.map(NSStringFromSelector) == action { return item }
        if let found = menuItem(item.submenu, action: action) { return found }
    }
    return nil
}

/// Tells when the user closes the window, as its delegate.
private final class OnboardingClosing: NSObject, NSWindowDelegate {
    let closed: () -> Void

    init(closed: @escaping () -> Void) {
        self.closed = closed
    }

    func windowWillClose(_ notification: Notification) { closed() }
}

struct OnboardingView: View {
    @ObservedObject var model: OnboardingModel
    /// The Spaces' names, icons and themes, as the app has them.
    @ObservedObject var app: AppModel
    /// The theme as it is being picked, which the window shows at once.
    @ObservedObject var picker: ThemePickerModel
    @Environment(\.accessibilityReduceMotion) private var still

    var body: some View {
        let steps = model.steps
        ZStack {
            SpaceBackdrop(theme: picker.theme)
            Group {
                switch model.step {
                case .welcome: WelcomeStep(model: model)
                case .importing: ImportStep(model: model)
                case .imported: ImportedStep(model: model)
                case .extensions: ExtensionsStep(model: model)
                case .spaces: SpacesStep(model: model, app: app)
                case .browser: BrowserStep(model: model)
                case .keys: KeysStep(model: model)
                }
            }
            .id(model.step)
            .transition(stepTransition)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(.top, 28)
            .padding(.bottom, footerHeight)
            // The bar the steps stay clear of: Back at its left, the steps' dots in its middle.
            ZStack {
                StepDots(count: steps.count, current: steps.firstIndex(of: model.step) ?? 0) { index in
                    if !model.importing { model.jump(to: steps[index]) }
                }
                if model.step != steps.first, !model.importing {
                    Button { model.back() } label: {
                        Label("Back", systemImage: "chevron.left")
                            .font(.system(size: 13, weight: .medium))
                            .padding(.horizontal, 12)
                            .frame(height: 30)
                            .background(Capsule().fill(.white.opacity(0.12)))
                            .contentShape(Capsule())
                    }
                    .buttonStyle(.plain)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .padding(.horizontal, 24)
            .frame(height: footerHeight)
            .frame(maxHeight: .infinity, alignment: .bottom)
        }
        .foregroundStyle(.white)
        .environment(\.colorScheme, .dark)
        .frame(width: windowSize.width, height: windowSize.height)
        .ignoresSafeArea()
    }

    /// Steps slide in from the way the flow goes and fade, or only fade with Reduce Motion.
    private var stepTransition: AnyTransition {
        guard !still else { return .opacity }
        let shift: CGFloat = model.forward ? 24 : -24
        return .asymmetric(
            insertion: .opacity.combined(with: .offset(x: shift)),
            removal: .opacity.combined(with: .offset(x: -shift)))
    }
}

/// A Space's theme as the main window shows it (Backdrop in app/macos/Window.swift): the desktop
/// blurred through the window, under the theme's colors, intensity and grain.
private struct SpaceBackdrop: View {
    let theme: Theme

    var body: some View {
        ZStack {
            DesktopBlur(light: !theme.colors.isEmpty || !systemDark())
            Tint(theme: theme)
        }
    }
}

private struct DesktopBlur: NSViewRepresentable {
    let light: Bool

    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.blendingMode = .behindWindow
        view.state = .active
        view.isEmphasized = true
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context: Context) {
        view.material = light ? .light : .dark
    }
}

/// Where the flow is, at the window's bottom: a dot per step, the current one stretched.
private struct StepDots: View {
    let count: Int
    let current: Int
    /// Goes to a step, by its rank.
    let select: (Int) -> Void

    var body: some View {
        HStack(spacing: 0) {
            ForEach(0..<count, id: \.self) { index in
                Button { select(index) } label: {
                    Capsule()
                        .fill(.white.opacity(index == current ? 1 : 0.3))
                        .frame(width: index == current ? 24 : 7, height: 7)
                        // A larger target than the dot.
                        .padding(.horizontal, 4)
                        .frame(height: 30)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Step \(index + 1) of \(count)")
                .accessibilityAddTraits(index == current ? .isSelected : [])
            }
        }
    }
}

/// The flow's buttons: white for the step's main one, which Return presses, a light glass for its
/// skip, which Escape presses; the key shows beside the title.
private struct OnboardingButton: View {
    let title: String
    var primary = true
    var enabled = true
    /// Return presses it, else Escape: the main button's, unless set apart.
    var returns: Bool?
    let action: () -> Void

    var body: some View {
        let returns = returns ?? primary
        Button(action: action) {
            HStack(spacing: 10) {
                Text(title)
                Text(returns ? "↩" : "⎋").font(.system(size: 13)).opacity(0.5)
            }
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(primary ? color("#16112a") : .white)
            .padding(.horizontal, 20)
            .frame(height: 44)
            .background(
                RoundedRectangle(cornerRadius: 10).fill(primary ? Color.white : Color.white.opacity(0.14)))
            .opacity(enabled ? 1 : 0.45)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .keyboardShortcut(returns ? .defaultAction : .cancelAction)
    }
}

/// A step's title and the line under it.
private struct StepHeading: View {
    let title: String
    let text: String
    var centered = false

    var body: some View {
        VStack(alignment: centered ? .center : .leading, spacing: 12) {
            Text(title).font(.system(size: 32, weight: .bold)).tracking(-0.6)
                .accessibilityAddTraits(.isHeader)
            Text(text).font(.system(size: 15)).opacity(0.8).lineSpacing(3)
        }
        .multilineTextAlignment(centered ? .center : .leading)
        .frame(maxWidth: .infinity, alignment: centered ? .center : .leading)
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// A frosted card, like the flow's lists.
private struct Glass: ViewModifier {
    var edge = Color.white.opacity(0.16)

    func body(content: Content) -> some View {
        content
            .background(RoundedRectangle(cornerRadius: 13).fill(.white.opacity(0.09)))
            .overlay(RoundedRectangle(cornerRadius: 13).strokeBorder(edge))
    }
}

/// A box ticked or not.
private struct TickBox: View {
    let ticked: Bool

    var body: some View {
        RoundedRectangle(cornerRadius: 5)
            .fill(ticked ? Color.white : .clear)
            .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(.white.opacity(ticked ? 1 : 0.55), lineWidth: 2))
            .overlay(
                Image(systemName: "checkmark").font(.system(size: 10, weight: .heavy))
                    .foregroundStyle(color("#16112a")).opacity(ticked ? 1 : 0)
            )
            .frame(width: 18, height: 18)
    }
}

/// The app's icon and name, written letter by letter, then a line and the way in.
private struct WelcomeStep: View {
    @ObservedObject var model: OnboardingModel
    @Environment(\.accessibilityReduceMotion) private var still
    private let shown = State(initialValue: false)
    private let name = Array("Arbalete")

    var body: some View {
        let shown = shown.wrappedValue
        VStack(spacing: 22) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 96, height: 96)
                .shadow(color: .black.opacity(0.35), radius: 15, y: 18)
                .opacity(shown ? 1 : 0)
                .animation(letterAnimation, value: shown)
            HStack(spacing: 0) {
                ForEach(Array(name.enumerated()), id: \.offset) { index, letter in
                    Text(String(letter))
                        .opacity(shown ? 1 : 0)
                        .offset(y: shown ? 0 : 18)
                        .blur(radius: shown ? 0 : 8)
                        .animation(letterAnimation.delay(still ? 0 : 0.2 + Double(index) * 0.07), value: shown)
                }
            }
            .font(.system(size: 72, weight: .bold))
            .tracking(-2.9)
            .accessibilityElement()
            .accessibilityLabel("Arbalete")
            .accessibilityAddTraits(.isHeader)
            VStack(spacing: 18) {
                Text("Fewer tabs to babysit, more web to enjoy.\nGive us a minute and you’re off.")
                    .font(.system(size: 17))
                    .multilineTextAlignment(.center)
                    .opacity(0.82)
                OnboardingButton(title: "Let’s go") { model.next() }
                Button("Skip setup") { model.finish() }
                    .buttonStyle(.plain)
                    .font(.system(size: 14, weight: .medium))
                    .opacity(0.68)
                    .keyboardShortcut(.cancelAction)
            }
            .opacity(shown ? 1 : 0)
            .blur(radius: shown ? 0 : 8)
            .offset(y: shown ? 0 : 8)
            .animation(
                still ? .easeOut(duration: 0.2) : .timingCurve(0.2, 0.8, 0.2, 1, duration: 0.6).delay(0.85),
                value: shown)
        }
        .frame(width: 520)
        .onAppear { self.shown.wrappedValue = true }
    }

    private var letterAnimation: Animation {
        still ? .easeOut(duration: 0.2) : .timingCurve(0.2, 0.8, 0.2, 1, duration: 0.5)
    }
}

/// The browsers' profiles found, to tick: each becomes its own Space with its own profile, or
/// merges into one of Arbalete's when the flow was shown again by hand.
private struct ImportStep: View {
    @ObservedObject var model: OnboardingModel

    var body: some View {
        let ticked = model.choices.filter(\.checked).count
        VStack(alignment: .leading, spacing: 16) {
            StepHeading(
                title: model.importing ? "Moving in…" : "Bring your old browser along",
                text: "Tick as many as you like: each one becomes its own Space, with its own profile. Your old browser keeps its copy.")
            if model.importing {
                ProgressView().progressViewStyle(.linear).tint(.white)
            } else {
                rows
                if model.locked {
                    FullDiskAccess(model: model)
                }
                HStack(spacing: 12) {
                    OnboardingButton(title: ticked > 1 ? "Import \(ticked)" : "Import", enabled: ticked > 0) {
                        model.runImport()
                    }
                    OnboardingButton(title: "Not now", primary: false) { model.next() }
                }
            }
        }
        .frame(width: 480)
    }

    /// The list, scrolling past a height that keeps the step within the window.
    private var rows: some View {
        let list = profiles.padding(5)
        return ViewThatFits(in: .vertical) {
            list
            ScrollView { list }.frame(height: model.locked ? 250 : 330)
        }
        .frame(maxHeight: model.locked ? 250 : 330)
        .modifier(Glass())
    }

    private var profiles: some View {
        VStack(spacing: 4) {
            ForEach(Array(model.choices.enumerated()), id: \.offset) { index, choice in
                if let sources = model.sources {
                    let browser = sources.browsers[choice.browser]
                    let profile = browser.profiles[choice.profile]
                    BrowserProfileRow(
                        name: browser.profiles.count > 1 ? "\(browser.name) · \(profile.name)" : browser.name,
                        locked: profile.needsFullDiskAccess, targets: sources.profiles,
                        choice: Binding(get: { model.choices[index] }, set: { model.choices[index] = $0 }))
                }
            }
        }
    }
}

private extension OnboardingModel {
    /// A browser profile cannot be read until Arbalete has Full Disk Access.
    var locked: Bool {
        sources?.browsers.contains { $0.profiles.contains(where: \.needsFullDiskAccess) } ?? false
    }
}

/// A browser profile to tick; a locked one shows a padlock instead.
private struct BrowserProfileRow: View {
    let name: String
    let locked: Bool
    let targets: [Sources.Target]
    @Binding var choice: ImportChoice

    var body: some View {
        HStack(spacing: 12) {
            if locked {
                Image(systemName: "lock.fill").font(.system(size: 12)).foregroundStyle(color("#ffd38a"))
                    .frame(width: 18)
            } else {
                TickBox(ticked: choice.checked)
            }
            Text(String(name.prefix(1)))
                .font(.system(size: 12, weight: .bold))
                .frame(width: 28, height: 28)
                .background(RoundedRectangle(cornerRadius: 7).fill(.white.opacity(0.2)))
            Text(name).font(.system(size: 14)).opacity(locked ? 0.75 : 1)
            Spacer()
            if locked {
                Text("Needs Full Disk Access").font(.system(size: 12)).foregroundStyle(color("#ffd38a"))
            } else if !targets.isEmpty {
                // Shown again by hand, an import can merge into a profile Arbalete has.
                Picker("", selection: $choice.into) {
                    Text("New profile").tag(Int?.none)
                    ForEach(targets) { Text("Into \($0.name)").tag(Int?.some($0.id)) }
                }
                .labelsHidden()
                .fixedSize()
            }
        }
        .padding(.horizontal, 12)
        .frame(minHeight: 44)
        .background(RoundedRectangle(cornerRadius: 8).fill(.white.opacity(choice.checked ? 0.2 : 0)))
        .contentShape(Rectangle())
        .onTapGesture { if !locked { choice.checked.toggle() } }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(choice.checked ? [.isButton, .isSelected] : .isButton)
    }
}

/// How to unlock Safari's data: macOS keeps it private until Arbalete has Full Disk Access.
private struct FullDiskAccess: View {
    @ObservedObject var model: OnboardingModel

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "lock").font(.system(size: 15, weight: .semibold))
            VStack(alignment: .leading, spacing: 2) {
                Text("Safari keeps its data under lock and key").font(.system(size: 14, weight: .semibold))
                Text("Turn on Arbalete in Full Disk Access, then come back: Safari unlocks by itself.")
                    .font(.system(size: 12))
                    .opacity(0.86)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            Button("Open System Settings") { model.app.run("open-full-disk-access") }
                .buttonStyle(.plain)
                .font(.system(size: 13, weight: .semibold))
                .padding(.horizontal, 14)
                .frame(height: 34)
                .background(RoundedRectangle(cornerRadius: 9).fill(.white.opacity(0.16)))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(Glass(edge: color("#ffd38a").opacity(0.45)))
    }
}

/// What came in.
private struct ImportedStep: View {
    @ObservedObject var model: OnboardingModel

    var body: some View {
        let count = model.imported?.spaces.count ?? 0
        let arc = !model.steps.contains(.spaces)
        VStack(alignment: .leading, spacing: 20) {
            StepHeading(
                title: "All moved in",
                text: arc
                    ? "Your Spaces came over from Arc, colors and all."
                    : "Each profile has its own Space now, and keeps its own logins, cookies and history. Next, give them a face.")
            HStack(spacing: 10) {
                fact(count, count == 1 ? "Space" : "Spaces")
                fact(model.imported?.pages ?? 0, "pages of history")
            }
            OnboardingButton(title: "Continue") { model.next() }
        }
        .frame(width: 480)
    }

    private func fact(_ value: Int, _ label: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value.formatted()).font(.system(size: 28, weight: .bold)).monospacedDigit()
            Text(label).font(.system(size: 13)).opacity(0.72)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(Glass())
    }
}

/// The extensions the browsers imported had, from the Chrome Web Store, to tick and add.
private struct ExtensionsStep: View {
    @ObservedObject var model: OnboardingModel

    var body: some View {
        let found = model.imported?.extensions ?? []
        let ticked = model.adding.count
        VStack(alignment: .leading, spacing: 16) {
            StepHeading(
                title: "Bring your extensions",
                text: "Once you’re in, they come fresh from the Chrome Web Store, one at a time: say yes to each as it asks.")
            let list = VStack(spacing: 4) {
                ForEach(found, id: \.self) { row($0) }
            }
            .padding(5)
            ViewThatFits(in: .vertical) {
                list
                ScrollView { list }.frame(height: 330)
            }
            .frame(maxHeight: 330)
            .modifier(Glass())
            HStack(spacing: 12) {
                OnboardingButton(title: ticked > 1 ? "Add \(ticked)" : "Add", enabled: ticked > 0) {
                    model.addExtensions()
                }
                OnboardingButton(title: "Not now", primary: false) { model.next() }
            }
        }
        .frame(width: 480)
    }

    private func row(_ found: FoundExtension) -> some View {
        let ticked = model.adding.contains(found)
        return HStack(spacing: 12) {
            TickBox(ticked: ticked)
            FoundExtensionIcon(path: found.icon).frame(width: 24, height: 24)
            Text(found.name).font(.system(size: 14)).lineLimit(1)
            Spacer()
        }
        .padding(.horizontal, 12)
        .frame(minHeight: 44)
        .background(RoundedRectangle(cornerRadius: 8).fill(.white.opacity(ticked ? 0.2 : 0)))
        .contentShape(Rectangle())
        .onTapGesture {
            if ticked { model.adding.remove(found) } else { model.adding.insert(found) }
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(ticked ? [.isButton, .isSelected] : .isButton)
    }
}

/// Names, icons and colors for the Spaces: those an import made, one tab each, or the first Space,
/// with a Work Space to add in a profile of its own.
private struct SpacesStep: View {
    @ObservedObject var model: OnboardingModel
    @ObservedObject var app: AppModel
    private let choosing = State(initialValue: false)
    private let name = State(initialValue: "")

    var body: some View {
        let spaces = model.spaces
        let shown = spaces.first { $0.id == model.space } ?? spaces.first
        let imported = spaces.count > 1 || model.imported != nil
        HStack(spacing: 40) {
            VStack(alignment: .leading, spacing: 16) {
                StepHeading(
                    title: imported ? "Make your Spaces yours" : "Make your first Space",
                    text: imported
                        ? "Pick one, then name it, give it an icon and paint the walls."
                        : "A Space is a room for one part of your life. Name it, pick an icon, paint the walls.")
                if spaces.count > 1 {
                    tabs(spaces)
                }
                if let shown {
                    HStack(spacing: 10) {
                        Button { choosing.wrappedValue = true } label: {
                            Group {
                                if let emoji = shown.emoji {
                                    SpaceGlyph(icon: emoji, size: 22)
                                } else {
                                    Image(systemName: "plus").font(.system(size: 15, weight: .semibold))
                                        .opacity(0.7)
                                }
                            }
                                .frame(width: 44, height: 44)
                                .background(RoundedRectangle(cornerRadius: 10).fill(.black.opacity(0.22)))
                        }
                        .buttonStyle(.plain)
                        .help("Choose an Icon")
                        .popover(isPresented: choosing.projectedValue, arrowEdge: .bottom) {
                            EmojiPicker(model: app, space: shown.id)
                        }
                        TextField("Name", text: name.projectedValue)
                            .textFieldStyle(.plain)
                            .font(.system(size: 16, weight: .semibold))
                            .padding(.horizontal, 14)
                            .frame(height: 44)
                            .background(RoundedRectangle(cornerRadius: 10).fill(.black.opacity(0.22)))
                            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.white.opacity(0.3)))
                            .onSubmit { rename(shown) }
                    }
                    .onAppear { name.wrappedValue = shown.title }
                    .onChange(of: shown.id) { name.wrappedValue = shown.title }
                }
                // Shown again by hand, not once a Work profile exists.
                if !imported, !(model.sources?.profiles.contains { $0.name == "Work" } ?? false) {
                    workBox
                }
                HStack(spacing: 12) {
                    OnboardingButton(title: "Continue") {
                        if let shown { rename(shown) }
                        model.next()
                    }
                    OnboardingButton(title: "Keep defaults", primary: false) {
                        model.work = false
                        model.next()
                    }
                }
            }
            .frame(width: 400)
            ThemePickerView(picker: model.picker)
                .frame(width: 365, height: 520)
                .background(PanelBlur())
                .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(.white.opacity(0.12)))
                .clipShape(RoundedRectangle(cornerRadius: 16))
        }
    }

    private func tabs(_ spaces: [Space]) -> some View {
        HStack(spacing: 6) {
            ForEach(spaces) { space in
                let selected = space.id == model.space
                Button {
                    if let shown = spaces.first(where: { $0.id == model.space }) { rename(shown) }
                    model.pick(space)
                } label: {
                    HStack(spacing: 8) {
                        Circle().fill(color(space.theme.colors.first ?? "#ffffff")).frame(width: 10, height: 10)
                        SpaceGlyph(icon: space.emoji ?? "", size: 14)
                        Text(space.title).lineLimit(1)
                    }
                    .font(.system(size: 14, weight: .semibold))
                    .padding(.horizontal, 12)
                    .frame(maxWidth: .infinity, minHeight: 40)
                    .background(RoundedRectangle(cornerRadius: 9).fill(.white.opacity(selected ? 0.22 : 0)))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? .isSelected : [])
            }
        }
        .padding(4)
        .background(RoundedRectangle(cornerRadius: 12).fill(.black.opacity(0.22)))
    }

    private var workBox: some View {
        HStack(alignment: .top, spacing: 12) {
            TickBox(ticked: model.work)
            VStack(alignment: .leading, spacing: 2) {
                Text("Also make a Work Space").font(.system(size: 14, weight: .semibold))
                Text("It gets its own profile: work logins, cookies and history stay apart from personal ones.")
                    .font(.system(size: 13)).opacity(0.74)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .modifier(Glass())
        .contentShape(Rectangle())
        .onTapGesture { model.work.toggle() }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(model.work ? [.isButton, .isSelected] : .isButton)
    }

    private func rename(_ space: Space) {
        let title = name.wrappedValue.trimmingCharacters(in: .whitespaces)
        if !title.isEmpty, title != space.title { app.run("rename-space", space.id, title) }
    }
}

/// The theme picker's frosted glass, as over the main window: the colors behind show through.
private struct PanelBlur: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .hudWindow
        view.blendingMode = .withinWindow
        view.state = .active
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context: Context) {}
}

/// Making Arbalete the default browser: macOS asks to confirm.
private struct BrowserStep: View {
    @ObservedObject var model: OnboardingModel

    var body: some View {
        VStack(spacing: 20) {
            Image(nsImage: NSApp.applicationIconImage)
                .resizable()
                .frame(width: 96, height: 96)
                .shadow(color: .black.opacity(0.35), radius: 15, y: 18)
            StepHeading(
                title: "Make Arbalete your everyday browser?",
                text: "Links from Mail, Messages and everywhere else will open right here. You can change it any time in Settings.",
                centered: true)
            HStack(spacing: 12) {
                OnboardingButton(title: "Set as default") {
                    makeDefaultBrowser()
                    model.next()
                }
                OnboardingButton(title: "Not now", primary: false) { model.next() }
            }
        }
        .frame(width: 520)
    }
}

/// Four shortcuts to try: each tile lights up as its keys are pressed, or as it is clicked.
private struct KeysStep: View {
    @ObservedObject var model: OnboardingModel

    var body: some View {
        let all = model.done.count == tried.count
        let keys = model.shortcuts
        VStack(spacing: 20) {
            StepHeading(
                title: all ? "You’re a natural" : "Four keys to rule them all",
                text: all
                    ? "That’s the whole kit. The others are in Settings › Shortcuts."
                    : "Try each one now. It lights up when you press it.",
                centered: true)
            LazyVGrid(columns: [GridItem(.flexible(), spacing: 12), GridItem(.flexible())], spacing: 12) {
                ForEach(Array(tried.enumerated()), id: \.offset) { index, shortcut in
                    tile(keys[index], shortcut.label, done: model.done.contains(index)) {
                        withAnimation(.spring(duration: 0.2)) { _ = model.done.insert(index) }
                    }
                }
            }
            HStack(spacing: 14) {
                OnboardingButton(title: "Start browsing", primary: all, returns: true) { model.finish() }
                if !all {
                    Text("\(model.done.count) of \(tried.count) tried").font(.system(size: 13)).opacity(0.72)
                        .monospacedDigit()
                }
            }
        }
        .frame(width: 560)
    }

    private func tile(_ keys: Keys, _ label: String, done: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 6) {
                    ForEach(Array(keys.tiles.enumerated()), id: \.offset) { _, glyph in
                        Text(glyph)
                            .font(.system(size: 17, weight: .medium))
                            .foregroundStyle(done ? color("#16112a") : .white)
                            .frame(minWidth: 40, minHeight: 40)
                            .padding(.horizontal, 4)
                            .background(RoundedRectangle(cornerRadius: 9).fill(done ? Color.white : .white.opacity(0.12)))
                            .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(.white.opacity(done ? 1 : 0.22)))
                    }
                }
                Text(label).font(.system(size: 14)).opacity(0.85)
            }
            .padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 13).fill(.white.opacity(done ? 0.24 : 0.09)))
            .overlay(RoundedRectangle(cornerRadius: 13).strokeBorder(.white.opacity(done ? 0.55 : 0.16)))
            .overlay(alignment: .topTrailing) {
                if done {
                    Image(systemName: "checkmark")
                        .font(.system(size: 11, weight: .heavy))
                        .foregroundStyle(color("#16112a"))
                        .frame(width: 22, height: 22)
                        .background(Circle().fill(.white))
                        .padding(14)
                        .transition(.scale.combined(with: .opacity))
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(label), \(keys.glyphs)")
        .accessibilityAddTraits(done ? .isSelected : [])
    }
}

@MainActor var onboardingModel: OnboardingModel?

/// Opens the welcome flow's window, or closes it once the flow ended.
@_cdecl("arbalete_ui_onboarding")
public func showOnboarding(_ shown: Bool) {
    MainActor.assumeIsolated { onboardingModel?.show(shown) }
}
