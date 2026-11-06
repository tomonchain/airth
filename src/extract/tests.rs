use super::*;
use std::io::Write;

/// A zip of `files`, each stored or deflated, as a .docx is.
fn zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let (mut out, mut dir) = (Vec::new(), Vec::new());
    for (name, data, deflate) in files {
        let body = if *deflate {
            let mut e =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        } else {
            data.to_vec()
        };
        let method: u16 = if *deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0]);
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 8]); // time, date, crc
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&body);
        dir.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0]);
        dir.extend_from_slice(&method.to_le_bytes());
        dir.extend_from_slice(&[0; 8]);
        dir.extend_from_slice(&(body.len() as u32).to_le_bytes());
        dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        dir.extend_from_slice(&(name.len() as u16).to_le_bytes());
        dir.extend_from_slice(&[0; 12]); // extra, comment, disk, attributes
        dir.extend_from_slice(&offset.to_le_bytes());
        dir.extend_from_slice(name.as_bytes());
    }
    let at = out.len() as u32;
    out.extend_from_slice(&dir);
    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

const DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

#[test]
fn a_word_document_reads_as_its_paragraphs() {
    let xml = br#"<?xml version="1.0"?><w:document xmlns:w="w"><w:body>
        <w:p><w:r><w:t>VAT number:</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve">GB 123 4567 89</w:t></w:r></w:p>
        <w:p><w:r><w:t>Tax regime: R&amp;D relief</w:t></w:r></w:p>
        </w:body></w:document>"#;
    for deflated in [false, true] {
        let doc = zip(&[
            ("[Content_Types].xml", b"<x/>", false),
            ("word/document.xml", xml, deflated),
        ]);
        assert_eq!(
            text(&doc, DOCX).unwrap(),
            "VAT number:\tGB 123 4567 89\nTax regime: R&D relief"
        );
    }
    assert!(text(&zip(&[("other.xml", b"<x/>", false)]), DOCX).is_err());
    assert!(text(b"not a zip at all", DOCX).is_err());
}

/// A PDF of `objects`, each a dictionary and a stream deflated or not.
fn pdf_of(objects: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    for (i, (dict, data, deflate)) in objects.iter().enumerate() {
        let body = if *deflate {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        } else {
            data.to_vec()
        };
        let filter = if *deflate {
            "/Filter /FlateDecode "
        } else {
            ""
        };
        out.extend_from_slice(
            format!(
                "{} 0 obj\n<< {dict} {filter}/Length {} >>\nstream\n",
                i + 1,
                body.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&body);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    }
    out.extend_from_slice(b"%%EOF\n");
    out
}

#[test]
fn a_pdf_made_by_software_reads_as_its_text() {
    let page = b"BT /F1 12 Tf 72 712 Td (Taxpayer ID: 0123456789) Tj 0 -14 Td [(Regime) -250 (micro) (-BIC)] TJ ET";
    let pdf = pdf_of(&[("/Type /Page", b"", false), ("", page, true)]);
    assert_eq!(
        text(&pdf, "application/pdf").unwrap(),
        "Taxpayer ID: 0123456789\nRegime micro-BIC"
    );
}

/// Its fonts map two-byte codes to characters, as the tax office's do.
#[test]
fn a_pdf_with_mapped_fonts_reads_through_its_tables() {
    let cmap = b"/CIDInit /ProcSet findresource begin begincmap 1 begincodespacerange <0000> <FFFF> endcodespacerange
        2 beginbfchar <0001> <0054> <0002> <00E9> endbfchar
        1 beginbfrange <0003> <0005> <0061> endbfrange endcmap";
    let page = b"BT /F1 10 Tf <0001000200030004> Tj ET";
    let pdf = pdf_of(&[("", cmap, true), ("", page, true)]);
    assert_eq!(text(&pdf, "application/pdf").unwrap(), "Téab");
}

#[test]
fn a_scan_is_said_to_be_one() {
    let pdf = pdf_of(&[("/Subtype /Image", &[0xFF, 0xD8, 0xFF, 0xE0], false)]);
    let err = format!("{:#}", text(&pdf, "application/pdf").unwrap_err());
    assert!(err.contains("a scan"), "{err}");
}
