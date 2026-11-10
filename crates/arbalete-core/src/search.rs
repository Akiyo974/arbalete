//! The search engines a profile searches what is typed in the Command Bar with.

use serde::{Deserialize, Serialize};

/// Arc's list of engines, Google first, its default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    #[default]
    Google,
    DuckDuckGo,
    Bing,
    Ecosia,
    Qwant,
    Brave,
    Perplexity,
    Yahoo,
}

impl SearchEngine {
    pub const ALL: [Self; 8] = [
        Self::Google,
        Self::DuckDuckGo,
        Self::Bing,
        Self::Ecosia,
        Self::Qwant,
        Self::Brave,
        Self::Perplexity,
        Self::Yahoo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::DuckDuckGo => "DuckDuckGo",
            Self::Bing => "Bing",
            Self::Ecosia => "Ecosia",
            Self::Qwant => "Qwant",
            Self::Brave => "Brave",
            Self::Perplexity => "Perplexity",
            Self::Yahoo => "Yahoo",
        }
    }

    /// The address of a search, before its percent-encoded query.
    pub fn url(self) -> &'static str {
        match self {
            Self::Google => "https://www.google.com/search?q=",
            Self::DuckDuckGo => "https://duckduckgo.com/?q=",
            Self::Bing => "https://www.bing.com/search?q=",
            Self::Ecosia => "https://www.ecosia.org/search?q=",
            Self::Qwant => "https://www.qwant.com/?q=",
            Self::Brave => "https://search.brave.com/search?q=",
            Self::Perplexity => "https://www.perplexity.ai/search?q=",
            Self::Yahoo => "https://search.yahoo.com/search?p=",
        }
    }

    /// The address of its suggestions for typed text, before the percent-encoded text, if it
    /// offers any. Each answers in the OpenSearch format (suggestions).
    pub fn suggest_url(self) -> Option<&'static str> {
        match self {
            Self::Google => {
                Some("https://suggestqueries.google.com/complete/search?client=firefox&q=")
            }
            Self::DuckDuckGo => Some("https://duckduckgo.com/ac/?type=list&q="),
            Self::Bing => Some("https://api.bing.com/osjson.aspx?query="),
            Self::Ecosia => Some("https://ac.ecosia.org/autocomplete?type=list&q="),
            Self::Qwant => Some("https://api.qwant.com/api/suggest/?client=opensearch&q="),
            Self::Brave => Some("https://search.brave.com/api/suggest?q="),
            Self::Perplexity => None,
            Self::Yahoo => Some("https://ff.search.yahoo.com/gossip?output=fxjson&command="),
        }
    }
}

/// Text percent-encoded for a query, as `encodeURIComponent` does in the UI: what is not a letter,
/// a digit or one of `-_.!~*'()` goes as its UTF-8 bytes.
pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// The address typed text opens, as the Command Bar reads it (toURL in app/macos/CommandBar.swift):
/// itself with a scheme, a host or an IP address with `https://`, localhost with `http://`, and
/// none for words to search for.
pub fn address(text: &str) -> Option<String> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    let schemes = [
        "http://",
        "https://",
        "file://",
        "ftp://",
        "chrome://",
        "arbalete://",
    ];
    let prefixes = ["about:", "data:", "mailto:", "view-source:"];
    if schemes
        .iter()
        .chain(&prefixes)
        .any(|start| lower.starts_with(start))
    {
        return Some(text.to_string());
    }
    if text.is_empty() || text.contains(char::is_whitespace) {
        return None;
    }
    let host = lower.split('/').next().unwrap_or_default();
    let (name, port) = host.split_once(':').unwrap_or((host, ""));
    if !port.chars().all(|c| c.is_ascii_digit()) || (host.contains(':') && port.is_empty()) {
        return None;
    }
    if name == "localhost" {
        return Some(format!("http://{text}"));
    }
    let labels: Vec<&str> = name.split('.').collect();
    let ip = labels.len() == 4
        && labels
            .iter()
            .all(|label| !label.is_empty() && label.chars().all(|c| c.is_ascii_digit()));
    let domain = labels.len() > 1
        && labels[..labels.len() - 1]
            .iter()
            .all(|label| !label.is_empty())
        && labels.last().is_some_and(|top| {
            top.len() >= 2 && top.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    (ip || domain).then(|| format!("https://{text}"))
}

/// The suggestions of an engine's OpenSearch answer, `["typed text", ["suggestion", …], …]`:
/// none when it is not one.
pub fn suggestions(answer: &str) -> Vec<String> {
    let answer: serde_json::Value = serde_json::from_str(answer).unwrap_or_default();
    let list = answer.get(1).and_then(serde_json::Value::as_array);
    list.into_iter()
        .flatten()
        .filter_map(|suggestion| suggestion.as_str().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_typed_text_as_an_address_or_words() {
        assert_eq!(
            address("https://a.test/x").as_deref(),
            Some("https://a.test/x")
        );
        assert_eq!(address("About:blank").as_deref(), Some("About:blank"));
        assert_eq!(
            address("example.com").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(address("example.com:8080/a b"), None);
        assert_eq!(
            address("docs.rs/serde").as_deref(),
            Some("https://docs.rs/serde")
        );
        assert_eq!(
            address("localhost:3000").as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(
            address("192.168.1.1").as_deref(),
            Some("https://192.168.1.1")
        );
        assert_eq!(address("rust traits"), None);
        assert_eq!(address("hello"), None);
        assert_eq!(address("v1.2"), None);
        assert_eq!(address("a.b:"), None);
        assert_eq!(address(""), None);
    }

    #[test]
    fn names_each_engine_in_lowercase_for_the_ui() {
        for engine in SearchEngine::ALL {
            let id = serde_json::to_string(&engine).unwrap();
            assert_eq!(id, format!("\"{}\"", engine.name().to_lowercase()));
            assert!(engine.url().starts_with("https://"));
        }
        assert_eq!(SearchEngine::default(), SearchEngine::Google);
    }

    #[test]
    fn encodes_queries_like_the_ui() {
        assert_eq!(
            encode("rust lang & café?"),
            "rust%20lang%20%26%20caf%C3%A9%3F"
        );
        assert_eq!(encode("a-b_c.d!~*'()"), "a-b_c.d!~*'()");
    }

    #[test]
    fn reads_the_suggestions_of_an_opensearch_answer() {
        let answer = r#"["rust",["rustdesk","rust language"],[],{"google:suggesttype":[]}]"#;
        assert_eq!(suggestions(answer), ["rustdesk", "rust language"]);
        assert!(suggestions("not json").is_empty());
        assert!(suggestions(r#"{"rust":1}"#).is_empty());
        assert!(
            SearchEngine::ALL
                .iter()
                .filter_map(|e| e.suggest_url())
                .all(|u| u.starts_with("https://"))
        );
    }
}
