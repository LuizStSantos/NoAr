//! Container metadata bounds checked before opening an output.

use std::collections::BTreeMap;

/// Validate untrusted global container tags before they reach libavformat.
/// Tag names are intentionally not format-specific: muxers decide which tags
/// they can represent.
pub fn validate_output_metadata(metadata: &BTreeMap<String, String>) -> Result<(), String> {
    if metadata.len() > 64 {
        return Err("output metadata must contain at most 64 entries".to_string());
    }

    let mut total_bytes = 0usize;

    for (key, value) in metadata {
        if key.trim().is_empty() || key.len() > 128 || key.chars().any(char::is_control) {
            return Err(format!("invalid output metadata key {key:?}"));
        }

        if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(format!("invalid output metadata value for {key:?}"));
        }

        total_bytes += key.len() + value.len();

        if total_bytes > 16_384 {
            return Err("output metadata exceeds 16 KiB".to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod output_metadata_tests {
    use super::*;

    #[test]
    fn accepts_generic_container_tags() {
        let tags = BTreeMap::from([
            ("copyright".to_string(), "© 2026 Example".to_string()),
            ("service_name".to_string(), "Channel One".to_string()),
            ("WM/Year".to_string(), "2026".to_string()),
        ]);
        assert!(validate_output_metadata(&tags).is_ok());
        assert!(validate_output_metadata(&BTreeMap::new()).is_ok());
    }

    #[test]
    fn rejects_unsafe_or_excessive_tags() {
        for (key, value) in [
            ("", "value"),
            ("bad\nkey", "value"),
            ("title", ""),
            ("title", "line\nbreak"),
            ("title", "nul\0byte"),
        ] {
            assert!(
                validate_output_metadata(&BTreeMap::from([(key.into(), value.into())])).is_err()
            );
        }
        assert!(
            validate_output_metadata(&BTreeMap::from([("title".into(), "x".repeat(4097))]))
                .is_err()
        );
    }
}
