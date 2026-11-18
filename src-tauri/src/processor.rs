use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesRef, BytesText, Event};
use quick_xml::{Reader, Writer};
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct ProcessResult {
    pub output_path: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

pub fn escape_xml_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            other => output.push(other),
        }
    }
    output
}

pub fn escape_json(input: &str) -> String {
    let quoted = serde_json::to_string(input).unwrap_or_else(|_| "\"\"".to_string());
    quoted[1..quoted.len() - 1].to_string()
}

fn decode_entities(input: &str) -> String {
    match quick_xml::escape::unescape(input) {
        Ok(value) => value.into_owned(),
        Err(_) => input.to_string(),
    }
}

pub fn unescape_json(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        if let Ok(inner) = serde_json::from_str::<String>(trimmed) {
            return inner;
        }
    }

    let mut output = String::with_capacity(input.len());
    let mut characters = input.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match characters.next() {
            Some('"') => output.push('"'),
            Some('\\') => output.push('\\'),
            Some('/') => output.push('/'),
            Some('b') => output.push('\u{0008}'),
            Some('f') => output.push('\u{000c}'),
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some('u') => {
                let mut hex = String::new();
                for _ in 0..4 {
                    if let Some(next) = characters.next() {
                        hex.push(next);
                    }
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(decoded) => output.push(decoded),
                    None => {
                        output.push_str("\\u");
                        output.push_str(&hex);
                    }
                }
            }
            Some('x') => {
                let mut hex = String::new();
                for _ in 0..2 {
                    if let Some(next) = characters.next() {
                        hex.push(next);
                    }
                }
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(decoded) => output.push(decoded),
                    None => {
                        output.push_str("\\x");
                        output.push_str(&hex);
                    }
                }
            }
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }
    output
}

fn parse_json_value(source: &str, do_unescape: bool) -> Result<Value, String> {
    let mut attempts = vec![source.to_string()];
    if do_unescape {
        let unescaped = unescape_json(source);
        if unescaped != source {
            attempts.push(unescaped);
        }
        let decoded = decode_entities(attempts.last().map(String::as_str).unwrap_or(source));
        if decoded != source && !attempts.contains(&decoded) {
            attempts.push(decoded);
        }
    }

    let mut last_error = String::from("Invalid JSON");
    for attempt in attempts {
        match serde_json::from_str::<Value>(&attempt) {
            Ok(mut value) => {
                if do_unescape {
                    if let Value::String(inner) = &value {
                        if let Ok(parsed) = serde_json::from_str::<Value>(inner) {
                            value = parsed;
                        }
                    }
                }
                return Ok(value);
            }
            Err(error) => last_error = format!("Invalid JSON: {error}"),
        }
    }
    Err(last_error)
}

fn format_json_content(
    source: &str,
    indent: usize,
    do_format: bool,
    do_unescape: bool,
) -> Result<String, String> {
    if !do_format && !do_unescape {
        return Ok(source.to_string());
    }

    if !do_format {
        return Ok(unescape_json(source));
    }

    let value = parse_json_value(source, do_unescape)?;
    let indent_bytes = " ".repeat(indent.max(1)).into_bytes();
    let formatter = PrettyFormatter::with_indent(&indent_bytes);
    let mut buffer = Vec::new();
    {
        let mut serializer = serde_json::Serializer::with_formatter(&mut buffer, formatter);
        value
            .serialize(&mut serializer)
            .map_err(|error| error.to_string())?;
    }
    String::from_utf8(buffer).map_err(|error| error.to_string())
}

fn general_ref_text(reference: &BytesRef<'_>) -> String {
    if let Ok(Some(character)) = reference.resolve_char_ref() {
        return character.to_string();
    }
    let raw = format!("&{};", reference.as_ref());
    decode_entities(&raw)
}

fn write_xml_pretty<R: BufRead, W: Write>(
    reader: R,
    writer: W,
    indent: usize,
) -> Result<W, String> {
    let mut xml_reader = Reader::from_reader(reader);
    xml_reader.config_mut().trim_text(false);
    let mut xml_writer = Writer::new_with_indent(writer, b' ', indent.max(1));
    let mut buffer = Vec::new();
    let mut saw_root = false;

    loop {
        let event = xml_reader
            .read_event_into(&mut buffer)
            .map_err(|error| format!("Invalid XML: {error}"))?;

        match event {
            Event::Eof => break,
            Event::Text(text) => {
                let decoded = decode_entities(text.as_ref());
                let trimmed = decoded.trim();
                if !trimmed.is_empty() {
                    let escaped = escape_xml_text(trimmed);
                    xml_writer
                        .write_event(Event::Text(BytesText::from_escaped(&escaped)))
                        .map_err(|error| error.to_string())?;
                }
            }
            Event::GeneralRef(reference) => {
                let text = general_ref_text(&reference);
                let escaped = escape_xml_text(&text);
                xml_writer
                    .write_event(Event::Text(BytesText::from_escaped(&escaped)))
                    .map_err(|error| error.to_string())?;
            }
            other => {
                if matches!(other, Event::Start(_) | Event::Empty(_)) {
                    saw_root = true;
                }
                xml_writer
                    .write_event(other)
                    .map_err(|error| error.to_string())?;
            }
        }

        buffer.clear();
    }

    if !saw_root {
        return Err("Invalid XML: document has no root element".to_string());
    }

    Ok(xml_writer.into_inner())
}

fn format_xml_content(
    source: &str,
    indent: usize,
    do_format: bool,
    do_unescape: bool,
) -> Result<String, String> {
    if !do_format && !do_unescape {
        return Ok(source.to_string());
    }

    if !do_format {
        return Ok(decode_entities(source));
    }

    match write_xml_pretty(source.as_bytes(), Vec::new(), indent) {
        Ok(buffer) => String::from_utf8(buffer).map_err(|error| error.to_string()),
        Err(error) => {
            if do_unescape {
                let decoded = decode_entities(source);
                if decoded != source {
                    let buffer = write_xml_pretty(decoded.as_bytes(), Vec::new(), indent)?;
                    return String::from_utf8(buffer)
                        .map_err(|string_error| string_error.to_string());
                }
            }
            Err(error)
        }
    }
}

pub fn escape_content(source: &str, mode: &str) -> Result<String, String> {
    match mode {
        "json" => Ok(escape_json(source)),
        "xml" => Ok(escape_xml_text(source)),
        other => Err(format!("Unsupported mode: {other}")),
    }
}

pub fn process_content(
    source: &str,
    mode: &str,
    indent: usize,
    do_format: bool,
    do_unescape: bool,
    do_escape: bool,
) -> Result<String, String> {
    if do_escape {
        return escape_content(source, mode);
    }

    match mode {
        "json" => format_json_content(source, indent, do_format, do_unescape),
        "xml" => format_xml_content(source, indent, do_format, do_unescape),
        other => Err(format!("Unsupported mode: {other}")),
    }
}

fn stream_json_pretty(input: &Path, output: &Path, indent: usize) -> Result<(), String> {
    let reader = BufReader::new(File::open(input).map_err(|error| error.to_string())?);
    let value: Value =
        serde_json::from_reader(reader).map_err(|error| format!("Invalid JSON: {error}"))?;

    let mut writer = BufWriter::new(File::create(output).map_err(|error| error.to_string())?);
    let indent_bytes = " ".repeat(indent.max(1)).into_bytes();
    let formatter = PrettyFormatter::with_indent(&indent_bytes);
    {
        let mut serializer = serde_json::Serializer::with_formatter(&mut writer, formatter);
        value
            .serialize(&mut serializer)
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())?;
    Ok(())
}

fn stream_xml_pretty(input: &Path, output: &Path, indent: usize) -> Result<(), String> {
    let reader = BufReader::new(File::open(input).map_err(|error| error.to_string())?);
    let writer = BufWriter::new(File::create(output).map_err(|error| error.to_string())?);
    let mut writer = write_xml_pretty(reader, writer, indent)?;
    writer.flush().map_err(|error| error.to_string())?;
    Ok(())
}

fn write_temp(
    input: &Path,
    temp: &Path,
    mode: &str,
    indent: usize,
    do_format: bool,
    do_unescape: bool,
    do_escape: bool,
) -> Result<(), String> {
    if do_escape {
        let source = fs::read_to_string(input).map_err(|error| error.to_string())?;
        let escaped = escape_content(&source, mode)?;
        fs::write(temp, escaped).map_err(|error| error.to_string())?;
        return Ok(());
    }

    if !do_format && !do_unescape {
        fs::copy(input, temp).map_err(|error| error.to_string())?;
        return Ok(());
    }

    if !do_unescape {
        match mode {
            "json" if do_format => return stream_json_pretty(input, temp, indent),
            "xml" if do_format => return stream_xml_pretty(input, temp, indent),
            _ => {}
        }
    }

    let source = fs::read_to_string(input).map_err(|error| error.to_string())?;
    let output = process_content(&source, mode, indent, do_format, do_unescape, false)?;
    fs::write(temp, output).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn process_file(
    input_path: &str,
    output_path: &str,
    mode: &str,
    indent: usize,
    do_format: bool,
    do_unescape: bool,
    do_escape: bool,
) -> Result<ProcessResult, String> {
    let input = PathBuf::from(input_path);
    let destination = PathBuf::from(output_path);

    match mode {
        "json" | "xml" => {}
        other => return Err(format!("Unsupported mode: {other}")),
    }

    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }

    let file_name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());
    let temp = destination.with_file_name(format!(".{}.{}.tmp", file_name, std::process::id()));

    if let Err(error) = write_temp(
        &input,
        &temp,
        mode,
        indent,
        do_format,
        do_unescape,
        do_escape,
    ) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }

    fs::copy(&temp, &destination).map_err(|error| {
        let _ = fs::remove_file(&temp);
        error.to_string()
    })?;
    let _ = fs::remove_file(&temp);

    let bytes_in = fs::metadata(&input).map(|meta| meta.len()).unwrap_or(0);
    let bytes_out = fs::metadata(&destination)
        .map(|meta| meta.len())
        .unwrap_or(0);

    Ok(ProcessResult {
        output_path: destination.to_string_lossy().into_owned(),
        bytes_in,
        bytes_out,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn process_large_file(
    input_path: String,
    output_path: String,
    mode: String,
    indent: u8,
    do_format: bool,
    do_unescape: bool,
    do_escape: bool,
) -> Result<ProcessResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        process_file(
            &input_path,
            &output_path,
            &mode,
            indent as usize,
            do_format,
            do_unescape,
            do_escape,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_text_file(path: String, content: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    fs::write(path, content).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_json_with_exact_indent() {
        let source = r#"{"userId":1042,"roles":["admin","editor"]}"#;
        let output = process_content(source, "json", 3, true, false, false).expect("json");
        assert_eq!(
            output,
            "{\n   \"userId\": 1042,\n   \"roles\": [\n      \"admin\",\n      \"editor\"\n   ]\n}"
        );
    }

    #[test]
    fn unescapes_json_payloads() {
        let source = r#"{\"userId\":1042,\"name\":\"alex\"}"#;
        let output = process_content(source, "json", 2, true, true, false).expect("json");
        assert!(output.contains("\"userId\": 1042"));
        assert!(output.contains("\"name\": \"alex\""));
    }

    #[test]
    fn unescapes_json_strings_that_contain_json() {
        let source = r#""{\"a\":1}""#;
        let output = process_content(source, "json", 2, true, true, false).expect("json");
        assert_eq!(output, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(process_content("{invalid", "json", 2, true, true, false).is_err());
    }

    #[test]
    fn formats_xml_with_declaration_and_escaped_apostrophe() {
        let source = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><catalog><book id=\"bk101\"><author>Gambardella, Matthew</author><title>XML Developer's Guide</title></book></catalog>";
        let output = process_content(source, "xml", 2, true, false, false).expect("xml");
        assert_eq!(
			output,
			"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<catalog>\n  <book id=\"bk101\">\n    <author>Gambardella, Matthew</author>\n    <title>XML Developer&apos;s Guide</title>\n  </book>\n</catalog>"
		);
    }

    #[test]
    fn formats_escaped_xml_after_unescape() {
        let source = "&lt;root&gt;&lt;a&gt;1&lt;/a&gt;&lt;b/&gt;&lt;/root&gt;";
        let output = process_content(source, "xml", 2, true, true, false).expect("xml");
        assert_eq!(output, "<root>\n  <a>1</a>\n  <b/>\n</root>");
    }

    #[test]
    fn rejects_invalid_xml() {
        assert!(process_content("<root><a></root>", "xml", 2, true, false, false).is_err());
    }

    #[test]
    fn escapes_content_for_both_modes() {
        assert_eq!(
            escape_content("<a>1</a>", "xml").unwrap(),
            "&lt;a&gt;1&lt;/a&gt;"
        );
        assert_eq!(
            escape_content("{\"a\":1}", "json").unwrap(),
            "{\\\"a\\\":1}"
        );
    }

    #[test]
    fn unescape_json_handles_common_sequences() {
        assert_eq!(unescape_json(r#"a\nb\tc\"d"#), "a\nb\tc\"d");
        assert_eq!(unescape_json(r#""{\"a\":1}""#), "{\"a\":1}");
    }

    #[test]
    fn process_file_writes_output() {
        let unique = format!(
            "webdev-tools-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        fs::create_dir_all(&directory).expect("temp dir");
        let input = directory.join("input.json");
        let output = directory.join("output.json");
        fs::write(&input, r#"{"a":1,"b":[1,2]}"#).expect("write input");

        let result = process_file(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "json",
            2,
            true,
            false,
            false,
        )
        .expect("process");
        assert!(result.bytes_in > 0);
        let formatted = fs::read_to_string(&output).expect("read output");
        assert_eq!(
            formatted,
            "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}"
        );

        let _ = fs::remove_dir_all(&directory);
    }
}
