// The About window, like Arc's: the standard panel, the app's version with Chromium's below it,
// and credits pointing to Chromium and the open source software Arbalete is made of.

import AppKit

@_cdecl("arbalete_ui_about")
public func about(_ chromium: UnsafePointer<CChar>) {
    let chromium = String(cString: chromium)
    MainActor.assumeIsolated {
        let info = Bundle.main.infoDictionary
        let version = info?["CFBundleShortVersionString"] as? String ?? ""
        let build = info?["CFBundleVersion"] as? String ?? version
        // Arc's "1.171.0 (89666)": the build only when it says something more.
        let number = build == version ? version : "\(version) (\(build))"
        NSApp.orderFrontStandardAboutPanel(options: [
            .applicationVersion: "\(number)\nChromium Engine Version \(chromium)",
            // Empty, so the panel adds no "(…)" after the version.
            .version: "",
            .credits: credits(),
            // In the credits, above them, like Arc's.
            NSApplication.AboutPanelOptionKey(rawValue: "Copyright"): "",
        ])
        NSApp.activate()
    }
}

/// The copyright, then "Arbalete is made possible by…", centred at the version's size, its links
/// to Chromium, the licenses of Chromium's software and of the Rust crates (the CREDITS.html and
/// Rust-CREDITS.html the app carries) and Arbalete's source.
@MainActor private func credits() -> NSAttributedString {
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = .center
    let plain: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize), .foregroundColor: NSColor.labelColor,
        .paragraphStyle: paragraph,
    ]
    let text = NSMutableAttributedString()
    func add(_ string: String, link: URL? = nil) {
        var attributes = plain
        if let link { attributes[.link] = link }
        text.append(NSAttributedString(string: string, attributes: attributes))
    }
    let licenses = Bundle.main.url(forResource: "CREDITS", withExtension: "html")
    let crates = Bundle.main.url(forResource: "Rust-CREDITS", withExtension: "html")
    add("Copyright © 2026 The Arbalete contributors. Licensed under MPL-2.0.\n\n")
    add("Arbalete is made possible by the ")
    add("Chromium", link: URL(string: "https://www.chromium.org"))
    add(" open source project, Chromium’s ")
    add("open source software", link: licenses)
    add(", Arbalete’s ")
    add("Rust crates", link: crates)
    add(", as well as other ")
    add("open source software", link: URL(string: "https://github.com/amenophis/arbalete"))
    add(".")
    return text
}
