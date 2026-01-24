//! Parse type comments from Python source code

/// Result of parsing a type comment
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCommentKind {
    /// A type: ignore comment with optional error codes
    Ignore(Vec<String>),
    /// A type annotation comment (e.g., `# type: list[int]`)
    /// Returns the type annotation string (without `# type:` prefix)
    TypeAnnotation(String),
}

/// Parse a type comment and determine if it's a type: ignore or type annotation.
///
/// # Arguments
///
/// * `comment` - The comment string to parse (should include the leading `#`)
///
/// # Returns
///
/// - `Some(TypeCommentKind::Ignore(codes))` if it's a type: ignore comment
/// - `Some(TypeCommentKind::TypeAnnotation(annotation))` if it's a type annotation
/// - `None` if it's not a type comment
///
/// # Examples
///
/// ```
/// use mypy_parser::type_comment::{parse_type_comment_kind, TypeCommentKind};
///
/// assert_eq!(parse_type_comment_kind("# type: ignore"), Some(TypeCommentKind::Ignore(vec![])));
/// assert_eq!(parse_type_comment_kind("# type: int"), Some(TypeCommentKind::TypeAnnotation("int".to_string())));
/// assert_eq!(parse_type_comment_kind("# type: list[int]  # comment"), Some(TypeCommentKind::TypeAnnotation("list[int]".to_string())));
/// ```
pub fn parse_type_comment_kind(comment: &str) -> Option<TypeCommentKind> {
    // Remove leading '#' and whitespace
    let trimmed = comment.trim_start_matches('#').trim_start();

    // Check if it starts with "type:"
    if !trimmed.starts_with("type:") {
        return None;
    }

    // Get the part after "type:"
    let after_type = trimmed["type:".len()..].trim_start();

    // Check if it's a type: ignore comment
    if after_type.starts_with("ignore") {
        // Check if "ignore" is followed by whitespace, '[', or end of string
        let after_ignore = &after_type["ignore".len()..];
        if after_ignore.is_empty()
            || after_ignore.starts_with(|c: char| c.is_whitespace() || c == '[')
        {
            // Parse as type: ignore
            let after_ignore_trimmed = after_ignore.trim_start();

            // Check if there are error codes in brackets
            if after_ignore_trimmed.starts_with('[') {
                // Ensure only whitespace was between 'ignore' and '['
                let whitespace_between =
                    &after_ignore[..after_ignore.len() - after_ignore_trimmed.len()];
                if !whitespace_between.chars().all(char::is_whitespace) {
                    return None;
                }

                if let Some(bracket_end) = after_ignore_trimmed.find(']') {
                    // Extract the content between brackets
                    let codes_str = &after_ignore_trimmed[1..bracket_end];

                    // Split by comma and collect error codes
                    let error_codes: Vec<String> = codes_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    return Some(TypeCommentKind::Ignore(error_codes));
                }
            }

            // No error codes specified (just "# type: ignore")
            return Some(TypeCommentKind::Ignore(Vec::new()));
        }
    }

    // Not a type: ignore, so treat as type annotation
    // Extract the type annotation, stopping at the next '#' (which could be a regular comment or type: ignore)
    let type_annotation = if let Some(hash_pos) = after_type.find('#') {
        // There's another comment after the type annotation
        after_type[..hash_pos].trim_end()
    } else {
        // No trailing comment
        after_type.trim_end()
    };

    if type_annotation.is_empty() {
        return None;
    }

    Some(TypeCommentKind::TypeAnnotation(type_annotation.to_string()))
}

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
/// Error codes are parsed from brackets like `[code1, code2]`.
/// Only whitespace is allowed between 'ignore' and '['.
///
/// # Examples
///
/// ```
/// use mypy_parser::type_comment::parse_type_comment;
///
/// assert_eq!(parse_type_comment("# type: ignore"), Some(vec![]));
/// assert_eq!(parse_type_comment("# type: ignore[arg-type]"), Some(vec!["arg-type".to_string()]));
/// assert_eq!(parse_type_comment("# type: ignore [override]"), Some(vec!["override".to_string()]));
/// assert_eq!(parse_type_comment("# type: ignore[arg-type, override]"), Some(vec!["arg-type".to_string(), "override".to_string()]));
/// assert_eq!(parse_type_comment("# regular comment"), None);
/// ```
pub fn parse_type_comment(comment: &str) -> Option<Vec<String>> {
    match parse_type_comment_kind(comment) {
        Some(TypeCommentKind::Ignore(codes)) => Some(codes),
        _ => None,
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
    fn test_type_ignore_with_single_code() {
        assert_eq!(
            parse_type_comment("# type: ignore[arg-type]"),
            Some(vec!["arg-type".to_string()])
        );
        assert_eq!(
            parse_type_comment("# type: ignore[override]"),
            Some(vec!["override".to_string()])
        );
    }

    #[test]
    fn test_type_ignore_with_multiple_codes() {
        assert_eq!(
            parse_type_comment("# type: ignore[arg-type, override]"),
            Some(vec!["arg-type".to_string(), "override".to_string()])
        );
        assert_eq!(
            parse_type_comment("# type: ignore[name-defined,no-untyped-def]"),
            Some(vec![
                "name-defined".to_string(),
                "no-untyped-def".to_string()
            ])
        );
    }

    #[test]
    fn test_type_ignore_with_whitespace() {
        assert_eq!(parse_type_comment("#type: ignore"), Some(vec![]));
        assert_eq!(parse_type_comment("#  type: ignore"), Some(vec![]));
        assert_eq!(parse_type_comment("# type: ignore "), Some(vec![]));

        // Whitespace before bracket is ok
        assert_eq!(
            parse_type_comment("# type: ignore [arg-type]"),
            Some(vec!["arg-type".to_string()])
        );

        // Whitespace around codes
        assert_eq!(
            parse_type_comment("# type: ignore[ arg-type , override ]"),
            Some(vec!["arg-type".to_string(), "override".to_string()])
        );
    }

    #[test]
    fn test_not_type_ignore() {
        assert_eq!(parse_type_comment("# regular comment"), None);
        assert_eq!(parse_type_comment("# TODO: fix this"), None);
        assert_eq!(parse_type_comment("# type: int"), None);

        // Non-whitespace between 'ignore' and '[' should fail
        assert_eq!(parse_type_comment("# type: ignore-[arg-type]"), None);
        assert_eq!(parse_type_comment("# type: ignorefoo[arg-type]"), None);
    }

    #[test]
    fn test_empty_comment() {
        assert_eq!(parse_type_comment("#"), None);
        assert_eq!(parse_type_comment(""), None);
    }

    #[test]
    fn test_empty_error_codes() {
        // Empty brackets should return empty vec
        assert_eq!(parse_type_comment("# type: ignore[]"), Some(vec![]));
    }

    #[test]
    fn test_type_comment_kind_ignore() {
        assert_eq!(
            parse_type_comment_kind("# type: ignore"),
            Some(TypeCommentKind::Ignore(vec![]))
        );
        assert_eq!(
            parse_type_comment_kind("# type: ignore[arg-type]"),
            Some(TypeCommentKind::Ignore(vec!["arg-type".to_string()]))
        );
    }

    #[test]
    fn test_type_comment_kind_annotation() {
        assert_eq!(
            parse_type_comment_kind("# type: int"),
            Some(TypeCommentKind::TypeAnnotation("int".to_string()))
        );
        assert_eq!(
            parse_type_comment_kind("# type: list[int]"),
            Some(TypeCommentKind::TypeAnnotation("list[int]".to_string()))
        );
        assert_eq!(
            parse_type_comment_kind("# type: Dict[str, int]"),
            Some(TypeCommentKind::TypeAnnotation(
                "Dict[str, int]".to_string()
            ))
        );
    }

    #[test]
    fn test_type_comment_kind_annotation_with_trailing_comment() {
        assert_eq!(
            parse_type_comment_kind("# type: int  # This is a comment"),
            Some(TypeCommentKind::TypeAnnotation("int".to_string()))
        );
        assert_eq!(
            parse_type_comment_kind("# type: list[int] # comment"),
            Some(TypeCommentKind::TypeAnnotation("list[int]".to_string()))
        );
    }

    #[test]
    fn test_type_comment_kind_annotation_with_type_ignore() {
        // Type annotation followed by type: ignore on the same line
        assert_eq!(
            parse_type_comment_kind("# type: str # type: ignore"),
            Some(TypeCommentKind::TypeAnnotation("str".to_string()))
        );
        assert_eq!(
            parse_type_comment_kind("# type: list[int] # type: ignore[arg-type]"),
            Some(TypeCommentKind::TypeAnnotation("list[int]".to_string()))
        );
    }

    #[test]
    fn test_type_comment_kind_not_type_comment() {
        assert_eq!(parse_type_comment_kind("# regular comment"), None);
        assert_eq!(parse_type_comment_kind("# TODO: fix this"), None);
        assert_eq!(parse_type_comment_kind("# type:"), None); // Empty annotation
    }

    #[test]
    fn test_type_comment_kind_whitespace_handling() {
        assert_eq!(
            parse_type_comment_kind("#type: int"),
            Some(TypeCommentKind::TypeAnnotation("int".to_string()))
        );
        assert_eq!(
            parse_type_comment_kind("#  type:  int  "),
            Some(TypeCommentKind::TypeAnnotation("int".to_string()))
        );
    }
}
