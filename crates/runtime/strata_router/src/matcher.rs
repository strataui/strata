//! Path Pattern Matching and Query Parameter Extraction.

use std::collections::HashMap;

/// Result of matching a URL path against a route pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteMatch {
    pub pattern: String,
    pub path: String,
    pub params: HashMap<String, String>,
    pub score: usize,
}

/// Matches a URL path against a pattern like `/users/:id/posts/:post_id` or `/files/*tail`.
pub fn match_path(pattern: &str, path: &str) -> Option<RouteMatch> {
    let clean_pattern = normalize_path(pattern);
    let clean_path = normalize_path(path);

    let pattern_segments: Vec<&str> = clean_pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_segments: Vec<&str> = clean_path.split('/').filter(|s| !s.is_empty()).collect();

    let mut params = HashMap::new();
    let mut score = 0;

    let mut p_idx = 0;
    let mut path_idx = 0;

    while p_idx < pattern_segments.len() {
        let p_seg = pattern_segments[p_idx];

        if let Some(stripped) = p_seg.strip_prefix('*') {
            // Wildcard match for remaining segments
            let var_name = if !stripped.is_empty() {
                stripped
            } else {
                "wildcard"
            };
            let remaining = path_segments[path_idx..].join("/");
            params.insert(var_name.to_string(), remaining);
            score += 10;
            return Some(RouteMatch {
                pattern: pattern.to_string(),
                path: path.to_string(),
                params,
                score,
            });
        }

        if path_idx >= path_segments.len() {
            return None;
        }

        let actual_seg = path_segments[path_idx];

        if let Some(var_name) = p_seg.strip_prefix(':') {
            // Dynamic parameter match
            params.insert(var_name.to_string(), actual_seg.to_string());
            score += 50; // Dynamic segment weight
        } else if p_seg == actual_seg {
            // Exact literal segment match
            score += 100; // Exact match carries highest weight
        } else {
            return None;
        }

        p_idx += 1;
        path_idx += 1;
    }

    if path_idx != path_segments.len() {
        // Path has trailing unmatched segments without wildcard
        return None;
    }

    Some(RouteMatch {
        pattern: pattern.to_string(),
        path: path.to_string(),
        params,
        score,
    })
}

/// Parses URL search query string (e.g., `?q=rust&sort=desc&page=2`).
pub fn parse_query_string(query_str: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    let query = query_str.trim_start_matches('?');
    if query.is_empty() {
        return params;
    }

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        if !key.is_empty() {
            params.insert(key.to_string(), value.to_string());
        }
    }

    params
}

/// Normalizes path by stripping duplicate slashes and trailing slashes.
pub fn normalize_path(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    let mut last_was_slash = false;

    for c in path.chars() {
        if c == '/' {
            if !last_was_slash {
                normalized.push('/');
                last_was_slash = true;
            }
        } else {
            normalized.push(c);
            last_was_slash = false;
        }
    }

    if normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_path_match() {
        let m = match_path("/dashboard", "/dashboard").unwrap();
        assert_eq!(m.pattern, "/dashboard");
        assert!(m.params.is_empty());
        assert_eq!(m.score, 100);

        assert!(match_path("/dashboard", "/dashboard/settings").is_none());
        assert!(match_path("/dashboard/settings", "/dashboard").is_none());
    }

    #[test]
    fn test_dynamic_params_match() {
        let m = match_path("/users/:user_id/posts/:post_id", "/users/42/posts/108").unwrap();
        assert_eq!(m.params.get("user_id").unwrap(), "42");
        assert_eq!(m.params.get("post_id").unwrap(), "108");
    }

    #[test]
    fn test_wildcard_match() {
        let m = match_path("/static/*filepath", "/static/images/avatars/user.png").unwrap();
        assert_eq!(m.params.get("filepath").unwrap(), "images/avatars/user.png");
    }

    #[test]
    fn test_query_string_parser() {
        let q = parse_query_string("?search=strata&page=3&sort=asc");
        assert_eq!(q.get("search").unwrap(), "strata");
        assert_eq!(q.get("page").unwrap(), "3");
        assert_eq!(q.get("sort").unwrap(), "asc");
    }
}
