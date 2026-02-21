use openconv_shared::error::OpenConvError;

use crate::error::ServerError;

/// Validate and normalize a display name.
///
/// Trims whitespace, rejects empty strings, strings longer than 64 characters,
/// and strings containing control characters.
pub fn validate_display_name(name: &str) -> Result<String, ServerError> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(OpenConvError::Validation("display name is required".into()).into());
    }
    if trimmed.chars().count() > 64 {
        return Err(
            OpenConvError::Validation("display name must be 64 characters or fewer".into()).into(),
        );
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(
            OpenConvError::Validation("display name must not contain control characters".into())
                .into(),
        );
    }
    Ok(trimmed)
}

/// Escape ILIKE metacharacters (`%` and `_`) in a search pattern.
pub fn escape_ilike(input: &str) -> String {
    input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_display_name_accepts_valid() {
        assert_eq!(validate_display_name("Alice").unwrap(), "Alice");
    }

    #[test]
    fn validate_display_name_trims_whitespace() {
        assert_eq!(validate_display_name("  Alice  ").unwrap(), "Alice");
    }

    #[test]
    fn validate_display_name_rejects_empty() {
        assert!(validate_display_name("").is_err());
    }

    #[test]
    fn validate_display_name_rejects_whitespace_only() {
        assert!(validate_display_name("   ").is_err());
    }

    #[test]
    fn validate_display_name_rejects_over_64_chars() {
        let long = "a".repeat(65);
        assert!(validate_display_name(&long).is_err());
    }

    #[test]
    fn validate_display_name_accepts_exactly_64_chars() {
        let name = "a".repeat(64);
        assert!(validate_display_name(&name).is_ok());
    }

    #[test]
    fn validate_display_name_rejects_control_characters() {
        assert!(validate_display_name("Alice\x00Bob").is_err());
        assert!(validate_display_name("Alice\nBob").is_err());
    }

    #[test]
    fn validate_display_name_counts_chars_not_bytes() {
        let name = "\u{4e00}".repeat(64);
        assert!(validate_display_name(&name).is_ok());
        let name = "\u{4e00}".repeat(65);
        assert!(validate_display_name(&name).is_err());
    }

    #[test]
    fn escape_ilike_escapes_percent() {
        assert_eq!(escape_ilike("100%"), "100\\%");
    }

    #[test]
    fn escape_ilike_escapes_underscore() {
        assert_eq!(escape_ilike("a_b"), "a\\_b");
    }

    #[test]
    fn escape_ilike_escapes_backslash() {
        assert_eq!(escape_ilike("a\\b"), "a\\\\b");
    }

    #[test]
    fn escape_ilike_leaves_normal_text_unchanged() {
        assert_eq!(escape_ilike("Alice"), "Alice");
    }
}
