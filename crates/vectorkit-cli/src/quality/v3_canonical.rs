use std::fs;
use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn parse_canonical_json(path: &Path, bytes: &[u8]) -> Result<Value, String> {
    validate_text_bytes(path, bytes, false)?;
    let value: Value = serde_json::from_slice(&bytes[..bytes.len() - 1])
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))?;
    let encoded = canonical_json_line(&value)?;
    compare_bytes(path, bytes, &encoded)?;
    Ok(value)
}

pub(super) fn parse_canonical_jsonl(path: &Path, bytes: &[u8]) -> Result<Vec<Value>, String> {
    validate_text_bytes(path, bytes, true)?;
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    for (offset, line) in bytes[..bytes.len() - 1]
        .split(|byte| *byte == b'\n')
        .enumerate()
    {
        if line.is_empty() {
            return Err(format!("{}: row {} is blank", path.display(), offset + 1));
        }
        let value: Value = serde_json::from_slice(line).map_err(|error| {
            format!(
                "{}: row {}: invalid JSON: {error}",
                path.display(),
                offset + 1
            )
        })?;
        if !value.is_object() {
            return Err(format!(
                "{}: row {}: expected a JSON object, actual {}",
                path.display(),
                offset + 1,
                kind(&value)
            ));
        }
        let encoded = canonical_json(&value)?;
        if encoded.as_bytes() != line {
            let difference = first_difference(line, encoded.as_bytes());
            return Err(format!(
                "{}: row {} is not canonical JSON; first differing byte {}",
                path.display(),
                offset + 1,
                difference
            ));
        }
        rows.push(value);
    }
    Ok(rows)
}

pub(super) fn canonical_json_line(value: &Value) -> Result<Vec<u8>, String> {
    let mut output = canonical_json(value)?.into_bytes();
    output.push(b'\n');
    Ok(output)
}

pub(super) fn canonical_json(value: &Value) -> Result<String, String> {
    let mut output = String::new();
    encode_value(value, &mut output)?;
    Ok(output)
}

pub(super) fn write_canonical_json(path: &Path, value: &Value) -> Result<(), String> {
    fs::write(path, canonical_json_line(value)?)
        .map_err(|error| format!("failed to write '{}': {error}", path.display()))
}

pub(super) fn validate_text_bytes(
    path: &Path,
    bytes: &[u8],
    allow_empty: bool,
) -> Result<(), String> {
    if bytes.is_empty() {
        return if allow_empty {
            Ok(())
        } else {
            Err(format!("{}: JSON file must not be empty", path.display()))
        };
    }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(format!(
            "{}: UTF-8 byte-order mark is forbidden",
            path.display()
        ));
    }
    std::str::from_utf8(bytes)
        .map_err(|error| format!("{}: invalid UTF-8: {error}", path.display()))?;
    if bytes.contains(&b'\r') {
        return Err(format!(
            "{}: CR bytes are forbidden; expected LF line endings",
            path.display()
        ));
    }
    if !bytes.ends_with(b"\n") {
        return Err(format!(
            "{}: expected exactly one terminating LF",
            path.display()
        ));
    }
    if bytes.ends_with(b"\n\n") {
        return Err(format!(
            "{}: expected exactly one terminating LF",
            path.display()
        ));
    }
    Ok(())
}

pub(super) fn compare_bytes(path: &Path, actual: &[u8], expected: &[u8]) -> Result<(), String> {
    if actual == expected {
        return Ok(());
    }
    Err(format!(
        "{}: canonical byte mismatch at offset {} (actual bytes {}, expected bytes {})",
        path.display(),
        first_difference(actual, expected),
        actual.len(),
        expected.len()
    ))
}

pub(super) fn first_difference(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .unwrap_or(left.len().min(right.len()))
}

fn encode_value(value: &Value, output: &mut String) -> Result<(), String> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => {
            let text = value.to_string();
            if text == "-0" || text == "-0.0" || value.as_f64() == Some(0.0) {
                output.push('0');
            } else {
                output.push_str(&canonical_number(&text)?);
            }
        }
        Value::String(value) => encode_string(value, output),
        Value::Array(values) => {
            output.push('[');
            for (offset, value) in values.iter().enumerate() {
                if offset > 0 {
                    output.push(',');
                }
                encode_value(value, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
            for (offset, (key, value)) in entries.into_iter().enumerate() {
                if offset > 0 {
                    output.push(',');
                }
                encode_string(key, output);
                output.push(':');
                encode_value(value, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn canonical_number(text: &str) -> Result<String, String> {
    let value = text
        .parse::<f64>()
        .map_err(|error| format!("invalid finite JSON number '{text}': {error}"))?;
    if !value.is_finite() {
        return Err(format!("non-finite JSON number '{text}' is forbidden"));
    }
    let mut result = text.replace("e+", "e").replace('E', "e");
    if let Some(index) = result.find('e') {
        let exponent = result[index + 1..]
            .parse::<i32>()
            .map_err(|_| format!("invalid exponent in '{text}'"))?;
        let mantissa = &result[..index];
        if (-6..=20).contains(&exponent) {
            result = scientific_to_plain(mantissa, exponent)?;
        } else {
            let sign = if exponent < 0 { "-" } else { "" };
            result = format!("{}e{}{}", trim_fraction(mantissa), sign, exponent.abs());
        }
    } else {
        result = trim_fraction(&result);
    }
    Ok(result)
}

fn scientific_to_plain(mantissa: &str, exponent: i32) -> Result<String, String> {
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.trim_start_matches('-');
    let digits = unsigned.replace('.', "");
    let decimal = 1_i32 + exponent;
    let plain = if decimal <= 0 {
        format!("0.{}{}", "0".repeat((-decimal) as usize), digits)
    } else if decimal as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(decimal as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..decimal as usize],
            &digits[decimal as usize..]
        )
    };
    let plain = trim_fraction(&plain);
    Ok(if negative { format!("-{plain}") } else { plain })
}

fn trim_fraction(value: &str) -> String {
    if !value.contains('.') {
        return value.to_owned();
    }
    let trimmed = value.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn encode_string(value: &str, output: &mut String) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{0008}' => output.push_str("\\b"),
            '\t' => output.push_str("\\t"),
            '\n' => output.push_str("\\n"),
            '\u{000c}' => output.push_str("\\f"),
            '\r' => output.push_str("\\r"),
            value if value <= '\u{001f}' => output.push_str(&format!("\\u{:04x}", value as u32)),
            value => output.push(value),
        }
    }
    output.push('"');
}

pub(super) fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_encoding_orders_and_escapes_exactly() {
        let value = json!({"z":"/\u{2028}","a":"\u{0000}\u{0008}\t\n\u{000c}\r\"\\"});
        assert_eq!(
            canonical_json_line(&value).unwrap(),
            b"{\"a\":\"\\u0000\\b\\t\\n\\f\\r\\\"\\\\\",\"z\":\"/\xe2\x80\xa8\"}\n"
        );
    }

    #[test]
    fn canonical_encoding_preserves_array_order_and_normalizes_numbers() {
        let value: Value = serde_json::from_str("[2,1,-0.0,1e-7,1e20,1e21]").unwrap();
        assert_eq!(
            canonical_json(&value).unwrap(),
            "[2,1,0,1e-7,100000000000000000000,1e21]"
        );
    }

    #[test]
    fn zero_row_jsonl_is_zero_bytes() {
        assert!(parse_canonical_jsonl(Path::new("empty.jsonl"), b"")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rejects_semantically_valid_noncanonical_json() {
        let error = parse_canonical_json(Path::new("bad.json"), b"{ \"a\": 1 }\n").unwrap_err();
        assert!(error.contains("canonical byte mismatch"));
    }
}
