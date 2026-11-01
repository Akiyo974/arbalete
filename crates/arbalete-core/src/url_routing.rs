//! URL Routing: where the links other apps open go, by rules on their url, like Arc's.

use crate::sidebar::SpaceId;
use serde::{Deserialize, Serialize};

/// How a route's text matches a url.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Match {
    /// The url holds the text, whatever its case.
    #[default]
    Contains,
    /// The url's host, or the whole url, is the text, whatever its case.
    IsEqualTo,
}

/// Where a link opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Destination {
    /// The Space shown last.
    #[default]
    MostRecentSpace,
    /// A Little Arbalete window.
    Little,
    /// A given Space.
    Space(SpaceId),
}

/// A rule: the links whose url matches `text` open in `destination`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    pub matching: Match,
    pub text: String,
    pub destination: Destination,
}

/// The routes, in order, and where the links no route matches open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlRouting {
    pub routes: Vec<Route>,
    pub default: Destination,
}

impl Default for UrlRouting {
    /// No route, and links open in a Little Arbalete window.
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            default: Destination::Little,
        }
    }
}

impl UrlRouting {
    /// Where `url` opens: as the first route it matches says, or as the default does. A route
    /// without text matches nothing.
    pub fn destination(&self, url: &str) -> Destination {
        self.routes
            .iter()
            .find(|route| matches(route.matching, &route.text, url))
            .map_or(self.default, |route| route.destination)
    }

    /// Sends the links that would open in `space` to the Space shown last instead.
    pub fn forget_space(&mut self, space: SpaceId) {
        let destinations = self.routes.iter_mut().map(|route| &mut route.destination);
        for destination in destinations.chain([&mut self.default]) {
            if *destination == Destination::Space(space) {
                *destination = Destination::MostRecentSpace;
            }
        }
    }
}

fn matches(matching: Match, text: &str, url: &str) -> bool {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return false;
    }
    let url = url.to_lowercase();
    match matching {
        Match::Contains => url.contains(&text),
        Match::IsEqualTo => {
            host(&url) == text || url.trim_end_matches('/') == text.trim_end_matches('/')
        }
    }
}

/// A url's host, without its port.
fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    authority.split(':').next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::Sidebar;

    /// Two Spaces of a sidebar.
    fn spaces() -> (SpaceId, SpaceId) {
        let mut sidebar = Sidebar::default();
        let home = sidebar.spaces()[0].clone();
        let work = sidebar.add_space("Work", home.profile).unwrap();
        (home.id, work)
    }

    fn route(matching: Match, text: &str, destination: Destination) -> Route {
        Route {
            matching,
            text: text.to_string(),
            destination,
        }
    }

    #[test]
    fn opens_links_in_a_little_window_without_routes() {
        let routing = UrlRouting::default();
        assert_eq!(
            routing.destination("https://example.com/"),
            Destination::Little
        );
    }

    #[test]
    fn the_first_matching_route_wins_else_the_default() {
        let (_, work) = spaces();
        let routing = UrlRouting {
            routes: vec![
                route(
                    Match::Contains,
                    "meet.google.com",
                    Destination::MostRecentSpace,
                ),
                route(Match::Contains, "google.com", Destination::Space(work)),
                route(Match::Contains, "", Destination::Little),
            ],
            default: Destination::Space(work),
        };
        assert_eq!(
            routing.destination("https://MEET.google.com/abc"),
            Destination::MostRecentSpace
        );
        assert_eq!(
            routing.destination("https://docs.google.com/"),
            Destination::Space(work)
        );
        // A route without text matches nothing.
        assert_eq!(
            routing.destination("https://example.com/"),
            Destination::Space(work)
        );
    }

    #[test]
    fn is_equal_to_compares_the_host_or_the_whole_url() {
        let routing = UrlRouting {
            routes: vec![
                route(Match::IsEqualTo, "test.com", Destination::Little),
                route(Match::IsEqualTo, "https://a.test/page", Destination::Little),
            ],
            default: Destination::MostRecentSpace,
        };
        for url in [
            "https://test.com/",
            "http://user@TEST.com:8080/path?q#f",
            "https://a.test/page/",
        ] {
            assert_eq!(routing.destination(url), Destination::Little, "{url}");
        }
        for url in [
            "https://www.test.com/",
            "https://test.com.evil/",
            "https://a.test/",
        ] {
            assert_eq!(
                routing.destination(url),
                Destination::MostRecentSpace,
                "{url}"
            );
        }
    }

    #[test]
    fn links_of_a_space_gone_open_in_the_space_shown_last() {
        let (home, work) = spaces();
        let mut routing = UrlRouting {
            routes: vec![
                route(Match::Contains, "a", Destination::Space(work)),
                route(Match::Contains, "b", Destination::Space(home)),
            ],
            default: Destination::Space(work),
        };
        routing.forget_space(work);
        assert_eq!(routing.routes[0].destination, Destination::MostRecentSpace);
        assert_eq!(routing.routes[1].destination, Destination::Space(home));
        assert_eq!(routing.default, Destination::MostRecentSpace);
    }
}
