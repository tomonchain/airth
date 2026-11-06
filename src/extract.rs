//! The text of a Word document or a PDF (decision 86), for an agent to read
//! a company document. A .docx is a zip of XML; a PDF made by software
//! keeps its text in compressed streams, mapped to characters by its fonts'
//! ToUnicode tables. A scanned PDF has no text: that is said, not guessed.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::io::Read;

/// The most a compressed part may grow to: a document, not a zip bomb.
const MAX_INFLATED: u64 = 50 * 1024 * 1024;

/// The text of `bytes`, by its media type: Word (.docx) or PDF.
pub fn text(bytes: &[u8], mime: &str) -> Result<String> {
    let text = match mime {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => docx(bytes)?,
        "application/pdf" => pdf(bytes),
        other => bail!("a {other} file has no text Airth can read"),
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        bail!(
            "no text found in it: most likely a scan (an image of the pages), which Airth cannot read yet"
        );
    }
    Ok(text)
}

// ---- Word ----

fn docx(bytes: &[u8]) -> Result<String> {
    use quick_xml::escape::resolve_predefined_entity;
    use quick_xml::events::Event;
    let xml = zip_entry(bytes, "word/document.xml")?
        .context("not a Word document: no word/document.xml")?;
    let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
    let name = |n: quick_xml::name::QName| n.into_inner().to_string();
    let mut out = String::new();
    let mut in_text = false;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if name(e.name()) == "w:t" => in_text = true,
            Ok(Event::End(e)) => match name(e.name()).as_str() {
                "w:t" => in_text = false,
                "w:p" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Empty(e)) => match name(e.name()).as_str() {
                "w:tab" => out.push('\t'),
                "w:br" => out.push('\n'),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => out.push_str(&t.xml10_content()),
            Ok(Event::GeneralRef(r)) if in_text => match r.resolve_char_ref() {
                Ok(Some(c)) => out.push(c),
                _ => {
                    out.push_str(resolve_predefined_entity(&r.xml10_content()).unwrap_or_default())
                }
            },
            Ok(Event::Eof) => break,
            Err(e) => bail!("this Word document's text is not well-formed: {e}"),
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

/// The entry `name` of a zip archive, inflated; None when it has none.
fn zip_entry(zip: &[u8], name: &str) -> Result<Option<Vec<u8>>> {
    let u16_at = |at: usize| {
        zip.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let u32_at = |at: usize| {
        zip.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    // The end of the central directory, within the last 64 KB.
    let end = (zip.len().saturating_sub(65_557)..zip.len().saturating_sub(21))
        .rev()
        .find(|&i| zip[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .context("not a zip archive")?;
    let count = u16_at(end + 10).context("a broken zip")?;
    let mut at = u32_at(end + 16).context("a broken zip")?;
    for _ in 0..count {
        if zip.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            bail!("a broken zip directory");
        }
        let method = u16_at(at + 10).context("a broken zip")?;
        let size = u32_at(at + 20).context("a broken zip")?;
        let (name_len, extra, comment) = (
            u16_at(at + 28).context("a broken zip")?,
            u16_at(at + 30).context("a broken zip")?,
            u16_at(at + 32).context("a broken zip")?,
        );
        let local = u32_at(at + 42).context("a broken zip")?;
        let entry = zip
            .get(at + 46..at + 46 + name_len)
            .context("a broken zip")?;
        at += 46 + name_len + extra + comment;
        if entry != name.as_bytes() {
            continue;
        }
        let data_at = local
            + 30
            + u16_at(local + 26).context("a broken zip")?
            + u16_at(local + 28).context("a broken zip")?;
        let data = zip
            .get(data_at..data_at + size)
            .context("a broken zip entry")?;
        return Ok(Some(match method {
            0 => data.to_vec(),
            8 => inflate(flate2::read::DeflateDecoder::new(data))?,
            m => bail!("a zip entry compressed in a way Airth does not read ({m})"),
        }));
    }
    Ok(None)
}

fn inflate(reader: impl Read) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    reader
        .take(MAX_INFLATED)
        .read_to_end(&mut out)
        .context("cannot inflate it")?;
    Ok(out)
}

// ---- PDF ----

/// The text of the PDF's pages, in the order its streams come. Characters
/// are mapped by the fonts' ToUnicode tables when it has any, else read as
/// Latin-1.
fn pdf(bytes: &[u8]) -> String {
    let streams = streams(bytes);
    let mut cmap = CMap::default();
    for s in streams
        .iter()
        .filter(|s| contains(s, b"beginbfchar") || contains(s, b"beginbfrange"))
    {
        cmap.read(s);
    }
    let mut out = String::new();
    for s in streams
        .iter()
        .filter(|s| contains(s, b"BT") && contains(s, b"ET"))
    {
        page_text(s, &cmap, &mut out);
        out.push('\n');
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// Every stream of the file, inflated when it is FlateDecode.
fn streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = find(pdf, b"stream", at) {
        at = start + 6;
        // "endstream" also holds "stream": only a stream's beginning counts.
        if start >= 3 && &pdf[start - 3..start] == b"end" {
            continue;
        }
        let mut body = at;
        if pdf.get(body) == Some(&b'\r') {
            body += 1;
        }
        if pdf.get(body) == Some(&b'\n') {
            body += 1;
        }
        let Some(end) = find(pdf, b"endstream", body) else {
            break;
        };
        let dict_from = pdf[..start]
            .windows(2)
            .rposition(|w| w == b"<<")
            .unwrap_or(0);
        let flate = contains(&pdf[dict_from..start], b"FlateDecode");
        let data = &pdf[body..end];
        at = end + 9;
        if flate {
            if let Ok(inflated) = inflate(flate2::read::ZlibDecoder::new(data)) {
                out.push(inflated);
            }
        } else {
            out.push(data.to_vec());
        }
    }
    out
}

/// The fonts' character maps, merged: codes to text.
#[derive(Default)]
struct CMap {
    map: HashMap<Vec<u8>, String>,
    two_bytes: bool,
}

impl CMap {
    fn read(&mut self, s: &[u8]) {
        let tokens = tokens(s);
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                Token::Op(op) if op == "beginbfchar" => {
                    i += 1;
                    while let (Some(Token::Hex(code)), Some(Token::Hex(to))) =
                        (tokens.get(i), tokens.get(i + 1))
                    {
                        self.put(code.clone(), utf16(to));
                        i += 2;
                    }
                }
                Token::Op(op) if op == "beginbfrange" => {
                    i += 1;
                    while let (Some(Token::Hex(lo)), Some(Token::Hex(hi))) =
                        (tokens.get(i), tokens.get(i + 1))
                    {
                        let (lo_n, hi_n) = (number(lo), number(hi));
                        match tokens.get(i + 2) {
                            Some(Token::Hex(to)) => {
                                let base = number(to);
                                for (k, code) in (lo_n..=hi_n.min(lo_n + 0xFFFF)).enumerate() {
                                    let unit = (base + k as u32) as u16;
                                    self.put(
                                        bytes_of(code, lo.len()),
                                        String::from_utf16_lossy(&[unit]),
                                    );
                                }
                            }
                            Some(Token::Array(list)) => {
                                for (k, to) in list.iter().enumerate() {
                                    if let Token::Hex(to) = to {
                                        self.put(bytes_of(lo_n + k as u32, lo.len()), utf16(to));
                                    }
                                }
                            }
                            _ => break,
                        }
                        i += 3;
                    }
                }
                _ => i += 1,
            }
        }
    }

    fn put(&mut self, code: Vec<u8>, to: String) {
        self.two_bytes |= code.len() == 2;
        self.map.insert(code, to);
    }

    fn decode(&self, s: &[u8]) -> String {
        if self.map.is_empty() {
            return s.iter().map(|&b| b as char).collect();
        }
        let width = if self.two_bytes { 2 } else { 1 };
        s.chunks(width)
            .map(|c| match self.map.get(c) {
                Some(t) => t.clone(),
                None if width == 1 => (c[0] as char).to_string(),
                None => String::new(),
            })
            .collect()
    }
}

fn number(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |n, &x| (n << 8) | x as u32)
}

fn bytes_of(n: u32, len: usize) -> Vec<u8> {
    (0..len).rev().map(|i| (n >> (8 * i)) as u8).collect()
}

fn utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
        .collect();
    String::from_utf16_lossy(&units)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Str(Vec<u8>),
    Hex(Vec<u8>),
    Num(f64),
    Array(Vec<Token>),
    Op(String),
}

/// The text operators of a page: strings shown, with a space for a wide
/// gap and a new line where the text moves down.
fn page_text(s: &[u8], cmap: &CMap, out: &mut String) {
    let tokens = tokens(s);
    for (i, t) in tokens.iter().enumerate() {
        let Token::Op(op) = t else { continue };
        let prev = i.checked_sub(1).and_then(|p| tokens.get(p));
        match (op.as_str(), prev) {
            ("Tj" | "'" | "\"", Some(Token::Str(b) | Token::Hex(b))) => {
                if op != "Tj" {
                    out.push('\n');
                }
                out.push_str(&cmap.decode(b));
            }
            ("TJ", Some(Token::Array(parts))) => {
                for p in parts {
                    match p {
                        Token::Str(b) | Token::Hex(b) => out.push_str(&cmap.decode(b)),
                        Token::Num(n) if *n < -180.0 => out.push(' '),
                        _ => {}
                    }
                }
            }
            ("Td" | "TD", Some(Token::Num(y))) if *y != 0.0 => out.push('\n'),
            ("T*", _) => out.push('\n'),
            ("ET", _) if !out.ends_with(['\n', ' ']) => out.push(' '),
            _ => {}
        }
    }
}

/// A PDF content stream as tokens; dictionaries and names are skipped.
fn tokens(s: &[u8]) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut stack: Vec<Vec<Token>> = Vec::new();
    let mut i = 0;
    let push = |t: Token, out: &mut Vec<Token>, stack: &mut Vec<Vec<Token>>| match stack.last_mut()
    {
        Some(a) => a.push(t),
        None => out.push(t),
    };
    while i < s.len() {
        match s[i] {
            b'(' => {
                let mut depth = 1;
                let mut b = Vec::new();
                i += 1;
                while i < s.len() && depth > 0 {
                    match s[i] {
                        b'\\' if i + 1 < s.len() => {
                            i += 1;
                            match s[i] {
                                b'n' => b.push(b'\n'),
                                b'r' => b.push(b'\r'),
                                b't' => b.push(b'\t'),
                                b'0'..=b'7' => {
                                    let mut v = 0u32;
                                    let mut k = 0;
                                    while k < 3 && i < s.len() && (b'0'..=b'7').contains(&s[i]) {
                                        v = v * 8 + (s[i] - b'0') as u32;
                                        i += 1;
                                        k += 1;
                                    }
                                    i -= 1;
                                    b.push(v as u8);
                                }
                                b'\r' | b'\n' => {}
                                c => b.push(c),
                            }
                        }
                        b'(' => {
                            depth += 1;
                            b.push(b'(');
                        }
                        b')' => {
                            depth -= 1;
                            if depth > 0 {
                                b.push(b')');
                            }
                        }
                        c => b.push(c),
                    }
                    i += 1;
                }
                push(Token::Str(b), &mut out, &mut stack);
            }
            b'<' if s.get(i + 1) == Some(&b'<') => i += 2,
            b'>' if s.get(i + 1) == Some(&b'>') => i += 2,
            b'<' => {
                let end = find(s, b">", i).unwrap_or(s.len());
                let hex: Vec<u8> = s[i + 1..end]
                    .iter()
                    .filter(|c| c.is_ascii_hexdigit())
                    .copied()
                    .collect();
                let mut bytes: Vec<u8> = hex
                    .chunks(2)
                    .map(|c| {
                        u8::from_str_radix(
                            &String::from_utf8_lossy(&[c[0], *c.get(1).unwrap_or(&b'0')]),
                            16,
                        )
                        .unwrap_or(0)
                    })
                    .collect();
                // An odd digit is the high half of its byte.
                if hex.len() % 2 == 1
                    && let Some(b) = bytes.last_mut()
                {
                    *b &= 0xF0;
                }
                push(Token::Hex(bytes), &mut out, &mut stack);
                i = end + 1;
            }
            b'[' => {
                stack.push(Vec::new());
                i += 1;
            }
            b']' => {
                let a = stack.pop().unwrap_or_default();
                push(Token::Array(a), &mut out, &mut stack);
                i += 1;
            }
            b'%' => {
                while i < s.len() && s[i] != b'\n' && s[i] != b'\r' {
                    i += 1;
                }
            }
            b'/' => {
                i += 1;
                while i < s.len() && !b" \t\r\n/[]()<>{}%".contains(&s[i]) {
                    i += 1;
                }
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let from = i;
                while i < s.len() && !b" \t\r\n/[]()<>{}%".contains(&s[i]) {
                    i += 1;
                }
                if i == from {
                    i += 1;
                    continue;
                }
                let word = String::from_utf8_lossy(&s[from..i]).to_string();
                let token = match word.parse::<f64>() {
                    Ok(n) => Token::Num(n),
                    Err(_) => Token::Op(word),
                };
                push(token, &mut out, &mut stack);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
