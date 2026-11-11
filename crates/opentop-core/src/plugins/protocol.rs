use serde::Deserialize;

use super::runtime::{MAX_LINE_SCALARS, MAX_LINES, PluginStatus};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProtocol {
    protocol_version: i64,
    lines: Vec<String>,
}

pub(super) fn parse_output(bytes: &[u8]) -> Result<Vec<String>, PluginStatus> {
    let text = std::str::from_utf8(bytes).map_err(|_| PluginStatus::InvalidOutput)?;
    let raw: RawProtocol = serde_json::from_str(text).map_err(|_| PluginStatus::InvalidOutput)?;
    if raw.protocol_version != 1 || raw.lines.len() > MAX_LINES {
        return Err(PluginStatus::InvalidOutput);
    }
    raw.lines
        .into_iter()
        .map(|line| {
            let clean = sanitize(&line);
            if clean.chars().count() > MAX_LINE_SCALARS {
                return Err(PluginStatus::InvalidOutput);
            }
            Ok(clean)
        })
        .collect()
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|c| {
            let code = *c as u32;
            !(code < 0x20 || (0x7f..=0x9f).contains(&code))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_single_object_and_strips_terminal_controls() {
        let output = br#"{"protocol_version":1,"lines":["hello\u001b[31m"]}"#;
        assert_eq!(parse_output(output).unwrap(), vec!["hello[31m"]);
        assert!(parse_output(br#"{"protocol_version":1,"lines":[]} {"extra":true}"#).is_err());
        assert!(parse_output(br#"{"protocol_version":2,"lines":[]}"#).is_err());
        assert!(parse_output(&[0xff, 0xfe]).is_err());
        let long_line = format!(
            "{{\"protocol_version\":1,\"lines\":[\"{}\"]}}",
            "x".repeat(MAX_LINE_SCALARS + 1)
        );
        assert!(parse_output(long_line.as_bytes()).is_err());
    }
}
