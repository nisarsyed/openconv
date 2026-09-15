/// Maximum file upload size: 25 MB.
pub const MAX_FILE_SIZE_BYTES: usize = 25 * 1024 * 1024;
/// Maximum length for user display names.
pub const MAX_DISPLAY_NAME_LENGTH: usize = 64;
/// Maximum length for channel names.
pub const MAX_CHANNEL_NAME_LENGTH: usize = 100;
/// Maximum length for guild names.
pub const MAX_GUILD_NAME_LENGTH: usize = 100;
/// Maximum size for a single message in bytes.
pub const MAX_MESSAGE_SIZE_BYTES: usize = 8 * 1024;

// These hold at compile time, so assert them at compile time rather than in a
// `#[test]` — a runtime assertion over a `const` can never fail at runtime, and
// clippy rightly flags it as a constant-valued assertion.
const _: () = assert!(MAX_FILE_SIZE_BYTES > 0);
const _: () = assert!(MAX_DISPLAY_NAME_LENGTH > 0);
const _: () = assert!(MAX_CHANNEL_NAME_LENGTH > 0);
const _: () = assert!(MAX_GUILD_NAME_LENGTH > 0);
const _: () = assert!(MAX_MESSAGE_SIZE_BYTES > 0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_file_size_is_25mb() {
        assert_eq!(MAX_FILE_SIZE_BYTES, 25 * 1024 * 1024);
    }
}
