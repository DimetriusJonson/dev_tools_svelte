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

struct JsonLayout {
    pad: String,
    parts: Vec<(usize, String)>,
    depth: usize,
    line_depth: usize,
    line: String,
    spacing: bool,
    in_string: bool,
    escaped: bool,
}

impl JsonLayout {
    fn new(indent: usize) -> Self {
        Self {
            pad: " ".repeat(indent.max(1)),
            parts: Vec::new(),
            depth: 0,
            line_depth: 0,
            line: String::new(),
            spacing: false,
            in_string: false,
            escaped: false,
        }
    }

    fn commit(&mut self) {
        let text = self.line.trim();
        if !text.is_empty() {
            self.parts.push((self.line_depth, text.to_string()));
        }
        self.line.clear();
        self.spacing = false;
    }

    fn start_line(&mut self) {
        self.commit();
        self.line_depth = self.depth;
    }

    fn pending_space(&mut self) {
        if self.spacing {
            self.line.push(' ');
            self.spacing = false;
        }
    }

    fn word(&mut self, character: char) {
        self.pending_space();
        self.line.push(character);
    }

    fn finish(mut self) -> String {
        self.commit();
        self.parts
            .iter()
            .map(|(depth, text)| format!("{}{}", self.pad.repeat(*depth), text))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/**
 * Re-indents JSON that `serde_json` rejects: structural characters break lines, other
 * content stays verbatim, and unclosed brackets are not invented. Mirrors `prettyJsonTolerant`.
 */
pub fn format_json_tolerant(source: &str, indent: usize) -> Option<String> {
    let trimmed = source.trim_start();
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return None;
    }

    let mut layout = JsonLayout::new(indent);
    for character in source.chars() {
        if layout.in_string {
            layout.line.push(character);
            if layout.escaped {
                layout.escaped = false;
            } else if character == '\\' {
                layout.escaped = true;
            } else if character == '"' {
                layout.in_string = false;
            }
            continue;
        }
        if character == '"' {
            layout.word(character);
            layout.in_string = true;
            continue;
        }
        if character.is_whitespace() {
            if character == '\n' || character == '\r' {
                layout.start_line();
                continue;
            }
            if !layout.line.trim().is_empty() {
                layout.spacing = true;
            }
            continue;
        }
        if character == '{' || character == '[' {
            layout.pending_space();
            layout.line.push(character);
            layout.depth += 1;
            layout.start_line();
            continue;
        }
        if character == '}' || character == ']' {
            layout.spacing = false;
            layout.depth = layout.depth.saturating_sub(1);
            layout.start_line();
            layout.line.push(character);
            continue;
        }
        if character == ',' {
            layout.spacing = false;
            layout.line.push(character);
            layout.start_line();
            continue;
        }
        if character == ':' {
            layout.spacing = false;
            layout.line.push(':');
            layout.spacing = true;
            continue;
        }
        layout.word(character);
    }

    Some(layout.finish())
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

/** Structural characters decide which unescape candidate is the better layout source. */
fn json_shape_score(source: &str) -> usize {
    source
        .chars()
        .filter(|character| matches!(character, '{' | '}' | '[' | ']' | '"'))
        .count()
}

/// Splits loose content before the first JSON bracket from the JSON body itself, so `aaa`
/// followed by a JSON document still gets formatted. Markup junk (`<`) is refused: an HTML
/// body has no JSON structure to re-indent. Mirrors `splitJsonJunk`.
fn split_json_junk(source: &str) -> Option<(&str, &str)> {
    let index = match (source.find('{'), source.find('[')) {
        (Some(brace), Some(bracket)) => brace.min(bracket),
        (Some(brace), None) => brace,
        (None, Some(bracket)) => bracket,
        (None, None) => return None,
    };
    let junk = &source[..index];
    let body = &source[index..];
    if junk.contains('<') {
        return None;
    }
    if !junk.trim().is_empty() && !body.contains('"') {
        return None;
    }
    Some((junk, body))
}

fn with_json_junk(junk: &str, body: String) -> String {
    let prefix = junk.trim_end();
    if prefix.is_empty() {
        body
    } else {
        format!("{prefix}\n{body}")
    }
}

fn json_tolerant_candidate(candidates: &[String]) -> Option<&String> {
    let mut best: Option<&String> = None;
    let mut best_score = 0usize;
    for candidate in candidates {
        if split_json_junk(candidate).is_none() {
            continue;
        }
        let score = json_shape_score(candidate);
        if best.is_none() || score >= best_score {
            best = Some(candidate);
            best_score = score;
        }
    }
    best
}

fn serialize_json_value(value: &Value, indent: usize) -> Result<String, String> {
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

fn json_unescape_candidates(source: &str, do_unescape: bool) -> Vec<String> {
    let mut candidates = vec![source.to_string()];
    if do_unescape {
        let unescaped = unescape_json(source);
        candidates.push(unescaped.clone());
        let decoded = decode_entities(&unescaped);
        if decoded != unescaped {
            candidates.push(decoded);
        }
    }
    candidates
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

    let strict_error = match parse_json_value(source, do_unescape) {
        Ok(value) => return serialize_json_value(&value, indent),
        Err(error) => error,
    };

    let candidates = json_unescape_candidates(source, do_unescape);
    // Junk around a real JSON document is kept as its own line; a broken body is re-indented.
    for candidate in &candidates {
        let Some((junk, body)) = split_json_junk(candidate) else {
            continue;
        };
        if let Ok(value) = parse_json_value(body, do_unescape) {
            return Ok(with_json_junk(junk, serialize_json_value(&value, indent)?));
        }
        if let Some(formatted) = format_json_tolerant(body, indent) {
            return Ok(with_json_junk(junk, formatted));
        }
    }

    Err(strict_error)
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
    let mut roots = 0usize;
    let mut depth = 0usize;
    let mut open_names: Vec<String> = Vec::new();

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
                    // Loose text outside the root is what makes the browser report an error
                    // too, so the strict path refuses it and the caller re-indents tolerantly.
                    if depth == 0 {
                        return Err("Invalid XML: content outside the root element".to_string());
                    }
                    let escaped = escape_xml_text(trimmed);
                    xml_writer
                        .write_event(Event::Text(BytesText::from_escaped(&escaped)))
                        .map_err(|error| error.to_string())?;
                }
            }
            Event::GeneralRef(reference) => {
                if depth == 0 {
                    return Err("Invalid XML: content outside the root element".to_string());
                }
                let text = general_ref_text(&reference);
                let escaped = escape_xml_text(&text);
                xml_writer
                    .write_event(Event::Text(BytesText::from_escaped(&escaped)))
                    .map_err(|error| error.to_string())?;
            }
            Event::Start(element) => {
                let name = element.local_name().as_ref().to_string();
                if depth == 0 {
                    roots += 1;
                    if roots > 1 {
                        return Err(
                            "Invalid XML: document has more than one root element".to_string()
                        );
                    }
                }
                depth += 1;
                open_names.push(name);
                xml_writer
                    .write_event(Event::Start(element))
                    .map_err(|error| error.to_string())?;
            }
            Event::End(element) => {
                let name = element.local_name().as_ref().to_string();
                match open_names.iter().rev().position(|open| *open == name) {
                    Some(offset) => {
                        open_names.truncate(open_names.len() - offset - 1);
                        depth -= offset + 1;
                    }
                    None => return Err(format!("Invalid XML: </{name}> has no matching element")),
                }
                xml_writer
                    .write_event(Event::End(element))
                    .map_err(|error| error.to_string())?;
            }
            Event::Empty(element) => {
                if depth == 0 {
                    roots += 1;
                    if roots > 1 {
                        return Err(
                            "Invalid XML: document has more than one root element".to_string()
                        );
                    }
                }
                xml_writer
                    .write_event(Event::Empty(element))
                    .map_err(|error| error.to_string())?;
            }
            other => {
                xml_writer
                    .write_event(other)
                    .map_err(|error| error.to_string())?;
            }
        }

        buffer.clear();
    }

    if roots == 0 {
        return Err("Invalid XML: document has no root element".to_string());
    }
    if depth != 0 {
        let name = open_names.last().cloned().unwrap_or_default();
        return Err(format!("Invalid XML: <{name}> is not closed"));
    }

    Ok(xml_writer.into_inner())
}

#[derive(Clone, Debug)]
enum XmlToken<'a> {
    Open {
        name: &'a str,
        attrs: &'a str,
        self_closing: bool,
    },
    Close(&'a str),
    Raw(&'a str),
    Text(&'a str),
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':'
}

fn is_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-')
}

/// Index of the `>` that ends a tag, ignoring `>` inside quoted attribute values.
fn tag_end(markup: &str, from: usize) -> Option<usize> {
    let bytes = markup.as_bytes();
    let mut quote: Option<u8> = None;
    let mut index = from;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(marker) => {
                if byte == marker {
                    quote = None;
                }
            }
            None => {
                if byte == b'"' || byte == b'\'' {
                    quote = Some(byte);
                } else if byte == b'>' {
                    return Some(index);
                }
            }
        }
        index += 1;
    }
    None
}

fn raw_token<'a>(
    markup: &'a str,
    index: usize,
    opener: &str,
    closer: &str,
) -> (XmlToken<'a>, usize) {
    let found = markup[index + opener.len()..].find(closer);
    let end = found
        .map(|offset| index + opener.len() + offset + closer.len())
        .unwrap_or(markup.len());
    (XmlToken::Raw(&markup[index..end]), end)
}

fn read_tag(markup: &str, index: usize) -> Option<(XmlToken<'_>, usize)> {
    let bytes = markup.as_bytes();
    let closing = markup[index..].starts_with("</");
    let start = index + if closing { 2 } else { 1 };
    if !is_name_start(bytes.get(start).copied().unwrap_or(b' ')) {
        return None;
    }

    let mut cursor = start;
    while cursor < bytes.len() && is_name_char(bytes[cursor]) {
        cursor += 1;
    }
    let name = &markup[start..cursor];
    let end = tag_end(markup, cursor)?;

    if closing {
        return Some((XmlToken::Close(name), end + 1));
    }

    let raw = &markup[index..end + 1];
    let self_closing = raw.ends_with("/>");
    let attrs = if self_closing {
        &raw[name.len() + 1..raw.len() - 2]
    } else {
        &raw[name.len() + 1..raw.len() - 1]
    };
    Some((
        XmlToken::Open {
            name,
            attrs,
            self_closing,
        },
        end + 1,
    ))
}

/// Reads one markup construct, or `None` when `<` at `index` is only text.
fn markup_token(markup: &str, index: usize) -> Option<(XmlToken<'_>, usize)> {
    let rest = &markup[index..];
    if rest.starts_with("<!--") {
        return Some(raw_token(markup, index, "<!--", "-->"));
    }
    if rest.starts_with("<![CDATA[") {
        return Some(raw_token(markup, index, "<![CDATA[", "]]>"));
    }
    if rest.starts_with("<?") {
        return Some(raw_token(markup, index, "<?", "?>"));
    }
    if rest.starts_with("<!") {
        return Some(raw_token(markup, index, "<!", ">"));
    }
    read_tag(markup, index)
}

/// Splits markup into tokens without validating it.
fn tokenize_xml(markup: &str) -> Vec<XmlToken<'_>> {
    let bytes = markup.as_bytes();
    let mut tokens: Vec<XmlToken<'_>> = Vec::new();
    let mut index = 0usize;
    let mut text_start = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'<' {
            if let Some((token, end)) = markup_token(markup, index) {
                if index > text_start {
                    tokens.push(XmlToken::Text(&markup[text_start..index]));
                }
                tokens.push(token);
                index = end;
                text_start = end;
                continue;
            }
        }
        index += 1;
    }

    if bytes.len() > text_start {
        tokens.push(XmlToken::Text(&markup[text_start..]));
    }
    tokens
}

fn markup_token_count(markup: &str) -> usize {
    tokenize_xml(markup)
        .iter()
        .filter(|token| !matches!(token, XmlToken::Text(_)))
        .count()
}

struct XmlNode {
    kind: XmlNodeType,
    children: Vec<usize>,
}

enum XmlNodeType {
    Text { text: String, verbatim: bool },
    Raw { text: String, block: bool },
    Element { name: String, attrs: String },
}

/**
 * Builds a tree from unvalidated tokens: unmatched closing tags pop the stack (unclosed
 * elements close where their parent does), and a stray closing tag stays as content.
 */
fn build_tolerant_tree(tokens: &[XmlToken<'_>]) -> (Vec<XmlNode>, Vec<usize>) {
    let mut arena: Vec<XmlNode> = Vec::new();
    let mut root: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for token in tokens {
        if let XmlToken::Close(name) = token {
            let mut matched: Option<usize> = None;
            for (position, &index) in stack.iter().enumerate().rev() {
                if let XmlNodeType::Element { name: element, .. } = &arena[index].kind {
                    if element == name {
                        matched = Some(position);
                        break;
                    }
                }
            }
            match matched {
                Some(position) => stack.truncate(position),
                None => {
                    let node = XmlNode {
                        kind: XmlNodeType::Raw {
                            text: format!("</{name}>"),
                            block: true,
                        },
                        children: Vec::new(),
                    };
                    let index = arena.len();
                    arena.push(node);
                    match stack.last().copied() {
                        Some(parent) => arena[parent].children.push(index),
                        None => root.push(index),
                    }
                }
            }
            continue;
        }

        let node = match token {
            XmlToken::Text(text) => XmlNode {
                kind: XmlNodeType::Text {
                    text: (*text).to_string(),
                    verbatim: true,
                },
                children: Vec::new(),
            },
            XmlToken::Raw(text) => XmlNode {
                kind: XmlNodeType::Raw {
                    text: (*text).to_string(),
                    block: text.starts_with("<!--"),
                },
                children: Vec::new(),
            },
            XmlToken::Open { name, attrs, .. } => XmlNode {
                kind: XmlNodeType::Element {
                    name: (*name).to_string(),
                    attrs: (*attrs).to_string(),
                },
                children: Vec::new(),
            },
            XmlToken::Close(_) => unreachable!("handled above"),
        };

        let index = arena.len();
        arena.push(node);
        match stack.last().copied() {
            Some(parent) => arena[parent].children.push(index),
            None => root.push(index),
        }
        if matches!(
            token,
            XmlToken::Open {
                self_closing: false,
                ..
            }
        ) {
            stack.push(index);
        }
    }

    (arena, root)
}

fn text_body(text: &str, verbatim: bool) -> String {
    let trimmed = text.trim();
    if verbatim {
        trimmed.to_string()
    } else {
        escape_xml_text(trimmed)
    }
}

fn serialize_virtual(
    arena: &[XmlNode],
    index: usize,
    depth: usize,
    pad: &str,
    block: bool,
) -> String {
    let node = &arena[index];
    let indent = pad.repeat(depth);

    match &node.kind {
        XmlNodeType::Text { text, verbatim } => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return if block { String::new() } else { text.clone() };
            }
            format!("{indent}{}", text_body(trimmed, *verbatim))
        }
        XmlNodeType::Raw { text, .. } => format!("{indent}{text}"),
        XmlNodeType::Element { name, attrs } => {
            let open = format!("<{name}{attrs}");
            let children: Vec<usize> = node
                .children
                .iter()
                .copied()
                .filter(|&child| match &arena[child].kind {
                    XmlNodeType::Text { text, .. } => !text.trim().is_empty(),
                    _ => true,
                })
                .collect();

            if children.is_empty() {
                return format!("{indent}{open}/>");
            }

            let has_block_child = children.iter().any(|&child| match &arena[child].kind {
                XmlNodeType::Element { .. } => true,
                XmlNodeType::Raw { block, .. } => *block,
                XmlNodeType::Text { .. } => false,
            });

            if !has_block_child {
                let inner = children
                    .iter()
                    .map(|&child| match &arena[child].kind {
                        XmlNodeType::Text { text, verbatim } => text_body(text, *verbatim),
                        XmlNodeType::Raw { text, .. } => text.clone(),
                        XmlNodeType::Element { .. } => String::new(),
                    })
                    .collect::<String>();
                return format!("{indent}{open}>{inner}</{name}>");
            }

            let inner = children
                .iter()
                .map(|&child| serialize_virtual(arena, child, depth + 1, pad, true))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}{open}>\n{inner}\n{indent}</{name}>")
        }
    }
}

/// Re-indents markup that `quick-xml` rejects. `None` when the text has no markup at all.
pub fn format_xml_tolerant(source: &str, indent: usize) -> Option<String> {
    let tokens = tokenize_xml(source);
    if tokens
        .iter()
        .all(|token| matches!(token, XmlToken::Text(_)))
    {
        return None;
    }

    let pad = " ".repeat(indent.max(1));
    let (arena, root) = build_tolerant_tree(&tokens);
    let parts: Vec<String> = root
        .iter()
        .map(|&child| {
            serialize_virtual(&arena, child, 0, &pad, false)
                .trim()
                .to_string()
        })
        .filter(|value| !value.is_empty())
        .collect();
    Some(parts.join("\n"))
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

    let mut candidates = vec![source.to_string()];
    if do_unescape {
        let decoded = decode_entities(source);
        if decoded != source {
            candidates.push(decoded);
        }
    }

    let mut last_error = String::from("Invalid XML: document has no root element");
    for candidate in &candidates {
        match write_xml_pretty(candidate.as_bytes(), Vec::new(), indent) {
            Ok(buffer) => return String::from_utf8(buffer).map_err(|error| error.to_string()),
            Err(error) => last_error = error,
        }
    }

    // Malformed markup is still worth formatting: re-indent the most markup-like candidate.
    let mut best: Option<&String> = None;
    let mut best_count = 0usize;
    for candidate in &candidates {
        let count = markup_token_count(candidate);
        if count > best_count {
            best_count = count;
            best = Some(candidate);
        }
    }
    if let Some(candidate) = best {
        if let Some(formatted) = format_xml_tolerant(candidate, indent) {
            return Ok(formatted);
        }
    }

    Err(last_error)
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
        let streamed = match mode {
            "json" if do_format => stream_json_pretty(input, temp, indent).is_ok(),
            "xml" if do_format => stream_xml_pretty(input, temp, indent).is_ok(),
            _ => false,
        };
        if streamed {
            return Ok(());
        }
    }

    // Streaming could not parse the file: re-read it and format it tolerantly in memory.
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
    fn formats_invalid_json_tolerantly() {
        assert_eq!(
            process_content("{invalid", "json", 2, true, true, false).expect("tolerant json"),
            "{\n  invalid"
        );
        assert_eq!(
            process_content(r#"{"a":1,}"#, "json", 2, true, false, false).expect("trailing comma"),
            "{\n  \"a\": 1,\n}"
        );
        assert_eq!(
            process_content("[{\"a\":[1,2]", "json", 2, true, false, false).expect("unclosed"),
            "[\n  {\n    \"a\": [\n      1,\n      2\n    ]"
        );
    }

    #[test]
    fn formats_json_with_junk_before_the_document() {
        let source = "aaa\n{\"userId\":1042,\"isActive\":true,\"roles\":[\"admin\",\"editor\"]}";
        assert_eq!(
            process_content(source, "json", 2, true, false, false).expect("junk prefix"),
            "aaa\n{\n  \"userId\": 1042,\n  \"isActive\": true,\n  \"roles\": [\n    \"admin\",\n    \"editor\"\n  ]\n}"
        );
        assert_eq!(
            process_content("{\"a\":1,,}\nbbb", "json", 2, true, false, false)
                .expect("broken body"),
            "{\n  \"a\": 1,\n  ,\n}\nbbb"
        );
    }

    #[test]
    fn rejects_text_that_is_not_json() {
        assert!(process_content("hello world", "json", 2, true, true, false).is_err());
        assert!(process_content("<!DOCTYPE html><html>", "json", 2, true, true, false).is_err());
        assert!(process_content(
            "<html><body>{ a }</body></html>",
            "json",
            2,
            true,
            false,
            false
        )
        .is_err());
        assert!(
            process_content("Error occurred in {module}", "json", 2, true, false, false).is_err()
        );
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
    fn formats_invalid_xml_tolerantly() {
        let source = "aaa\n<?xml version=\"1.0\" encoding=\"UTF-8\"?><catalog><book id=\"bk101\"><title>XML Developer's Guide</title></book></catalog>";
        let output = process_content(source, "xml", 2, true, false, false).expect("tolerant xml");
        assert_eq!(
            output,
            "aaa\n<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<catalog>\n  <book id=\"bk101\">\n    <title>XML Developer's Guide</title>\n  </book>\n</catalog>"
        );
        assert_eq!(
            process_content("<root><a></root>", "xml", 2, true, false, false).expect("mismatched"),
            "<root>\n  <a/>\n</root>"
        );
        assert_eq!(
            process_content("<a><b><c>d", "xml", 2, true, false, false).expect("unclosed"),
            "<a>\n  <b>\n    <c>d</c>\n  </b>\n</a>"
        );
        assert_eq!(
            process_content("&lt;root&gt;&lt;a&gt;1", "xml", 2, true, true, false)
                .expect("escaped"),
            "<root>\n  <a>1</a>\n</root>"
        );
    }

    #[test]
    fn keeps_invalid_xml_markup_verbatim_in_tolerant_mode() {
        let output = process_content(
            "<a title=\"x &amp; y\">a &lt; b</a><oops>",
            "xml",
            2,
            true,
            false,
            false,
        )
        .expect("tolerant xml");
        assert_eq!(output, "<a title=\"x &amp; y\">a &lt; b</a>\n<oops/>");
    }

    #[test]
    fn rejects_text_that_is_not_xml() {
        assert!(process_content("hello world", "xml", 2, true, true, false).is_err());
        assert!(process_content("&lt;root&gt;&lt;a&gt;1", "xml", 2, true, false, false).is_err());
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

    #[test]
    fn process_file_formats_invalid_markup() {
        let unique = format!(
            "webdev-tools-tolerant-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        fs::create_dir_all(&directory).expect("temp dir");
        let input = directory.join("input.xml");
        let output = directory.join("output.xml");
        fs::write(&input, "<root><a></root>").expect("write input");

        let result = process_file(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "xml",
            2,
            true,
            false,
            false,
        )
        .expect("tolerant file format");
        assert!(result.bytes_out > result.bytes_in);
        assert_eq!(
            fs::read_to_string(&output).expect("read output"),
            "<root>\n  <a/>\n</root>"
        );

        let _ = fs::remove_dir_all(&directory);
    }
}
