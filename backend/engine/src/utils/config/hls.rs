//! HLS rendition parsing and subtitle track metadata validation.

use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsVariant {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub video_bitrate: u64,
    pub audio_bitrate: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsSubtitle {
    pub name: String,
    pub language: String,
    pub default: bool,
}

impl HlsSubtitle {
    pub fn validate(&self) -> Result<(), String> {
        validate_stream_map_value("subtitle name", &self.name)?;
        validate_stream_map_value("subtitle language", &self.language)
    }
}

fn validate_stream_map_value(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{label} must not be empty"));
    }

    if value.chars().any(|ch| ch.is_whitespace() || ch == ',') {
        return Err(format!("{label} must not contain whitespace or ','"));
    }

    Ok(())
}

impl FromStr for HlsVariant {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split(':');
        let name = parts
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| "missing variant name".to_string())?;
        if !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            return Err(
                "variant name may only contain ASCII letters, numbers, '_' and '-'".to_string(),
            );
        }

        let resolution = parts
            .next()
            .ok_or_else(|| "missing variant resolution".to_string())?;
        let video_bitrate = parts
            .next()
            .ok_or_else(|| "missing variant video bitrate".to_string())?;
        let audio_bitrate = parts.next().unwrap_or("128k");

        if parts.next().is_some() {
            return Err("expected NAME:WIDTHxHEIGHT:VIDEO_BITRATE[:AUDIO_BITRATE]".to_string());
        }

        let (width, height) = resolution
            .split_once('x')
            .ok_or_else(|| "resolution must use WIDTHxHEIGHT".to_string())?;
        let width = width
            .parse::<u32>()
            .map_err(|_| "width must be a positive integer".to_string())?;
        let height = height
            .parse::<u32>()
            .map_err(|_| "height must be a positive integer".to_string())?;
        if width == 0 || height == 0 {
            return Err("width and height must be greater than zero".to_string());
        }

        Ok(Self {
            name: name.to_string(),
            width,
            height,
            video_bitrate: parse_bitrate(video_bitrate)?,
            audio_bitrate: parse_bitrate(audio_bitrate)?,
        })
    }
}

fn parse_bitrate(value: &str) -> Result<u64, String> {
    let value = value.trim();

    if value.is_empty() {
        return Err("bitrate must not be empty".to_string());
    }

    let (number, multiplier) = match value.as_bytes().last().copied() {
        Some(b'k') | Some(b'K') => (&value[..value.len() - 1], 1_000),
        Some(b'm') | Some(b'M') => (&value[..value.len() - 1], 1_000_000),
        _ => (value, 1),
    };
    let number = number
        .parse::<u64>()
        .map_err(|_| format!("invalid bitrate {value:?}"))?;
    if number == 0 {
        return Err("bitrate must be greater than zero".to_string());
    }

    Ok(number * multiplier)
}

#[cfg(test)]
mod hls_subtitle_tests {
    use super::HlsSubtitle;

    #[test]
    fn accepts_stream_map_safe_metadata() {
        assert!(
            HlsSubtitle {
                name: "Deutsch".to_string(),
                language: "de-DE".to_string(),
                default: false,
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn rejects_values_that_break_stream_map() {
        for name in ["", "Deutsch SD", "Deutsch,SD"] {
            assert!(
                HlsSubtitle {
                    name: name.to_string(),
                    language: "de-DE".to_string(),
                    default: false,
                }
                .validate()
                .is_err()
            );
        }
    }
}
