/// A NATS-style subject for event routing.
///
/// A subject is a dot-separated string (e.g. `"signal.BTC"`, `"market.price.>"`).
/// Patterns use two wildcard tokens:
/// - `*` matches exactly one word
/// - `>` matches one or more trailing words (must be the last token)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subject(String);

impl Subject {
    /// Create a subject from a string.
    /// Panics if the subject contains a pattern with `>` not at the end.
    pub fn new(s: &str) -> Self {
        let s = s.trim().to_string();
        if let Some(pos) = s.find('>') {
            assert!(
                pos == s.len() - 1 || (pos + 1 < s.len() && s[pos + 1..].trim().is_empty()),
                "`>` wildcard must be the last token in a subject"
            );
        }
        Self(s)
    }

    /// Create a subject from a raw string (for internal use).
    pub fn from_raw(raw: String) -> Self {
        Self(raw)
    }

    /// Returns the subject string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if this subject matches the given published subject.
    ///
    /// A subject IS a pattern. So we check if `self` (the subscription pattern)
    /// matches `other` (the published subject).
    ///
    /// Rules:
    /// - Exact match returns true
    /// - `*` matches exactly one word
    /// - `>` matches any remaining words (must be last)
    /// - `>` alone matches everything
    pub fn matches(&self, other: &Subject) -> bool {
        let pattern_parts: Vec<&str> = self.0.split('.').collect();
        let subject_parts: Vec<&str> = other.0.split('.').collect();

        // `>` alone matches everything
        if pattern_parts == [">"] {
            return true;
        }

        let mut p_iter = pattern_parts.iter();
        let mut s_iter = subject_parts.iter();

        loop {
            match (p_iter.next(), s_iter.next()) {
                // Both consumed → match
                (None, None) => return true,
                // Pattern has ">" at end → matches any remaining subject tokens
                (Some(&">"), _) => return true,
                // Pattern consumed but subject has more → no match
                (None, Some(_)) => return false,
                // Subject consumed but pattern has more (and not ">") → no match
                (Some(_), None) => return false,
                // "*" matches any single token
                (Some(&"*"), Some(_)) => continue,
                // Exact match (same token)
                (Some(&a), Some(&b)) if a == b => continue,
                // Mismatch
                (Some(_), Some(_)) => return false,
            }
        }
    }
}

impl std::fmt::Display for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Subject {
    fn from(s: &str) -> Self {
        Subject::new(s)
    }
}

impl From<String> for Subject {
    fn from(s: String) -> Self {
        Subject::new(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let sub = Subject::new("signal.BTC");
        assert!(sub.matches(&Subject::new("signal.BTC")));
        assert!(!sub.matches(&Subject::new("signal.ETH")));
        assert!(!sub.matches(&Subject::new("signal.BTC.detail")));
    }

    #[test]
    fn test_wildcard_star() {
        let sub = Subject::new("signal.*");
        assert!(sub.matches(&Subject::new("signal.BTC")));
        assert!(sub.matches(&Subject::new("signal.ETH")));
        assert!(!sub.matches(&Subject::new("signal.BTC.detail")));
        assert!(!sub.matches(&Subject::new("other.BTC")));
    }

    #[test]
    fn test_wildcard_greater() {
        let sub = Subject::new("signal.>");
        assert!(sub.matches(&Subject::new("signal.BTC")));
        assert!(sub.matches(&Subject::new("signal.ETH")));
        assert!(sub.matches(&Subject::new("signal.BTC.detail")));
        assert!(!sub.matches(&Subject::new("other.BTC")));
    }

    #[test]
    fn test_wildcard_catch_all() {
        let sub = Subject::new(">");
        assert!(sub.matches(&Subject::new("signal.BTC")));
        assert!(sub.matches(&Subject::new("anything.here")));
    }

    #[test]
    fn test_no_match_when_subject_shorter() {
        let sub = Subject::new("signal.BTC.detail");
        assert!(!sub.matches(&Subject::new("signal.BTC")));
    }

    #[test]
    fn test_display() {
        let sub = Subject::new("signal.>");
        assert_eq!(format!("{}", sub), "signal.>");
    }

    #[test]
    fn test_from_str() {
        let sub: Subject = "signal.BTC".into();
        assert_eq!(sub.as_str(), "signal.BTC");
    }

    #[test]
    fn test_from_string() {
        let sub: Subject = String::from("signal.BTC").into();
        assert_eq!(sub.as_str(), "signal.BTC");
    }

    #[test]
    #[should_panic(expected = "`>` wildcard must be the last token")]
    fn test_greater_not_at_end_panics() {
        Subject::new("signal.>.BTC");
    }
}
