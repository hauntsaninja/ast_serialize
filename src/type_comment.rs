//! Parse type comments from Python source code

/// Parse a type comment and extract error codes if it's a type ignore comment.
///
/// # Arguments
///
/// * `comment` - The comment string to parse (should include the leading `#`)
///
/// # Returns
///
/// `Some(Vec<String>)` containing error codes if it's a type ignore comment,
/// `None` if it's not a type ignore comment.
/// For now, always returns an empty Vec when it is a type ignore comment.
///
/// # Examples
///
/// ```
/// use mypy_parser::type_comment::parse_type_comment;
///
/// assert_eq!(parse_type_comment("# type: ignore"), Some(vec![]));
/// assert_eq!(parse_type_comment("# type: ignore[arg-type]"), Some(vec![]));
/// assert_eq!(parse_type_comment("# regular comment"), None);
/// ```
pub fn parse_type_comment(comment: &str) -> Option<Vec<String>> {
    // Remove leading '#' and whitespace
    let trimmed = comment.trim_start_matches('#').trim_start();

    // Check if it starts with "type: ignore"
    if trimmed.starts_with("type: ignore") {
        // For now, always return an empty list of error codes
        // TODO: Parse error codes from brackets like [arg-type, override]
        Some(Vec::new())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_ignore_basic() {
        assert_eq!(parse_type_comment("# type: ignore"), Some(vec![]));
    }

    #[test]
    fn test_type_ignore_with_code() {
        // For now, we return empty vec even with error codes
        assert_eq!(parse_type_comment("# type: ignore[arg-type]"), Some(vec![]));
        assert_eq!(parse_type_comment("# type: ignore[override]"), Some(vec![]));
    }

    #[test]
    fn test_type_ignore_with_whitespace() {
        assert_eq!(parse_type_comment("#type: ignore"), Some(vec![]));
        assert_eq!(parse_type_comment("#  type: ignore"), Some(vec![]));
        assert_eq!(parse_type_comment("# type: ignore "), Some(vec![]));
    }

    #[test]
    fn test_not_type_ignore() {
        assert_eq!(parse_type_comment("# regular comment"), None);
        assert_eq!(parse_type_comment("# TODO: fix this"), None);
        assert_eq!(parse_type_comment("# type: int"), None);
    }

    #[test]
    fn test_empty_comment() {
        assert_eq!(parse_type_comment("#"), None);
        assert_eq!(parse_type_comment(""), None);
    }
}
