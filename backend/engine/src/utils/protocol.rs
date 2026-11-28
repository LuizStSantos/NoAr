use std::collections::BTreeMap;

use super::config::StreamType;

/// Validate the native transport without opening a socket.
pub fn validate_output_protocol_options(
    stream_type: StreamType,
    address: &str,
    options: &BTreeMap<String, String>,
) -> Result<(), String> {
    let Some((scheme, _)) = address.split_once("://") else {
        return if stream_type == StreamType::Custom && options.is_empty() {
            Ok(())
        } else {
            Err("network output requires a transport URL".to_string())
        };
    };
    let compatible = match stream_type {
        StreamType::Rtmp => matches!(scheme, "rtmp" | "rtmps"),
        StreamType::Srt => scheme == "srt",
        StreamType::Udp => scheme == "udp",
        StreamType::Custom => matches!(scheme, "rtmp" | "rtmps" | "srt" | "udp" | "tcp"),
    };
    if !compatible {
        return Err(format!(
            "output type {stream_type:?} is incompatible with transport {scheme:?}"
        ));
    }
    for (key, value) in options {
        if key.trim() != key || key.is_empty() || value.trim().is_empty() {
            return Err("protocol option names and values must not be empty".to_string());
        }
        if matches!(
            key.as_str(),
            "rw_timeout" | "timeout" | "listen_timeout" | "mode" | "listen"
        ) {
            return Err(format!(
                "protocol option {key:?} is managed by NoAr and cannot be overridden"
            ));
        }
    }
    crate::output::link::validate_transport_options(address, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_transport_even_without_custom_options() {
        assert!(
            validate_output_protocol_options(
                StreamType::Srt,
                "srt://127.0.0.1:8890",
                &BTreeMap::new()
            )
            .is_ok()
        );
        assert!(
            validate_output_protocol_options(
                StreamType::Srt,
                "udp://127.0.0.1:8890",
                &BTreeMap::new()
            )
            .is_err()
        );
        assert!(
            validate_output_protocol_options(
                StreamType::Custom,
                "rist://127.0.0.1:8890",
                &BTreeMap::new()
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_unknown_alias_conflicts_and_lifecycle_overrides() {
        for (url, key, value) in [
            ("srt://127.0.0.1:8890", "rw_timeout", "1"),
            ("srt://127.0.0.1:8890?latency=200000", "latency", "300000"),
            ("udp://127.0.0.1:8890", "pkt_size ", "1316"),
            ("udp://127.0.0.1:8890", "typo", "true"),
        ] {
            assert!(
                validate_output_protocol_options(
                    StreamType::Custom,
                    url,
                    &BTreeMap::from([(key.into(), value.into())])
                )
                .is_err()
            );
        }
    }
}
