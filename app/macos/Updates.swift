// Updates, with Sparkle's standard alerts: a release looks for them once a day in the feed its
// Info.plist names (SUFeedURL, set by mise run release). A development build has no feed, so its
// updater never starts and it never updates itself.

import AppKit
import Sparkle

@MainActor private var controller: SPUStandardUpdaterController?

@_cdecl("arbalete_ui_start_updates")
public func startUpdates() {
    MainActor.assumeIsolated {
        controller = SPUStandardUpdaterController(
            startingUpdater: Bundle.main.object(forInfoDictionaryKey: "SUFeedURL") != nil,
            updaterDelegate: nil, userDriverDelegate: nil)
    }
}

@_cdecl("arbalete_ui_check_for_updates")
public func checkForUpdates() {
    MainActor.assumeIsolated { controller?.checkForUpdates(nil) }
}

/// Whether Check for Updates… can check now: not in a development build, nor during a check.
@_cdecl("arbalete_ui_can_check_for_updates")
public func canCheckForUpdates() -> Bool {
    MainActor.assumeIsolated { controller?.updater.canCheckForUpdates ?? false }
}
