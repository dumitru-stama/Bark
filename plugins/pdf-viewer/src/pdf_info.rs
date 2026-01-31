//! PDF file parsing and rich ASCII rendering module

use std::collections::BTreeMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse PDF metadata (fast): info, document info, page details, structure.
pub fn parse_pdf_metadata(path: &str) -> Result<String, String> {
    let file_size = std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| e.to_string())?;

    let doc = lopdf::Document::load(path).map_err(|e| format!("Failed to parse PDF: {}", e))?;

    let mut output = String::new();
    output.push_str(&format_pdf_info(&doc, file_size));
    output.push_str(&format_document_info(&doc));
    output.push_str(&format_page_details(&doc));
    output.push_str(&format_document_structure(&doc));

    Ok(output)
}

/// Extract and format the full text content (expensive).
pub fn parse_pdf_text(path: &str) -> String {
    format_text_preview(path)
}

// ---------------------------------------------------------------------------
// Section 1: PDF INFO
// ---------------------------------------------------------------------------

fn format_pdf_info(doc: &lopdf::Document, file_size: u64) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                         PDF INFO\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // PDF version
    o.push_str(&format!("  PDF version:  {}\n", doc.version));

    // Page count
    let pages = doc.get_pages();
    o.push_str(&format!("  Pages:        {}\n", pages.len()));

    // File size
    o.push_str(&format!("  File size:    {}\n", human_size(file_size)));

    // Encrypted
    let encrypted = doc.trailer.get(b"Encrypt").is_ok();
    o.push_str(&format!(
        "  Encrypted:    {}\n",
        if encrypted { "Yes" } else { "No" }
    ));

    // Linearized — check first page object or catalog for Linearized key
    let linearized = check_linearized(doc);
    o.push_str(&format!(
        "  Linearized:   {}\n",
        if linearized {
            "Yes (fast web view)"
        } else {
            "No"
        }
    ));

    o.push('\n');
    o
}

fn check_linearized(doc: &lopdf::Document) -> bool {
    // Linearized PDFs have a Linearized key in one of the first few objects
    for (_, object) in doc.objects.iter().take(5) {
        if let lopdf::Object::Dictionary(dict) = object {
            if dict.has(b"Linearized") {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Section 2: DOCUMENT INFO
// ---------------------------------------------------------------------------

fn format_document_info(doc: &lopdf::Document) -> String {
    let mut o = String::new();

    // Get Info dictionary reference from trailer
    let info_dict = match doc.trailer.get(b"Info") {
        Ok(obj) => resolve_dict(doc, obj),
        Err(_) => None,
    };

    let info = match info_dict {
        Some(d) => d,
        None => return o,
    };

    // Extract fields
    let fields: Vec<(&str, &[u8])> = vec![
        ("Title", b"Title"),
        ("Author", b"Author"),
        ("Subject", b"Subject"),
        ("Keywords", b"Keywords"),
        ("Creator", b"Creator"),
        ("Producer", b"Producer"),
        ("Created", b"CreationDate"),
        ("Modified", b"ModDate"),
    ];

    let mut lines: Vec<(String, String)> = Vec::new();

    for (label, key) in &fields {
        if let Ok(val) = info.get(*key) {
            let text = object_to_string(doc, val);
            if !text.is_empty() {
                let display = if *label == "Created" || *label == "Modified" {
                    format_pdf_date(&text)
                } else {
                    text
                };
                lines.push((label.to_string(), display));
            }
        }
    }

    if lines.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      DOCUMENT INFO\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    for (label, value) in &lines {
        o.push_str(&format!("  {:<14}{}\n", format!("{}:", label), value));
    }

    o.push('\n');
    o
}

/// Resolve an object reference to a dictionary
fn resolve_dict<'a>(
    doc: &'a lopdf::Document,
    obj: &'a lopdf::Object,
) -> Option<&'a lopdf::Dictionary> {
    match obj {
        lopdf::Object::Dictionary(d) => Some(d),
        lopdf::Object::Reference(r) => {
            if let Ok(resolved) = doc.get_object(*r) {
                if let lopdf::Object::Dictionary(d) = resolved {
                    return Some(d);
                }
            }
            None
        }
        _ => None,
    }
}

/// Convert a PDF object to a displayable string
fn object_to_string(doc: &lopdf::Document, obj: &lopdf::Object) -> String {
    match obj {
        lopdf::Object::String(bytes, _) => {
            // Try UTF-16 BE (starts with BOM FE FF)
            if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
                let chars: Vec<u16> = bytes[2..]
                    .chunks(2)
                    .filter_map(|c| {
                        if c.len() == 2 {
                            Some(u16::from_be_bytes([c[0], c[1]]))
                        } else {
                            None
                        }
                    })
                    .collect();
                String::from_utf16_lossy(&chars)
            } else {
                // PDFDocEncoding (close enough to Latin-1 for display)
                bytes.iter().map(|&b| b as char).collect()
            }
        }
        lopdf::Object::Name(name) => String::from_utf8_lossy(name).to_string(),
        lopdf::Object::Integer(i) => i.to_string(),
        lopdf::Object::Real(f) => format!("{}", f),
        lopdf::Object::Boolean(b) => b.to_string(),
        lopdf::Object::Reference(r) => {
            if let Ok(resolved) = doc.get_object(*r) {
                object_to_string(doc, resolved)
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// Format a PDF date string (D:YYYYMMDDHHmmSSOHH'mm') into a readable form
fn format_pdf_date(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("D:").unwrap_or(s);

    if s.len() < 8 {
        return s.to_string();
    }

    let year = &s[0..4];
    let month = s.get(4..6).unwrap_or("01");
    let day = s.get(6..8).unwrap_or("01");
    let hour = s.get(8..10).unwrap_or("00");
    let minute = s.get(10..12).unwrap_or("00");
    let second = s.get(12..14).unwrap_or("00");

    format!(
        "{}-{}-{} {}:{}:{}",
        year, month, day, hour, minute, second
    )
}

// ---------------------------------------------------------------------------
// Section 3: PAGE DETAILS
// ---------------------------------------------------------------------------

fn format_page_details(doc: &lopdf::Document) -> String {
    let mut o = String::new();

    let pages = doc.get_pages();
    if pages.is_empty() {
        return o;
    }

    // Collect page sizes (MediaBox)
    let mut sizes: BTreeMap<String, Vec<u32>> = BTreeMap::new();

    for (&page_num, &page_id) in &pages {
        let media_box = get_page_media_box(doc, page_id);
        if let Some((w, h)) = media_box {
            let key = format!("{:.0}x{:.0}", w, h);
            sizes.entry(key).or_default().push(page_num);
        }
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      PAGE DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    if sizes.is_empty() {
        o.push_str("  Page size:    Unknown\n");
    } else if sizes.len() == 1 {
        // All pages same size
        let (key, _page_nums) = sizes.iter().next().unwrap();
        let (w, h) = parse_size_key(key);
        let name = paper_size_name(w, h);
        let orientation = if w > h { "Landscape" } else { "Portrait" };
        let w_in = w / 72.0;
        let h_in = h / 72.0;
        o.push_str(&format!(
            "  Page size:    {:.0} x {:.0} pts ({:.1} x {:.1} in{})\n",
            w,
            h,
            w_in,
            h_in,
            if name.is_empty() {
                String::new()
            } else {
                format!(", {}", name)
            }
        ));
        o.push_str(&format!("  Orientation:  {}\n", orientation));
    } else {
        // Multiple sizes
        for (key, page_nums) in &sizes {
            let (w, h) = parse_size_key(key);
            let name = paper_size_name(w, h);
            let w_in = w / 72.0;
            let h_in = h / 72.0;
            let range = format_page_ranges(page_nums);
            o.push_str(&format!(
                "  {:.0} x {:.0} pts ({:.1} x {:.1} in{}): {}\n",
                w,
                h,
                w_in,
                h_in,
                if name.is_empty() {
                    String::new()
                } else {
                    format!(", {}", name)
                },
                range
            ));
        }
    }

    o.push('\n');
    o
}

fn get_page_media_box(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> Option<(f64, f64)> {
    // Try to get MediaBox from this page, then walk up to parent
    if let Ok(page_obj) = doc.get_object(page_id) {
        if let lopdf::Object::Dictionary(dict) = page_obj {
            if let Some(mb) = extract_media_box(doc, dict) {
                return Some(mb);
            }
            // Try parent
            if let Ok(parent_ref) = dict.get(b"Parent") {
                if let lopdf::Object::Reference(r) = parent_ref {
                    if let Ok(lopdf::Object::Dictionary(parent_dict)) = doc.get_object(*r) {
                        if let Some(mb) = extract_media_box(doc, parent_dict) {
                            return Some(mb);
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_media_box(doc: &lopdf::Document, dict: &lopdf::Dictionary) -> Option<(f64, f64)> {
    let mb = dict.get(b"MediaBox").ok()?;
    let arr = resolve_array(doc, mb)?;
    if arr.len() >= 4 {
        let x0 = obj_to_f64(doc, &arr[0]).unwrap_or(0.0);
        let y0 = obj_to_f64(doc, &arr[1]).unwrap_or(0.0);
        let x1 = obj_to_f64(doc, &arr[2]).unwrap_or(0.0);
        let y1 = obj_to_f64(doc, &arr[3]).unwrap_or(0.0);
        let w = (x1 - x0).abs();
        let h = (y1 - y0).abs();
        if w > 0.0 && h > 0.0 {
            return Some((w, h));
        }
    }
    None
}

fn resolve_array<'a>(
    doc: &'a lopdf::Document,
    obj: &'a lopdf::Object,
) -> Option<&'a Vec<lopdf::Object>> {
    match obj {
        lopdf::Object::Array(a) => Some(a),
        lopdf::Object::Reference(r) => {
            if let Ok(resolved) = doc.get_object(*r) {
                if let lopdf::Object::Array(a) = resolved {
                    return Some(a);
                }
            }
            None
        }
        _ => None,
    }
}

fn obj_to_f64(doc: &lopdf::Document, obj: &lopdf::Object) -> Option<f64> {
    match obj {
        lopdf::Object::Integer(i) => Some(*i as f64),
        lopdf::Object::Real(f) => Some(*f as f64),
        lopdf::Object::Reference(r) => {
            if let Ok(resolved) = doc.get_object(*r) {
                obj_to_f64(doc, resolved)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_size_key(key: &str) -> (f64, f64) {
    let parts: Vec<&str> = key.split('x').collect();
    if parts.len() == 2 {
        let w: f64 = parts[0].parse().unwrap_or(0.0);
        let h: f64 = parts[1].parse().unwrap_or(0.0);
        (w, h)
    } else {
        (0.0, 0.0)
    }
}

fn paper_size_name(w: f64, h: f64) -> &'static str {
    // Normalize to portrait (smaller dimension first)
    let (small, large) = if w < h { (w, h) } else { (h, w) };

    // Tolerance of ~2 pts for rounding
    let matches = |a: f64, b: f64, target_a: f64, target_b: f64| -> bool {
        (a - target_a).abs() < 2.0 && (b - target_b).abs() < 2.0
    };

    if matches(small, large, 612.0, 792.0) {
        "Letter"
    } else if matches(small, large, 595.0, 842.0) {
        "A4"
    } else if matches(small, large, 612.0, 1008.0) {
        "Legal"
    } else if matches(small, large, 841.0, 1190.0) {
        "A3"
    } else if matches(small, large, 420.0, 595.0) {
        "A5"
    } else if matches(small, large, 297.0, 420.0) {
        "A6"
    } else if matches(small, large, 504.0, 720.0) {
        "7x10"
    } else if matches(small, large, 432.0, 648.0) {
        "6x9"
    } else if matches(small, large, 540.0, 666.0) {
        "7.5x9.25"
    } else {
        ""
    }
}

fn format_page_ranges(pages: &[u32]) -> String {
    if pages.is_empty() {
        return String::new();
    }
    let mut sorted = pages.to_vec();
    sorted.sort();

    let mut ranges: Vec<String> = Vec::new();
    let mut start = sorted[0];
    let mut end = sorted[0];

    for &p in &sorted[1..] {
        if p == end + 1 {
            end = p;
        } else {
            if start == end {
                ranges.push(format!("p{}", start));
            } else {
                ranges.push(format!("p{}-{}", start, end));
            }
            start = p;
            end = p;
        }
    }
    if start == end {
        ranges.push(format!("p{}", start));
    } else {
        ranges.push(format!("p{}-{}", start, end));
    }

    format!("{} ({})", ranges.join(", "), pages.len())
}

// ---------------------------------------------------------------------------
// Section 4: DOCUMENT STRUCTURE
// ---------------------------------------------------------------------------

fn format_document_structure(doc: &lopdf::Document) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                   DOCUMENT STRUCTURE\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // Object count
    let obj_count = doc.objects.len();
    o.push_str(&format!("  Objects:      {}\n", format_number(obj_count)));

    // Collect fonts across all pages
    let mut fonts: BTreeMap<String, String> = BTreeMap::new();
    let pages = doc.get_pages();

    for &page_id in pages.values() {
        collect_page_fonts(doc, page_id, &mut fonts);
    }

    if !fonts.is_empty() {
        o.push_str(&format!("  Fonts:        {}\n", fonts.len()));
        for (name, subtype) in &fonts {
            let display_name = name
                .strip_prefix('/')
                .unwrap_or(name)
                .replace('#', "")
                .replace('+', " ");
            // Trim subset prefix (e.g., "ABCDEF+FontName" -> "FontName")
            let display_name = if display_name.len() > 7
                && display_name.as_bytes()[6] == b' '
                && display_name[..6].chars().all(|c| c.is_ascii_uppercase())
            {
                &display_name[7..]
            } else {
                &display_name
            };
            o.push_str(&format!(
                "    {:<40} {}\n",
                truncate(display_name, 40),
                subtype
            ));
        }
    }

    // Check for forms (AcroForm)
    let has_forms = doc
        .catalog()
        .ok()
        .and_then(|cat| cat.get(b"AcroForm").ok())
        .is_some();
    if has_forms {
        o.push_str("  Forms:        Yes (AcroForm)\n");
    }

    // Check for outlines (bookmarks)
    let has_outlines = doc
        .catalog()
        .ok()
        .and_then(|cat| cat.get(b"Outlines").ok())
        .is_some();
    if has_outlines {
        o.push_str("  Bookmarks:    Yes\n");
    }

    o.push('\n');
    o
}

fn collect_page_fonts(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    fonts: &mut BTreeMap<String, String>,
) {
    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return,
    };

    let page_dict = match page_obj {
        lopdf::Object::Dictionary(d) => d,
        _ => return,
    };

    // Get Resources -> Font dictionary
    let resources = match page_dict.get(b"Resources") {
        Ok(r) => r,
        Err(_) => {
            // Try parent for inherited resources
            if let Ok(parent_ref) = page_dict.get(b"Parent") {
                if let lopdf::Object::Reference(r) = parent_ref {
                    if let Ok(lopdf::Object::Dictionary(parent)) = doc.get_object(*r) {
                        match parent.get(b"Resources") {
                            Ok(r) => r,
                            Err(_) => return,
                        }
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            } else {
                return;
            }
        }
    };

    let res_dict = match resources {
        lopdf::Object::Dictionary(d) => d,
        lopdf::Object::Reference(r) => {
            match doc.get_object(*r) {
                Ok(lopdf::Object::Dictionary(d)) => d,
                _ => return,
            }
        }
        _ => return,
    };

    let font_obj = match res_dict.get(b"Font") {
        Ok(f) => f,
        Err(_) => return,
    };

    let font_dict = match font_obj {
        lopdf::Object::Dictionary(d) => d,
        lopdf::Object::Reference(r) => {
            match doc.get_object(*r) {
                Ok(lopdf::Object::Dictionary(d)) => d,
                _ => return,
            }
        }
        _ => return,
    };

    for (name, val) in font_dict.iter() {
        let font_name = String::from_utf8_lossy(name).to_string();

        // Resolve the font descriptor to get BaseFont and Subtype
        let (base_font, subtype) = match val {
            lopdf::Object::Reference(r) => {
                match doc.get_object(*r) {
                    Ok(lopdf::Object::Dictionary(fd)) => {
                        let bf = fd
                            .get(b"BaseFont")
                            .ok()
                            .map(|o| object_to_string(doc, o))
                            .unwrap_or_default();
                        let st = fd
                            .get(b"Subtype")
                            .ok()
                            .map(|o| object_to_string(doc, o))
                            .unwrap_or_default();
                        (bf, st)
                    }
                    _ => (String::new(), String::new()),
                }
            }
            lopdf::Object::Dictionary(fd) => {
                let bf = fd
                    .get(b"BaseFont")
                    .ok()
                    .map(|o| object_to_string(doc, o))
                    .unwrap_or_default();
                let st = fd
                    .get(b"Subtype")
                    .ok()
                    .map(|o| object_to_string(doc, o))
                    .unwrap_or_default();
                (bf, st)
            }
            _ => (String::new(), String::new()),
        };

        let display_name = if !base_font.is_empty() {
            base_font
        } else {
            font_name
        };

        if !display_name.is_empty() {
            fonts.insert(display_name, subtype);
        }
    }
}

// ---------------------------------------------------------------------------
// Section 5: TEXT PREVIEW
// ---------------------------------------------------------------------------

fn format_text_preview(path: &str) -> String {
    let mut o = String::new();

    let text = match pdf_extract::extract_text(Path::new(path)) {
        Ok(t) => t,
        Err(_) => return o,
    };

    let text = text.trim();
    if text.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      TEXT CONTENT\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // Indent each line with 2 spaces, collapse runs of blank lines.
    // Strip control characters that might appear in extracted PDF text.
    let mut prev_blank = false;
    for line in text.lines() {
        let trimmed: String = line
            .trim_end()
            .chars()
            .filter(|c| !c.is_control() || *c == '\t')
            .collect();
        if trimmed.is_empty() {
            if !prev_blank {
                o.push('\n');
                prev_blank = true;
            }
        } else {
            o.push_str(&format!("  {}\n", trimmed));
            prev_blank = false;
        }
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
