// The app's state, as it sends it (UiState::to_json in app/src/shell.rs).

import Foundation

struct AppState: Decodable {
    var page: PageState
    /// The displayed Space, among every Space in sidebar order.
    var space: Int
    var spaces: [Space]
    /// The pages of the displayed Space and its neighbors.
    var pages: [SpacePage]
    var profiles: [Profile]
    /// The bar above the page holds the navigation buttons and the address.
    var toolbar: Bool
    /// The displayed Space's profile's pinned extensions, whose buttons show by the address.
    var extensions: [Extension]
    /// The tabs closed since launch, the last closed first.
    var closed: [ClosedTab]
    /// All of that profile's extensions, which the library lists.
    var installed: [InstalledExtension]
}

/// The loading state of the shown page.
struct PageState: Decodable {
    var progress: Double
    var loading: Bool
    var canGoBack: Bool
    var canGoForward: Bool
}

struct Space: Decodable, Identifiable {
    var id: Int
    var title: String
    var emoji: String?
    var theme: Theme
    var profile: Int
}

/// How a Space colors the window (Theme in crates/arbalete-core/src/sidebar.rs).
struct Theme: Codable, Equatable {
    var colors: [String]
    var mode: String
    var intensity: Double
    var noise: Double
}

struct Profile: Decodable, Identifiable {
    var id: Int
    var name: String
    var color: String
}

struct Extension: Decodable, Identifiable {
    var id: String
    var name: String
    var icon: String?
    var popup: Bool
}

/// What the sidebar shows of a Space: its tabs, and the favorites of its profile.
struct SpacePage: Decodable, Identifiable {
    var space: Int
    var selected: Int?
    /// Only the Space's header and its New Tab row show.
    var collapsed: Bool
    /// The containers of the sections, to move tabs and folders into.
    var roots: Roots
    var favorites: [Tab]
    /// The empty favorites show a box inviting to drag tabs there.
    var favoritesHint: Bool
    var pinned: [Item]
    var today: [Item]

    var id: Int { space }
}

struct Roots: Decodable {
    var favorites: Int
    var pinned: Int
    var today: Int
}

struct Tab: Decodable, Identifiable {
    var id: Int
    var url: String
    var title: String
    var favicon: String?
    var muted: Bool?
    /// Its page is frozen or unloaded to save memory.
    var asleep: Bool?

    /// Its title, or its address without one.
    var label: String { title.isEmpty ? url : title }
}

/// A tab, a folder, or two tabs split in one row.
enum Item: Decodable, Identifiable {
    case tab(Tab)
    case folder(id: Int, title: String, expanded: Bool, children: [Item])
    case split(id: Int, tabs: [Tab])

    private enum Keys: String, CodingKey {
        case id, title, expanded, children, split
    }

    init(from decoder: Decoder) throws {
        let keys = try decoder.container(keyedBy: Keys.self)
        let id = try keys.decode(Int.self, forKey: .id)
        if let children = try keys.decodeIfPresent([Item].self, forKey: .children) {
            self = .folder(
                id: id, title: try keys.decode(String.self, forKey: .title),
                expanded: try keys.decode(Bool.self, forKey: .expanded), children: children)
        } else if let tabs = try keys.decodeIfPresent([Tab].self, forKey: .split) {
            self = .split(id: id, tabs: tabs)
        } else {
            self = .tab(try Tab(from: decoder))
        }
    }

    var id: Int {
        switch self {
        case .tab(let tab): tab.id
        case .folder(let id, _, _, _), .split(let id, _): id
        }
    }

    /// The tabs it holds.
    var tabs: [Tab] {
        switch self {
        case .tab(let tab): [tab]
        case .folder(_, _, _, let children): children.flatMap(\.tabs)
        case .split(_, let tabs): tabs
        }
    }
}
