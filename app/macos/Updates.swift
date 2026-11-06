// Updates, with Sparkle: a release looks for them once a day in the feed its Info.plist names
// (SUFeedURL, set by mise run release) and downloads them silently. Like Arc's, a banner over the
// sidebar's footer (UpdateBanner) then offers to restart into the update, which otherwise installs
// as the app quits, and once updated says so. Check for Updates… shows Sparkle's own window. A
// development build has no feed, so its updater never starts and it never updates itself.

import AppKit
import Sparkle
import SwiftUI

@MainActor final class Updates: NSObject, ObservableObject, SPUUpdaterDelegate {
    static let shared = Updates()

    enum Banner {
        /// An update downloaded, which `install` installs before relaunching the app.
        case ready(install: () -> Void)
        /// The app runs a version newer than the last time.
        case updated(version: String)
    }

    @Published var banner: Banner?
    private var controller: SPUStandardUpdaterController?

    func start() {
        let info = Bundle.main.infoDictionary
        guard info?["SUFeedURL"] != nil else { return }
        controller = SPUStandardUpdaterController(
            startingUpdater: true, updaterDelegate: self, userDriverDelegate: nil)
        let version = info?["CFBundleShortVersionString"] as? String ?? ""
        let defaults = UserDefaults.standard
        if let last = defaults.string(forKey: "ArbaleteLastVersion"), last != version {
            banner = .updated(version: version)
        }
        defaults.set(version, forKey: "ArbaleteLastVersion")
    }

    func updater(
        _ updater: SPUUpdater, willInstallUpdateOnQuit item: SUAppcastItem,
        immediateInstallationBlock: @escaping () -> Void
    ) -> Bool {
        withAnimation { banner = .ready(install: immediateInstallationBlock) }
        return true
    }

    func check() { controller?.checkForUpdates(nil) }

    var canCheck: Bool { controller?.updater.canCheckForUpdates ?? false }
}

@_cdecl("arbalete_ui_start_updates")
public func startUpdates() {
    MainActor.assumeIsolated { Updates.shared.start() }
}

@_cdecl("arbalete_ui_check_for_updates")
public func checkForUpdates() {
    MainActor.assumeIsolated { Updates.shared.check() }
}

/// Whether Check for Updates… can check now: not in a development build, nor during a check.
@_cdecl("arbalete_ui_can_check_for_updates")
public func canCheckForUpdates() -> Bool {
    MainActor.assumeIsolated { Updates.shared.canCheck }
}
