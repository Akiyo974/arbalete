// The app's windows, which the interface makes and keeps: the app knows each by an id, places
// its pages and panels in it, and learns as it resizes, comes forward or closes
// (app/src/native.rs, Window).

import AppKit

/// A main window, the delegate of its NSWindow, telling the app what happens to it.
@MainActor
private final class MainWindow: NSObject, NSWindowDelegate {
    let id: UInt32
    let window: NSWindow
    /// Its frame before it entered full screen, which it gets back leaving it.
    var normalFrame: NSRect?
    /// Opened full screen, it enters full screen once shown.
    var opensFullScreen = false
    private var observers: [NSObjectProtocol] = []

    init(id: UInt32, frame: NSRect?, minSize: NSSize) {
        self.id = id
        // Like Arc's, no title bar: the window's buttons sit in the sidebar, over its content.
        window = NSWindow(
            contentRect: NSRect(origin: .zero, size: frame?.size ?? NSSize(width: 1280, height: 820)),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered, defer: false)
        super.init()
        window.title = "Arbalete"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.minSize = minSize
        window.tabbingMode = .disallowed
        // Its margins around the card move it, as a title bar would.
        window.contentView = WindowDrag()
        if let frame { window.setFrame(frame, display: false) } else { window.center() }
        window.delegate = self
        observers = [
            NSWindow.didResizeNotification, NSWindow.didEndLiveResizeNotification,
            NSWindow.didEnterFullScreenNotification, NSWindow.didExitFullScreenNotification,
        ].map {
            NotificationCenter.default.addObserver(forName: $0, object: window, queue: .main) {
                [id] _ in
                MainActor.assumeIsolated { arbalete_window_resized(id) }
            }
        }
        // Main, rather than key: a panel above the page takes the keyboard, the window staying in
        // front.
        for (name, active) in [
            (NSWindow.didBecomeMainNotification, true), (NSWindow.didResignMainNotification, false),
        ] {
            observers.append(
                NotificationCenter.default.addObserver(forName: name, object: window, queue: .main) {
                    [id] _ in
                    MainActor.assumeIsolated { arbalete_window_activated(id, active) }
                })
        }
    }

    /// Asks the app, which may keep it open while its pages close, then closes it.
    func requestClose() {
        if windowShouldClose(window) { window.close() }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool { arbalete_window_should_close(id) }

    func windowWillEnterFullScreen(_ notification: Notification) { normalFrame = window.frame }

    func windowDidExitFullScreen(_ notification: Notification) { normalFrame = nil }

    func windowWillClose(_ notification: Notification) {
        observers.forEach(NotificationCenter.default.removeObserver)
        observers = []
        // After AppKit is done with it.
        DispatchQueue.main.async { [id] in
            mainWindows[id] = nil
            arbalete_window_closed(id)
        }
    }
}

/// The main windows, by id.
@MainActor private var mainWindows: [UInt32: MainWindow] = [:]

/// Makes the window `id`, with its interface, hidden until shown: placed, at `x`, `y`, `width`
/// and `height` from the top left corner of the screen `screen`, a display's UUID, and full
/// screen if `fullScreen`; else centered.
@_cdecl("arbalete_ui_open_window")
public func openWindow(
    _ id: UInt32, _ placed: Bool, _ screen: UnsafePointer<CChar>, _ x: Double, _ y: Double,
    _ width: Double, _ height: Double, _ fullScreen: Bool, _ minWidth: Double,
    _ minHeight: Double, _ send: Send
) {
    MainActor.assumeIsolated {
        let frame =
            placed
            ? onScreen(String(cString: screen), NSRect(x: x, y: y, width: width, height: height))
            : nil
        let main = MainWindow(
            id: id, frame: frame, minSize: NSSize(width: minWidth, height: minHeight))
        main.opensFullScreen = placed && fullScreen
        mainWindows[id] = main
        guard let content = main.window.contentView else { return }
        let app = AppModel(window: id, send: send)
        nativeWindows[id] = NativeWindow(window: main.window, content: content, app: app)
        if settingsModel == nil { settingsModel = SettingsModel(app: app) }
        if onboardingModel == nil { onboardingModel = OnboardingModel(app: app) }
    }
}

/// The UUID of the display showing `screen`, which outlives reconnecting it, unlike its number.
@MainActor private func uuid(of screen: NSScreen) -> String? {
    guard
        let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber,
        let uuid = CGDisplayCreateUUIDFromDisplayID(number.uint32Value)?.takeRetainedValue()
    else { return nil }
    return CFUUIDCreateString(nil, uuid) as String
}

/// The frame `rect`, from the top left corner of the screen `screen`, in AppKit's screen
/// coordinates: on that screen, else on the main one, which has the menu bar, at the same place;
/// within the screen's room left by the menu bar and the Dock, smaller if need be.
@MainActor private func onScreen(_ screen: String, _ rect: NSRect) -> NSRect? {
    guard
        let target = NSScreen.screens.first(where: { uuid(of: $0) == screen })
            ?? NSScreen.screens.first
    else { return nil }
    let room = target.visibleFrame
    let width = min(rect.width, room.width)
    let height = min(rect.height, room.height)
    let x = target.frame.minX + rect.minX
    let top = target.frame.maxY - rect.minY
    return NSRect(
        x: min(max(x, room.minX), room.maxX - width),
        y: min(max(top - height, room.minY), room.maxY - height),
        width: width, height: height)
}

/// Where the window `id` is: its screen's display UUID, written into `screen`, at most `size`
/// bytes with the ending zero, and its frame from the top left corner of that screen; full
/// screen, its frame once it leaves it. False for a window not made.
@_cdecl("arbalete_ui_window_place")
public func windowPlace(
    _ id: UInt32, _ screen: UnsafeMutablePointer<CChar>, _ size: Int,
    _ x: UnsafeMutablePointer<Double>, _ y: UnsafeMutablePointer<Double>,
    _ width: UnsafeMutablePointer<Double>, _ height: UnsafeMutablePointer<Double>,
    _ fullScreen: UnsafeMutablePointer<Bool>
) -> Bool {
    MainActor.assumeIsolated {
        guard let main = mainWindows[id],
            let on = main.window.screen ?? NSScreen.screens.first,
            let name = uuid(of: on)
        else { return false }
        let full = main.window.styleMask.contains(.fullScreen)
        let frame = full ? main.normalFrame ?? main.window.frame : main.window.frame
        strlcpy(screen, name, size)
        (x.pointee, y.pointee) = (frame.minX - on.frame.minX, on.frame.maxY - frame.maxY)
        (width.pointee, height.pointee) = (frame.width, frame.height)
        fullScreen.pointee = full
        return true
    }
}

/// A rect from the top left corner of the main screen, as the app counts, in AppKit's screen
/// coordinates, from its bottom left corner; or back.
@MainActor private func screenRect(_ rect: NSRect) -> NSRect {
    let top = NSScreen.screens.first?.frame.maxY ?? 0
    return NSRect(x: rect.minX, y: top - rect.maxY, width: rect.width, height: rect.height)
}

/// The window `id`'s frame, from the top left corner of the main screen, and its content's size.
@_cdecl("arbalete_ui_window_frame")
public func windowFrame(
    _ id: UInt32, _ x: UnsafeMutablePointer<Double>, _ y: UnsafeMutablePointer<Double>,
    _ width: UnsafeMutablePointer<Double>, _ height: UnsafeMutablePointer<Double>
) {
    MainActor.assumeIsolated {
        guard let window = mainWindows[id]?.window else { return }
        let frame = screenRect(window.frame)
        (x.pointee, y.pointee) = (frame.minX, frame.minY)
        (width.pointee, height.pointee) = (frame.width, frame.height)
    }
}

/// Shows the window `id` in front, with the keyboard, or brings it there.
@_cdecl("arbalete_ui_window_activate")
public func activateWindow(_ id: UInt32) {
    MainActor.assumeIsolated {
        guard let window = mainWindows[id]?.window else { return }
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        if mainWindows[id]?.opensFullScreen == true {
            mainWindows[id]?.opensFullScreen = false
            window.toggleFullScreen(nil)
        }
    }
}

/// Asks the window `id` to close, which the app may refuse while its pages close.
@_cdecl("arbalete_ui_window_close")
public func closeWindow(_ id: UInt32) {
    MainActor.assumeIsolated { mainWindows[id]?.requestClose() }
}

/// The window `id`'s NSWindow, for what the app does to it directly (app/src/mac.rs).
@_cdecl("arbalete_ui_window_handle")
public func windowHandle(_ id: UInt32) -> UnsafeMutableRawPointer? {
    // As an address: a pointer may not leave the main actor's closure.
    let address: UInt = MainActor.assumeIsolated {
        guard let window = mainWindows[id]?.window else { return 0 }
        return UInt(bitPattern: Unmanaged.passUnretained(window).toOpaque())
    }
    return UnsafeMutableRawPointer(bitPattern: address)
}
