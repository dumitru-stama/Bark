//! Image file parsing and rich ASCII rendering module

use std::fs::File;
use std::io::Read;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Check if this file is a supported image format
pub fn can_handle(path: &str) -> bool {
    // Try to detect format via the image crate
    match image::ImageReader::open(path) {
        Ok(reader) => match reader.with_guessed_format() {
            Ok(reader) => reader.format().is_some(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// Parse an image file and return formatted output
pub fn parse_image(path: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|e| e.to_string())?;

    let file_size = data.len() as u64;

    // Detect format via the image crate
    let reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;

    let format = reader.format();
    let format_name = format
        .map(|f| format_name(f))
        .unwrap_or_else(|| "Unknown".to_string());

    // Try to read dimensions and color type without decoding pixels
    let (dimensions, color_type) = match reader.into_dimensions() {
        Ok((w, h)) => {
            // Re-open to get color type (into_dimensions consumes the reader)
            let color = image::ImageReader::open(path)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.decode().ok())
                .map(|img| format!("{:?}", img.color()));
            (Some((w, h)), color)
        }
        Err(_) => (None, None),
    };

    let mut output = String::new();

    // 1. IMAGE INFO (always)
    output.push_str(&format_image_info(
        &format_name,
        dimensions,
        color_type.as_deref(),
        file_size,
    ));

    // 2. Format-specific details (from raw bytes)
    output.push_str(&format_specific_details(&data, format));

    // 3. EXIF data
    if let Some(exif_output) = format_exif_data(path) {
        output.push_str(&exif_output);
    }

    Ok(output)
}

// ---------------------------------------------------------------------------
// Section 1: IMAGE INFO
// ---------------------------------------------------------------------------

fn format_image_info(
    format_name: &str,
    dimensions: Option<(u32, u32)>,
    color_type: Option<&str>,
    file_size: u64,
) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                        IMAGE INFO\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str(&format!("  Format:       {}\n", format_name));

    if let Some((w, h)) = dimensions {
        let megapixels = (w as f64 * h as f64) / 1_000_000.0;
        if megapixels >= 1.0 {
            o.push_str(&format!(
                "  Dimensions:   {} x {} ({:.1} megapixels)\n",
                w, h, megapixels
            ));
        } else {
            o.push_str(&format!("  Dimensions:   {} x {}\n", w, h));
        }

        // Aspect ratio
        let gcd = gcd(w, h);
        if gcd > 0 {
            let ar_w = w / gcd;
            let ar_h = h / gcd;
            // Only show if it simplifies to something reasonable
            if ar_w <= 100 && ar_h <= 100 {
                o.push_str(&format!("  Aspect ratio: {}:{}\n", ar_w, ar_h));
            }
        }
    }

    if let Some(ct) = color_type {
        o.push_str(&format!("  Color type:   {}\n", ct));
    }

    o.push_str(&format!("  File size:    {}\n", human_size(file_size)));

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Section 2: Format-specific details
// ---------------------------------------------------------------------------

fn format_specific_details(data: &[u8], format: Option<image::ImageFormat>) -> String {
    match format {
        Some(image::ImageFormat::Jpeg) => format_jpeg_details(data),
        Some(image::ImageFormat::Png) => format_png_details(data),
        Some(image::ImageFormat::Gif) => format_gif_details(data),
        Some(image::ImageFormat::Bmp) => format_bmp_details(data),
        Some(image::ImageFormat::WebP) => format_webp_details(data),
        Some(image::ImageFormat::Tiff) => format_tiff_details(data),
        Some(image::ImageFormat::Ico) => format_ico_details(data),
        Some(image::ImageFormat::Tga) => format_tga_details(data),
        Some(image::ImageFormat::Dds) => format_dds_details(data),
        Some(image::ImageFormat::Hdr) => format_hdr_details(data),
        Some(image::ImageFormat::OpenExr) => format_exr_details(data),
        Some(image::ImageFormat::Pnm) => format_pnm_details(data),
        Some(image::ImageFormat::Qoi) => format_qoi_details(data),
        Some(image::ImageFormat::Farbfeld) => format_farbfeld_details(data),
        Some(image::ImageFormat::Avif) => format_avif_details(data),
        _ => String::new(),
    }
}

// --- JPEG ---

fn format_jpeg_details(data: &[u8]) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      JPEG DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // Scan for SOF markers to determine baseline vs progressive
    let mut encoding = "Unknown";
    let mut components = 0u8;
    let mut precision = 0u8;
    let mut has_jfif = false;
    let mut has_exif = false;
    let mut has_icc = false;
    let mut has_adobe = false;
    let mut has_thumbnail = false;
    let mut restart_interval = 0u16;
    let mut num_scans = 0u32;

    let mut i = 0;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }

        let marker = data[i + 1];
        match marker {
            // SOF0 = baseline DCT
            0xC0 => {
                encoding = "Baseline DCT";
                if i + 9 < data.len() {
                    precision = data[i + 4];
                    components = data[i + 9];
                }
            }
            // SOF1 = extended sequential DCT
            0xC1 => {
                encoding = "Extended Sequential DCT";
                if i + 9 < data.len() {
                    precision = data[i + 4];
                    components = data[i + 9];
                }
            }
            // SOF2 = progressive DCT
            0xC2 => {
                encoding = "Progressive DCT";
                if i + 9 < data.len() {
                    precision = data[i + 4];
                    components = data[i + 9];
                }
            }
            // SOF3 = lossless
            0xC3 => {
                encoding = "Lossless (Huffman)";
                if i + 9 < data.len() {
                    precision = data[i + 4];
                    components = data[i + 9];
                }
            }
            // SOF9 = arithmetic coding
            0xC9 => {
                encoding = "Extended Sequential (Arithmetic)";
            }
            // SOF10 = progressive arithmetic
            0xCA => {
                encoding = "Progressive (Arithmetic)";
            }
            // SOF11 = lossless arithmetic
            0xCB => {
                encoding = "Lossless (Arithmetic)";
            }
            // SOS = start of scan
            0xDA => {
                num_scans += 1;
            }
            // DRI = define restart interval
            0xDD => {
                if i + 5 < data.len() {
                    restart_interval =
                        u16::from_be_bytes([data[i + 4], data[i + 5]]);
                }
            }
            // APP0 = JFIF
            0xE0 => {
                if i + 6 < data.len() && &data[i + 4..i + 6] == b"JF" {
                    has_jfif = true;
                    // Check for thumbnail
                    if i + 18 < data.len() {
                        let tw = data[i + 16];
                        let th = data[i + 17];
                        if tw > 0 && th > 0 {
                            has_thumbnail = true;
                        }
                    }
                }
            }
            // APP1 = EXIF
            0xE1 => {
                if i + 10 < data.len() && &data[i + 4..i + 8] == b"Exif" {
                    has_exif = true;
                }
            }
            // APP2 = ICC profile
            0xE2 => {
                if i + 18 < data.len() && &data[i + 4..i + 15] == b"ICC_PROFILE" {
                    has_icc = true;
                }
            }
            // APP14 = Adobe
            0xEE => {
                if i + 9 < data.len() && &data[i + 4..i + 9] == b"Adobe" {
                    has_adobe = true;
                }
            }
            _ => {}
        }

        // Skip past marker segment (markers with length field)
        if marker >= 0xC0 && marker != 0xD8 && marker != 0xD9 && !(0xD0..=0xD7).contains(&marker) {
            if i + 3 < data.len() {
                let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                i += 2 + seg_len;
            } else {
                i += 2;
            }
        } else {
            i += 2;
        }
    }

    o.push_str(&format!("  Encoding:     {}\n", encoding));
    if precision > 0 {
        o.push_str(&format!("  Precision:    {}-bit\n", precision));
    }
    if components > 0 {
        let color_space = match components {
            1 => "Grayscale",
            3 => "YCbCr (likely sRGB)",
            4 => "CMYK",
            _ => "Unknown",
        };
        o.push_str(&format!(
            "  Components:   {} ({})\n",
            components, color_space
        ));
    }
    if num_scans > 0 {
        o.push_str(&format!("  Scans:        {}\n", num_scans));
    }
    if restart_interval > 0 {
        o.push_str(&format!("  Restart int:  {} MCUs\n", restart_interval));
    }

    // Markers summary
    let mut markers = Vec::new();
    if has_jfif {
        markers.push("JFIF");
    }
    if has_exif {
        markers.push("EXIF");
    }
    if has_icc {
        markers.push("ICC Profile");
    }
    if has_adobe {
        markers.push("Adobe");
    }
    if has_thumbnail {
        markers.push("Thumbnail");
    }
    if !markers.is_empty() {
        o.push_str(&format!("  Markers:      {}\n", markers.join(", ")));
    }

    o.push('\n');
    o
}

// --- PNG ---

fn format_png_details(data: &[u8]) -> String {
    let mut o = String::new();

    // PNG signature: 137 80 78 71 13 10 26 10
    if data.len() < 33 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       PNG DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // IHDR chunk at offset 8: length(4) + "IHDR"(4) + data(13)
    let ihdr_start = 16; // skip length(4) + "IHDR"(4)
    if data.len() < ihdr_start + 13 {
        return o;
    }

    let bit_depth = data[ihdr_start + 8];
    let color_type_byte = data[ihdr_start + 9];
    let compression = data[ihdr_start + 10];
    let filter = data[ihdr_start + 11];
    let interlace = data[ihdr_start + 12];

    let color_type_str = match color_type_byte {
        0 => "Grayscale",
        2 => "RGB (Truecolor)",
        3 => "Indexed (Palette)",
        4 => "Grayscale + Alpha",
        6 => "RGBA (Truecolor + Alpha)",
        _ => "Unknown",
    };

    o.push_str(&format!("  Bit depth:    {}\n", bit_depth));
    o.push_str(&format!(
        "  Color type:   {} (type {})\n",
        color_type_str, color_type_byte
    ));
    o.push_str(&format!(
        "  Compression:  {}\n",
        if compression == 0 {
            "Deflate"
        } else {
            "Unknown"
        }
    ));
    o.push_str(&format!(
        "  Filter:       {}\n",
        if filter == 0 {
            "Adaptive"
        } else {
            "Unknown"
        }
    ));
    o.push_str(&format!(
        "  Interlace:    {}\n",
        match interlace {
            0 => "None",
            1 => "Adam7",
            _ => "Unknown",
        }
    ));

    // Scan chunks for additional info
    let mut has_srgb = false;
    let mut has_iccp = false;
    let mut has_gamma = false;
    let mut has_chrm = false;
    let mut has_text = false;
    let mut has_itxt = false;
    let mut has_actl = false; // APNG animation control
    let mut has_trns = false;
    let mut has_phys = false;
    let mut has_time = false;
    let mut palette_entries = 0u32;
    let mut num_frames = 0u32;
    let mut text_entries: Vec<(String, String)> = Vec::new();
    let mut phys_x = 0u32;
    let mut phys_y = 0u32;
    let mut phys_unit = 0u8;
    let mut gamma_value = 0u32;

    let mut pos = 8;
    while pos + 12 <= data.len() {
        let chunk_len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let chunk_type = &data[pos + 4..pos + 8];

        match chunk_type {
            b"PLTE" => {
                palette_entries = (chunk_len / 3) as u32;
            }
            b"tRNS" => has_trns = true,
            b"sRGB" => has_srgb = true,
            b"iCCP" => has_iccp = true,
            b"gAMA" => {
                has_gamma = true;
                if chunk_len >= 4 && pos + 12 <= data.len() {
                    gamma_value = u32::from_be_bytes([
                        data[pos + 8],
                        data[pos + 9],
                        data[pos + 10],
                        data[pos + 11],
                    ]);
                }
            }
            b"cHRM" => has_chrm = true,
            b"pHYs" => {
                has_phys = true;
                if chunk_len >= 9 && pos + 17 <= data.len() {
                    phys_x = u32::from_be_bytes([
                        data[pos + 8],
                        data[pos + 9],
                        data[pos + 10],
                        data[pos + 11],
                    ]);
                    phys_y = u32::from_be_bytes([
                        data[pos + 12],
                        data[pos + 13],
                        data[pos + 14],
                        data[pos + 15],
                    ]);
                    phys_unit = data[pos + 16];
                }
            }
            b"tIME" => has_time = true,
            b"tEXt" => {
                has_text = true;
                if chunk_len > 0 && pos + 8 + chunk_len <= data.len() {
                    let text_data = &data[pos + 8..pos + 8 + chunk_len];
                    if let Some(null_pos) = text_data.iter().position(|&b| b == 0) {
                        let key = String::from_utf8_lossy(&text_data[..null_pos]).to_string();
                        let val =
                            String::from_utf8_lossy(&text_data[null_pos + 1..]).to_string();
                        if text_entries.len() < 20 {
                            text_entries.push((key, val));
                        }
                    }
                }
            }
            b"iTXt" => {
                has_itxt = true;
            }
            b"acTL" => {
                has_actl = true;
                if chunk_len >= 8 && pos + 16 <= data.len() {
                    num_frames = u32::from_be_bytes([
                        data[pos + 8],
                        data[pos + 9],
                        data[pos + 10],
                        data[pos + 11],
                    ]);
                }
            }
            _ => {}
        }

        pos += 12 + chunk_len; // length(4) + type(4) + data + crc(4)
    }

    if palette_entries > 0 {
        o.push_str(&format!("  Palette:      {} entries\n", palette_entries));
    }
    if has_trns {
        o.push_str("  Transparency: Yes (tRNS chunk)\n");
    }
    if has_actl {
        o.push_str(&format!(
            "  Animation:    APNG ({} frames)\n",
            num_frames
        ));
    }
    if has_gamma {
        o.push_str(&format!(
            "  Gamma:        {:.5}\n",
            gamma_value as f64 / 100_000.0
        ));
    }
    if has_phys {
        if phys_unit == 1 {
            // Pixels per meter
            let dpi_x = (phys_x as f64 * 0.0254).round() as u32;
            let dpi_y = (phys_y as f64 * 0.0254).round() as u32;
            o.push_str(&format!(
                "  Resolution:   {} x {} DPI\n",
                dpi_x, dpi_y
            ));
        } else {
            o.push_str(&format!(
                "  Pixel ratio:  {} : {}\n",
                phys_x, phys_y
            ));
        }
    }

    // Color management
    let mut color_mgmt = Vec::new();
    if has_srgb {
        color_mgmt.push("sRGB");
    }
    if has_iccp {
        color_mgmt.push("ICC Profile");
    }
    if has_chrm {
        color_mgmt.push("Chromaticities");
    }
    if !color_mgmt.is_empty() {
        o.push_str(&format!("  Color mgmt:   {}\n", color_mgmt.join(", ")));
    }

    // Metadata chunks
    let mut meta = Vec::new();
    if has_text {
        meta.push("tEXt");
    }
    if has_itxt {
        meta.push("iTXt");
    }
    if has_time {
        meta.push("tIME");
    }
    if !meta.is_empty() {
        o.push_str(&format!("  Metadata:     {}\n", meta.join(", ")));
    }

    o.push('\n');

    // Text entries
    if !text_entries.is_empty() {
        o.push_str("═══════════════════════════════════════════════════════════════\n");
        o.push_str("                      PNG TEXT DATA\n");
        o.push_str("═══════════════════════════════════════════════════════════════\n\n");

        for (key, val) in &text_entries {
            let display_val = if val.len() > 200 {
                format!("{}...", &val[..200])
            } else {
                val.clone()
            };
            o.push_str(&format!("  {:<16}{}\n", format!("{}:", key), display_val));
        }
        o.push('\n');
    }

    o
}

// --- GIF ---

fn format_gif_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 13 {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       GIF DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // Version: GIF87a or GIF89a
    let version = String::from_utf8_lossy(&data[0..6]);
    o.push_str(&format!("  Version:      {}\n", version));

    // Logical screen descriptor
    let _width = u16::from_le_bytes([data[6], data[7]]);
    let _height = u16::from_le_bytes([data[8], data[9]]);
    let packed = data[10];
    let has_gct = packed & 0x80 != 0;
    let color_resolution = ((packed >> 4) & 0x07) + 1;
    let gct_sorted = packed & 0x08 != 0;
    let gct_size = if has_gct {
        1 << ((packed & 0x07) + 1)
    } else {
        0
    };
    let bg_color_index = data[11];
    let pixel_aspect = data[12];

    o.push_str(&format!("  Color depth:  {} bits\n", color_resolution));
    if has_gct {
        o.push_str(&format!(
            "  Global table: {} colors{}\n",
            gct_size,
            if gct_sorted { " (sorted)" } else { "" }
        ));
    }
    o.push_str(&format!("  Background:   index {}\n", bg_color_index));
    if pixel_aspect != 0 {
        let ratio = (pixel_aspect as f64 + 15.0) / 64.0;
        o.push_str(&format!("  Pixel aspect: {:.3}\n", ratio));
    }

    // Count frames and check for features
    let mut frame_count = 0u32;
    let mut has_transparency = false;
    let mut has_comment = false;
    let mut has_plain_text = false;
    let mut has_app_ext = false;
    let mut has_netscape_loop = false;
    let mut loop_count = 0u16;
    let mut total_delay = 0u32;

    let gct_bytes = if has_gct { gct_size * 3 } else { 0 };
    let mut pos = 13 + gct_bytes as usize;

    while pos < data.len() {
        match data[pos] {
            // Image descriptor
            0x2C => {
                frame_count += 1;
                if pos + 10 < data.len() {
                    let local_packed = data[pos + 9];
                    let has_lct = local_packed & 0x80 != 0;
                    if has_lct {
                        let lct_size = 1usize << ((local_packed & 0x07) + 1);
                        pos += 10 + lct_size * 3;
                    } else {
                        pos += 10;
                    }
                    // Skip LZW minimum code size + sub-blocks
                    if pos < data.len() {
                        pos += 1; // LZW min code size
                        // Skip sub-blocks
                        while pos < data.len() {
                            let block_size = data[pos] as usize;
                            pos += 1;
                            if block_size == 0 {
                                break;
                            }
                            pos += block_size;
                        }
                    }
                } else {
                    pos += 1;
                }
            }
            // Extension
            0x21 => {
                if pos + 1 >= data.len() {
                    break;
                }
                let ext_label = data[pos + 1];
                match ext_label {
                    // Graphic control extension
                    0xF9 => {
                        if pos + 6 < data.len() {
                            let gce_packed = data[pos + 3];
                            if gce_packed & 0x01 != 0 {
                                has_transparency = true;
                            }
                            let delay =
                                u16::from_le_bytes([data[pos + 4], data[pos + 5]]) as u32;
                            total_delay += delay;
                        }
                        pos += 2;
                        // Skip sub-blocks
                        while pos < data.len() {
                            let block_size = data[pos] as usize;
                            pos += 1;
                            if block_size == 0 {
                                break;
                            }
                            pos += block_size;
                        }
                    }
                    // Comment extension
                    0xFE => {
                        has_comment = true;
                        pos += 2;
                        while pos < data.len() {
                            let block_size = data[pos] as usize;
                            pos += 1;
                            if block_size == 0 {
                                break;
                            }
                            pos += block_size;
                        }
                    }
                    // Plain text extension
                    0x01 => {
                        has_plain_text = true;
                        pos += 2;
                        while pos < data.len() {
                            let block_size = data[pos] as usize;
                            pos += 1;
                            if block_size == 0 {
                                break;
                            }
                            pos += block_size;
                        }
                    }
                    // Application extension
                    0xFF => {
                        has_app_ext = true;
                        if pos + 14 < data.len() && data[pos + 2] == 11 {
                            let app_id = &data[pos + 3..pos + 14];
                            if app_id == b"NETSCAPE2.0" || app_id == b"ANIMEXTS1.0" {
                                has_netscape_loop = true;
                                if pos + 18 < data.len()
                                    && data[pos + 14] == 3
                                    && data[pos + 15] == 1
                                {
                                    loop_count = u16::from_le_bytes([
                                        data[pos + 16],
                                        data[pos + 17],
                                    ]);
                                }
                            }
                        }
                        pos += 2;
                        while pos < data.len() {
                            let block_size = data[pos] as usize;
                            pos += 1;
                            if block_size == 0 {
                                break;
                            }
                            pos += block_size;
                        }
                    }
                    _ => {
                        pos += 2;
                        while pos < data.len() {
                            let block_size = data[pos] as usize;
                            pos += 1;
                            if block_size == 0 {
                                break;
                            }
                            pos += block_size;
                        }
                    }
                }
            }
            // Trailer
            0x3B => break,
            _ => {
                pos += 1;
            }
        }
    }

    o.push_str(&format!("  Frames:       {}\n", frame_count));
    if frame_count > 1 {
        let duration_secs = total_delay as f64 / 100.0;
        o.push_str(&format!("  Duration:     {:.2}s\n", duration_secs));
        if duration_secs > 0.0 {
            let fps = frame_count as f64 / duration_secs;
            o.push_str(&format!("  Frame rate:   {:.1} fps\n", fps));
        }
        if has_netscape_loop {
            if loop_count == 0 {
                o.push_str("  Looping:      Infinite\n");
            } else {
                o.push_str(&format!("  Looping:      {} times\n", loop_count));
            }
        }
    }
    if has_transparency {
        o.push_str("  Transparency: Yes\n");
    }

    let mut features = Vec::new();
    if has_comment {
        features.push("Comments");
    }
    if has_plain_text {
        features.push("Plain text");
    }
    if has_app_ext && !has_netscape_loop {
        features.push("App extensions");
    }
    if !features.is_empty() {
        o.push_str(&format!("  Features:     {}\n", features.join(", ")));
    }

    o.push('\n');
    o
}

// --- BMP ---

fn format_bmp_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 18 || &data[0..2] != b"BM" {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       BMP DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let data_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);
    let dib_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);

    let dib_version = match dib_size {
        12 => "BITMAPCOREHEADER (OS/2 v1)",
        40 => "BITMAPINFOHEADER (Windows v3)",
        52 => "BITMAPV2INFOHEADER",
        56 => "BITMAPV3INFOHEADER",
        64 => "BITMAPCOREHEADER2 (OS/2 v2)",
        108 => "BITMAPV4HEADER",
        124 => "BITMAPV5HEADER",
        _ => "Unknown",
    };

    o.push_str(&format!("  DIB header:   {} ({} bytes)\n", dib_version, dib_size));
    o.push_str(&format!("  Data offset:  {} bytes\n", data_offset));

    if dib_size >= 40 && data.len() >= 54 {
        let bit_count = u16::from_le_bytes([data[28], data[29]]);
        let compression = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);
        let image_size = u32::from_le_bytes([data[34], data[35], data[36], data[37]]);
        let x_ppm = u32::from_le_bytes([data[38], data[39], data[40], data[41]]);
        let y_ppm = u32::from_le_bytes([data[42], data[43], data[44], data[45]]);
        let colors_used = u32::from_le_bytes([data[46], data[47], data[48], data[49]]);
        let colors_important = u32::from_le_bytes([data[50], data[51], data[52], data[53]]);

        o.push_str(&format!("  Bits/pixel:   {}\n", bit_count));

        let compression_str = match compression {
            0 => "BI_RGB (uncompressed)",
            1 => "BI_RLE8",
            2 => "BI_RLE4",
            3 => "BI_BITFIELDS",
            4 => "BI_JPEG",
            5 => "BI_PNG",
            6 => "BI_ALPHABITFIELDS",
            _ => "Unknown",
        };
        o.push_str(&format!("  Compression:  {}\n", compression_str));

        if image_size > 0 {
            o.push_str(&format!("  Image data:   {}\n", human_size(image_size as u64)));
        }

        if x_ppm > 0 && y_ppm > 0 {
            let dpi_x = (x_ppm as f64 * 0.0254).round() as u32;
            let dpi_y = (y_ppm as f64 * 0.0254).round() as u32;
            o.push_str(&format!("  Resolution:   {} x {} DPI\n", dpi_x, dpi_y));
        }

        if colors_used > 0 {
            o.push_str(&format!("  Colors used:  {}\n", colors_used));
        }
        if colors_important > 0 && colors_important != colors_used {
            o.push_str(&format!("  Important:    {}\n", colors_important));
        }

        // V4/V5 color space info
        if dib_size >= 108 && data.len() >= 14 + 108 {
            let cs_type = u32::from_le_bytes([data[70], data[71], data[72], data[73]]);
            let cs_str = match cs_type {
                0x00000000 => Some("LCS_CALIBRATED_RGB"),
                0x73524742 => Some("sRGB"),
                0x57696E20 => Some("WINDOWS_COLOR_SPACE"),
                0x4C494E4B => Some("PROFILE_LINKED"),
                0x4D424544 => Some("PROFILE_EMBEDDED"),
                _ => None,
            };
            if let Some(cs) = cs_str {
                o.push_str(&format!("  Color space:  {}\n", cs));
            }
        }
    } else if dib_size == 12 && data.len() >= 26 {
        // OS/2 core header
        let bit_count = u16::from_le_bytes([data[24], data[25]]);
        o.push_str(&format!("  Bits/pixel:   {}\n", bit_count));
    }

    // Check for top-down vs bottom-up
    if dib_size >= 40 && data.len() >= 26 {
        let height_raw = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
        if height_raw < 0 {
            o.push_str("  Orientation:  Top-down\n");
        } else {
            o.push_str("  Orientation:  Bottom-up (standard)\n");
        }
    }

    o.push('\n');
    o
}

// --- WebP ---

fn format_webp_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 20 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      WEBP DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let file_size_riff = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    o.push_str(&format!("  RIFF size:    {}\n", human_size(file_size_riff as u64 + 8)));

    // Check chunk at offset 12
    if data.len() < 16 {
        o.push('\n');
        return o;
    }

    let chunk_type = &data[12..16];

    match chunk_type {
        b"VP8 " => {
            o.push_str("  Encoding:     Lossy (VP8)\n");
            // VP8 bitstream header
            if data.len() >= 30 {
                let chunk_size =
                    u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
                o.push_str(&format!(
                    "  VP8 data:     {}\n",
                    human_size(chunk_size as u64)
                ));
            }
        }
        b"VP8L" => {
            o.push_str("  Encoding:     Lossless (VP8L)\n");
            if data.len() >= 25 {
                let chunk_size =
                    u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
                o.push_str(&format!(
                    "  VP8L data:    {}\n",
                    human_size(chunk_size as u64)
                ));
            }
        }
        b"VP8X" => {
            o.push_str("  Encoding:     Extended (VP8X)\n");
            if data.len() >= 30 {
                let flags = data[20];
                let has_icc = flags & 0x20 != 0;
                let has_alpha = flags & 0x10 != 0;
                let has_exif = flags & 0x08 != 0;
                let has_xmp = flags & 0x04 != 0;
                let has_anim = flags & 0x02 != 0;

                let mut features = Vec::new();
                if has_alpha {
                    features.push("Alpha");
                }
                if has_anim {
                    features.push("Animation");
                }
                if has_icc {
                    features.push("ICC Profile");
                }
                if has_exif {
                    features.push("EXIF");
                }
                if has_xmp {
                    features.push("XMP");
                }

                if !features.is_empty() {
                    o.push_str(&format!("  Features:     {}\n", features.join(", ")));
                }

                // Count ANMF frames if animated
                if has_anim {
                    let frame_count = count_webp_frames(data);
                    if frame_count > 0 {
                        o.push_str(&format!("  Frames:       {}\n", frame_count));
                    }
                }
            }
        }
        _ => {
            o.push_str(&format!(
                "  Chunk:        {}\n",
                String::from_utf8_lossy(chunk_type)
            ));
        }
    }

    o.push('\n');
    o
}

fn count_webp_frames(data: &[u8]) -> u32 {
    let mut count = 0u32;
    let mut pos = 12;
    while pos + 8 <= data.len() {
        let chunk = &data[pos..pos + 4];
        let size = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;
        if chunk == b"ANMF" {
            count += 1;
        }
        pos += 8 + size;
        // Chunks are 2-byte aligned
        if size % 2 != 0 {
            pos += 1;
        }
    }
    count
}

// --- TIFF ---

fn format_tiff_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 8 {
        return o;
    }

    let is_le = &data[0..2] == b"II";
    let is_be = &data[0..2] == b"MM";
    if !is_le && !is_be {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      TIFF DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let byte_order = if is_le { "Little-endian (Intel)" } else { "Big-endian (Motorola)" };
    o.push_str(&format!("  Byte order:   {}\n", byte_order));

    let magic = if is_le {
        u16::from_le_bytes([data[2], data[3]])
    } else {
        u16::from_be_bytes([data[2], data[3]])
    };

    if magic == 43 {
        o.push_str("  Version:      BigTIFF (64-bit offsets)\n");
    } else {
        o.push_str(&format!("  Version:      {} (Classic TIFF)\n", magic));
    }

    // Count IFDs
    let first_ifd = if is_le {
        u32::from_le_bytes([data[4], data[5], data[6], data[7]])
    } else {
        u32::from_be_bytes([data[4], data[5], data[6], data[7]])
    } as usize;

    let mut ifd_count = 0u32;
    let mut ifd_offset = first_ifd;
    while ifd_offset > 0 && ifd_offset + 2 <= data.len() {
        ifd_count += 1;
        let num_entries = if is_le {
            u16::from_le_bytes([data[ifd_offset], data[ifd_offset + 1]])
        } else {
            u16::from_be_bytes([data[ifd_offset], data[ifd_offset + 1]])
        } as usize;

        let next_off_pos = ifd_offset + 2 + num_entries * 12;
        if next_off_pos + 4 > data.len() {
            break;
        }
        ifd_offset = if is_le {
            u32::from_le_bytes([
                data[next_off_pos],
                data[next_off_pos + 1],
                data[next_off_pos + 2],
                data[next_off_pos + 3],
            ])
        } else {
            u32::from_be_bytes([
                data[next_off_pos],
                data[next_off_pos + 1],
                data[next_off_pos + 2],
                data[next_off_pos + 3],
            ])
        } as usize;

        if ifd_count > 100 {
            break;
        }
    }

    if ifd_count > 1 {
        o.push_str(&format!("  IFDs:         {} (multi-page)\n", ifd_count));
    }

    o.push('\n');
    o
}

// --- ICO ---

fn format_ico_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 6 {
        return o;
    }

    let reserved = u16::from_le_bytes([data[0], data[1]]);
    let image_type = u16::from_le_bytes([data[2], data[3]]);
    let count = u16::from_le_bytes([data[4], data[5]]);

    if reserved != 0 || (image_type != 1 && image_type != 2) {
        return o;
    }

    let type_str = if image_type == 1 { "ICO" } else { "CUR" };

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str(&format!(
        "                       {} DETAILS\n",
        type_str
    ));
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str(&format!(
        "  Type:         {}\n",
        if image_type == 1 { "Icon" } else { "Cursor" }
    ));
    o.push_str(&format!("  Images:       {}\n", count));

    // Parse directory entries
    o.push('\n');
    o.push_str("  #   Size        BPP    Data Size  Format\n");
    o.push_str("  --- ----------  -----  ---------  ------\n");

    for i in 0..count as usize {
        let entry_off = 6 + i * 16;
        if entry_off + 16 > data.len() {
            break;
        }

        let width = if data[entry_off] == 0 {
            256u32
        } else {
            data[entry_off] as u32
        };
        let height = if data[entry_off + 1] == 0 {
            256u32
        } else {
            data[entry_off + 1] as u32
        };
        let _color_count = data[entry_off + 2];
        let bpp = u16::from_le_bytes([data[entry_off + 6], data[entry_off + 7]]);
        let img_size = u32::from_le_bytes([
            data[entry_off + 8],
            data[entry_off + 9],
            data[entry_off + 10],
            data[entry_off + 11],
        ]);
        let img_offset = u32::from_le_bytes([
            data[entry_off + 12],
            data[entry_off + 13],
            data[entry_off + 14],
            data[entry_off + 15],
        ]);

        // Detect if it's PNG or BMP inside
        let inner_format = if img_offset as usize + 4 <= data.len() {
            if &data[img_offset as usize..img_offset as usize + 4] == b"\x89PNG" {
                "PNG"
            } else {
                "BMP/DIB"
            }
        } else {
            "Unknown"
        };

        o.push_str(&format!(
            "  {:>3} {:>4}x{:<4}   {:>5}  {:>9}  {}\n",
            i + 1,
            width,
            height,
            if bpp > 0 {
                format!("{}", bpp)
            } else {
                "-".to_string()
            },
            human_size(img_size as u64),
            inner_format,
        ));
    }

    o.push('\n');
    o
}

// --- TGA ---

fn format_tga_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 18 {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       TGA DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let id_length = data[0] as usize;
    let colormap_type = data[1];
    let image_type = data[2];
    let colormap_length = u16::from_le_bytes([data[5], data[6]]);
    let colormap_entry_size = data[7];
    let pixel_depth = data[16];
    let descriptor = data[17];

    let image_type_str = match image_type {
        0 => "No image data",
        1 => "Uncompressed color-mapped",
        2 => "Uncompressed true-color",
        3 => "Uncompressed grayscale",
        9 => "RLE color-mapped",
        10 => "RLE true-color",
        11 => "RLE grayscale",
        _ => "Unknown",
    };

    o.push_str(&format!("  Image type:   {} (type {})\n", image_type_str, image_type));
    o.push_str(&format!("  Pixel depth:  {} bits\n", pixel_depth));

    let alpha_bits = descriptor & 0x0F;
    if alpha_bits > 0 {
        o.push_str(&format!("  Alpha bits:   {}\n", alpha_bits));
    }

    let origin = if descriptor & 0x20 != 0 {
        "Top-left"
    } else {
        "Bottom-left"
    };
    o.push_str(&format!("  Origin:       {}\n", origin));

    if colormap_type != 0 {
        o.push_str(&format!(
            "  Color map:    {} entries, {}-bit\n",
            colormap_length, colormap_entry_size
        ));
    }

    if id_length > 0 && 18 + id_length <= data.len() {
        let id = String::from_utf8_lossy(&data[18..18 + id_length]);
        let id_trimmed = id.trim();
        if !id_trimmed.is_empty() {
            o.push_str(&format!("  Image ID:     {}\n", id_trimmed));
        }
    }

    // Check for TGA 2.0 footer
    if data.len() >= 26 {
        let footer = &data[data.len() - 18..];
        if &footer[8..18] == b"TRUEVISION-XFILE." || &footer[8..17] == b"TRUEVISION" {
            o.push_str("  Version:      TGA 2.0\n");
        }
    }

    o.push('\n');
    o
}

// --- DDS ---

fn format_dds_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 128 || &data[0..4] != b"DDS " {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       DDS DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let flags = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let _height = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let _width = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    let pitch_or_linear = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
    let depth = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    let mipmap_count = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);

    // Pixel format at offset 76
    let pf_flags = u32::from_le_bytes([data[80], data[81], data[82], data[83]]);
    let four_cc = &data[84..88];
    let rgb_bit_count = u32::from_le_bytes([data[88], data[89], data[90], data[91]]);

    // Determine format
    let format_str: String = if pf_flags & 0x04 != 0 {
        // DDPF_FOURCC
        match four_cc {
            b"DXT1" => "DXT1 (BC1) - Compressed".to_string(),
            b"DXT2" => "DXT2 - Compressed (premul alpha)".to_string(),
            b"DXT3" => "DXT3 (BC2) - Compressed".to_string(),
            b"DXT4" => "DXT4 - Compressed (premul alpha)".to_string(),
            b"DXT5" => "DXT5 (BC3) - Compressed".to_string(),
            b"DX10" => "DX10 extended header".to_string(),
            b"ATI1" => "ATI1 (BC4) - Compressed".to_string(),
            b"ATI2" => "ATI2 (BC5) - Compressed".to_string(),
            _ => String::from_utf8_lossy(four_cc).to_string(),
        }
    } else if pf_flags & 0x40 != 0 {
        // DDPF_RGB
        match rgb_bit_count {
            32 => "RGBA 32-bit",
            24 => "RGB 24-bit",
            16 => "RGB 16-bit",
            _ => "RGB (uncompressed)",
        }.to_string()
    } else if pf_flags & 0x20000 != 0 {
        "Luminance".to_string()
    } else if pf_flags & 0x02 != 0 {
        "Alpha only".to_string()
    } else {
        "Unknown".to_string()
    };

    o.push_str(&format!("  Format:       {}\n", format_str));

    if depth > 1 {
        o.push_str(&format!("  Depth:        {} (volume texture)\n", depth));
    }

    if mipmap_count > 1 {
        o.push_str(&format!("  Mipmaps:      {}\n", mipmap_count));
    }

    if flags & 0x08 != 0 {
        o.push_str(&format!(
            "  Pitch/Linear: {}\n",
            human_size(pitch_or_linear as u64)
        ));
    }

    // Caps
    let caps = u32::from_le_bytes([data[108], data[109], data[110], data[111]]);
    let caps2 = u32::from_le_bytes([data[112], data[113], data[114], data[115]]);

    let mut features = Vec::new();
    if caps2 & 0x200 != 0 {
        features.push("Cubemap");
    }
    if caps2 & 0x200000 != 0 {
        features.push("Volume");
    }
    if caps & 0x08 != 0 {
        features.push("Complex");
    }
    if !features.is_empty() {
        o.push_str(&format!("  Features:     {}\n", features.join(", ")));
    }

    // DX10 extended header
    if four_cc == b"DX10" && data.len() >= 148 {
        let dxgi_format = u32::from_le_bytes([data[128], data[129], data[130], data[131]]);
        let resource_dim = u32::from_le_bytes([data[132], data[133], data[134], data[135]]);
        let array_size = u32::from_le_bytes([data[140], data[141], data[142], data[143]]);

        let dim_str = match resource_dim {
            2 => "1D Texture",
            3 => "2D Texture",
            4 => "3D Texture",
            _ => "Unknown",
        };
        o.push_str(&format!("  DXGI format:  {}\n", dxgi_format_name(dxgi_format)));
        o.push_str(&format!("  Resource:     {}\n", dim_str));
        if array_size > 1 {
            o.push_str(&format!("  Array size:   {}\n", array_size));
        }
    }

    o.push('\n');
    o
}

fn dxgi_format_name(fmt: u32) -> &'static str {
    match fmt {
        0 => "UNKNOWN",
        2 => "R32G32B32A32_FLOAT",
        10 => "R16G16B16A16_FLOAT",
        28 => "R8G8B8A8_UNORM",
        29 => "R8G8B8A8_UNORM_SRGB",
        61 => "R8_UNORM",
        71 => "BC1_UNORM",
        72 => "BC1_UNORM_SRGB",
        74 => "BC2_UNORM",
        75 => "BC2_UNORM_SRGB",
        77 => "BC3_UNORM",
        78 => "BC3_UNORM_SRGB",
        80 => "BC4_UNORM",
        83 => "BC5_UNORM",
        87 => "B8G8R8A8_UNORM",
        95 => "BC6H_UF16",
        96 => "BC6H_SF16",
        98 => "BC7_UNORM",
        99 => "BC7_UNORM_SRGB",
        _ => "Other",
    }
}

// --- HDR (Radiance) ---

fn format_hdr_details(data: &[u8]) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     HDR/RGBE DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // Parse the text header (lines until empty line)
    let header_end = find_double_newline(data).unwrap_or(data.len().min(4096));
    let header_text = String::from_utf8_lossy(&data[..header_end]);

    for line in header_text.lines() {
        if line.starts_with("#?") {
            o.push_str(&format!("  Program:      {}\n", &line[2..]));
        } else if line.starts_with("FORMAT=") {
            o.push_str(&format!("  Format:       {}\n", &line[7..]));
        } else if line.starts_with("EXPOSURE=") {
            o.push_str(&format!("  Exposure:     {}\n", &line[9..]));
        } else if line.starts_with("GAMMA=") {
            o.push_str(&format!("  Gamma:        {}\n", &line[6..]));
        } else if line.starts_with("PRIMARIES=") {
            o.push_str(&format!("  Primaries:    {}\n", &line[10..]));
        } else if line.starts_with("SOFTWARE=") {
            o.push_str(&format!("  Software:     {}\n", &line[9..]));
        }
    }

    o.push('\n');
    o
}

fn find_double_newline(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(1).min(8192) {
        if data[i] == b'\n' && data[i + 1] == b'\n' {
            return Some(i);
        }
    }
    None
}

// --- OpenEXR ---

fn format_exr_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 8 {
        return o;
    }

    // EXR magic: 76 2F 31 01
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0x01312F76 {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     OpenEXR DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let version = data[4];
    let flags = data[5];

    o.push_str(&format!("  Version:      {}\n", version));

    let mut features = Vec::new();
    if flags & 0x02 != 0 {
        features.push("Tiled");
    } else {
        features.push("Scanline");
    }
    if flags & 0x04 != 0 {
        features.push("Long names");
    }
    if flags & 0x08 != 0 {
        features.push("Non-image (deep data)");
    }
    if flags & 0x10 != 0 {
        features.push("Multi-part");
    }
    if !features.is_empty() {
        o.push_str(&format!("  Features:     {}\n", features.join(", ")));
    }

    o.push('\n');
    o
}

// --- PNM (PBM/PGM/PPM/PAM) ---

fn format_pnm_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 3 || data[0] != b'P' {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       PNM DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let magic = data[1];
    let (format_str, encoding) = match magic {
        b'1' => ("PBM (Portable Bitmap)", "ASCII"),
        b'2' => ("PGM (Portable Graymap)", "ASCII"),
        b'3' => ("PPM (Portable Pixmap)", "ASCII"),
        b'4' => ("PBM (Portable Bitmap)", "Binary"),
        b'5' => ("PGM (Portable Graymap)", "Binary"),
        b'6' => ("PPM (Portable Pixmap)", "Binary"),
        b'7' => ("PAM (Portable Arbitrary Map)", "Binary"),
        _ => ("Unknown", "Unknown"),
    };

    o.push_str(&format!("  Format:       {}\n", format_str));
    o.push_str(&format!("  Encoding:     {}\n", encoding));

    o.push('\n');
    o
}

// --- QOI ---

fn format_qoi_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 14 || &data[0..4] != b"qoif" {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       QOI DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let channels = data[12];
    let colorspace = data[13];

    o.push_str(&format!(
        "  Channels:     {} ({})\n",
        channels,
        match channels {
            3 => "RGB",
            4 => "RGBA",
            _ => "Unknown",
        }
    ));
    o.push_str(&format!(
        "  Colorspace:   {}\n",
        match colorspace {
            0 => "sRGB with linear alpha",
            1 => "All channels linear",
            _ => "Unknown",
        }
    ));

    o.push('\n');
    o
}

// --- Farbfeld ---

fn format_farbfeld_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 16 || &data[0..8] != b"farbfeld" {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                    FARBFELD DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str("  Channels:     4 (RGBA)\n");
    o.push_str("  Bit depth:    16 bits per channel\n");
    o.push_str("  Total:        64 bits per pixel\n");

    o.push('\n');
    o
}

// --- AVIF ---

fn format_avif_details(data: &[u8]) -> String {
    let mut o = String::new();

    if data.len() < 12 {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      AVIF DETAILS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // AVIF uses ISOBMFF (ISO Base Media File Format) container
    // Check for ftyp box
    if data.len() >= 12 && &data[4..8] == b"ftyp" {
        let brand = String::from_utf8_lossy(&data[8..12]);
        o.push_str(&format!("  Major brand:  {}\n", brand.trim()));

        // Scan compatible brands
        let box_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if box_size <= data.len() && box_size > 16 {
            let mut brands = Vec::new();
            let mut pos = 16;
            while pos + 4 <= box_size {
                let b = String::from_utf8_lossy(&data[pos..pos + 4]).trim().to_string();
                if !b.is_empty() && b != brand.trim() {
                    brands.push(b);
                }
                pos += 4;
            }
            if !brands.is_empty() {
                o.push_str(&format!(
                    "  Compatible:   {}\n",
                    brands.join(", ")
                ));
            }
        }
    }

    o.push_str("  Codec:        AV1 (AOMedia Video 1)\n");
    o.push_str("  Container:    ISOBMFF (HEIF)\n");

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Section 3: EXIF data
// ---------------------------------------------------------------------------

fn format_exif_data(path: &str) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bufreader = std::io::BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut bufreader).ok()?;

    let fields: Vec<_> = exif.fields().collect();
    if fields.is_empty() {
        return None;
    }

    let mut o = String::new();

    // Collect camera info
    let mut camera_lines: Vec<String> = Vec::new();
    let mut shooting_lines: Vec<String> = Vec::new();
    let mut gps_lines: Vec<String> = Vec::new();
    let mut other_lines: Vec<String> = Vec::new();

    for field in &fields {
        let tag = field.tag;
        let value = field.display_value().with_unit(&exif).to_string();

        if value.is_empty()
            || value == "(unknown)"
            || value.starts_with("\"\", \"\"")
            || value == "unknown"
        {
            continue;
        }

        use exif::Tag;

        match tag {
            // Camera info
            Tag::Make => {
                camera_lines.push(format!("  Make:         {}", value.trim()));
            }
            Tag::Model => {
                camera_lines.push(format!("  Model:        {}", value.trim()));
            }
            Tag::LensModel => {
                camera_lines.push(format!("  Lens:         {}", value.trim()));
            }
            Tag::LensMake => {
                camera_lines.push(format!("  Lens make:    {}", value.trim()));
            }
            Tag::BodySerialNumber => {
                camera_lines.push(format!("  Serial:       {}", value.trim()));
            }
            Tag::Software => {
                camera_lines.push(format!("  Software:     {}", value.trim()));
            }
            Tag::Artist => {
                camera_lines.push(format!("  Artist:       {}", value.trim()));
            }
            Tag::Copyright => {
                camera_lines.push(format!("  Copyright:    {}", value.trim()));
            }

            // Shooting settings
            Tag::DateTimeOriginal => {
                shooting_lines.push(format!("  Date taken:   {}", value));
            }
            Tag::ExposureTime => {
                shooting_lines.push(format!("  Exposure:     {}", value));
            }
            Tag::FNumber => {
                shooting_lines.push(format!("  F-number:     {}", value));
            }
            Tag::PhotographicSensitivity => {
                shooting_lines.push(format!("  ISO:          {}", value));
            }
            Tag::FocalLength => {
                if !value.contains("inf") {
                    shooting_lines.push(format!("  Focal length: {}", value));
                }
            }
            Tag::FocalLengthIn35mmFilm => {
                if !value.contains("unknown") {
                    shooting_lines.push(format!("  35mm equiv:   {}", value));
                }
            }
            Tag::Flash => {
                // Clean up verbose flash descriptions
                let clean = value
                    .replace(" 0 (unknown)", "")
                    .replace(", no return light detection function", "");
                shooting_lines.push(format!("  Flash:        {}", clean));
            }
            Tag::WhiteBalance => {
                shooting_lines.push(format!("  White bal:    {}", value));
            }
            Tag::MeteringMode => {
                if value != "unknown" {
                    shooting_lines.push(format!("  Metering:     {}", value));
                }
            }
            Tag::ExposureMode => {
                shooting_lines.push(format!("  Exp. mode:    {}", value));
            }
            Tag::ExposureBiasValue => {
                shooting_lines.push(format!("  Exp. bias:    {}", value));
            }
            Tag::ExposureProgram => {
                shooting_lines.push(format!("  Exp. program: {}", value));
            }
            Tag::SceneCaptureType => {
                shooting_lines.push(format!("  Scene type:   {}", value));
            }
            Tag::DigitalZoomRatio => {
                if value != "1" && value != "0" {
                    shooting_lines.push(format!("  Digital zoom: {}", value));
                }
            }
            Tag::BrightnessValue => {
                shooting_lines.push(format!("  Brightness:   {}", value));
            }
            Tag::LightSource => {
                shooting_lines.push(format!("  Light source: {}", value));
            }
            Tag::SubjectDistance => {
                shooting_lines.push(format!("  Subject dist: {}", value));
            }
            Tag::MaxApertureValue => {
                shooting_lines.push(format!("  Max aperture: {}", value));
            }
            Tag::Sharpness => {
                shooting_lines.push(format!("  Sharpness:    {}", value));
            }
            Tag::Contrast => {
                shooting_lines.push(format!("  Contrast:     {}", value));
            }
            Tag::Saturation => {
                shooting_lines.push(format!("  Saturation:   {}", value));
            }

            // GPS
            Tag::GPSLatitude => {
                gps_lines.push(format!("  Latitude:     {}", value));
            }
            Tag::GPSLatitudeRef => {
                gps_lines.push(format!("  Lat ref:      {}", value));
            }
            Tag::GPSLongitude => {
                gps_lines.push(format!("  Longitude:    {}", value));
            }
            Tag::GPSLongitudeRef => {
                gps_lines.push(format!("  Long ref:     {}", value));
            }
            Tag::GPSAltitude => {
                gps_lines.push(format!("  Altitude:     {}", value));
            }
            Tag::GPSAltitudeRef => {
                gps_lines.push(format!("  Alt ref:      {}", value));
            }
            Tag::GPSTimeStamp => {
                gps_lines.push(format!("  GPS time:     {}", value));
            }
            Tag::GPSDateStamp => {
                gps_lines.push(format!("  GPS date:     {}", value));
            }
            Tag::GPSSpeed => {
                gps_lines.push(format!("  GPS speed:    {}", value));
            }
            Tag::GPSImgDirection => {
                gps_lines.push(format!("  Direction:    {}", value));
            }

            // Other interesting fields
            Tag::Orientation => {
                let orientation_str = match value.as_str() {
                    "1" | "row 0 at top and column 0 at left" => "Normal (1)",
                    "2" => "Mirrored horizontal (2)",
                    "3" | "row 0 at bottom and column 0 at right" => "Rotated 180 (3)",
                    "4" => "Mirrored vertical (4)",
                    "5" => "Mirrored horizontal, rotated 270 CW (5)",
                    "6" | "row 0 at right and column 0 at top" => "Rotated 90 CW (6)",
                    "7" => "Mirrored horizontal, rotated 90 CW (7)",
                    "8" | "row 0 at left and column 0 at bottom" => "Rotated 270 CW (8)",
                    _ => &value,
                };
                other_lines.push(format!("  Orientation:  {}", orientation_str));
            }
            Tag::ColorSpace => {
                other_lines.push(format!("  Color space:  {}", value));
            }
            Tag::PixelXDimension => {
                other_lines.push(format!("  EXIF width:   {}", value));
            }
            Tag::PixelYDimension => {
                other_lines.push(format!("  EXIF height:  {}", value));
            }
            Tag::ImageDescription => {
                other_lines.push(format!("  Description:  {}", value.trim()));
            }
            Tag::UserComment => {
                let trimmed = value.trim();
                if !trimmed.is_empty() && trimmed != "ASCII" && trimmed.len() > 1 {
                    other_lines.push(format!("  Comment:      {}", trimmed));
                }
            }

            _ => {}
        }
    }

    // EXIF section
    if !camera_lines.is_empty() || !shooting_lines.is_empty() || !other_lines.is_empty() {
        o.push_str("═══════════════════════════════════════════════════════════════\n");
        o.push_str("                        EXIF DATA\n");
        o.push_str("═══════════════════════════════════════════════════════════════\n\n");

        if !camera_lines.is_empty() {
            o.push_str("  --- Camera ---\n");
            for line in &camera_lines {
                o.push_str(line);
                o.push('\n');
            }
            o.push('\n');
        }

        if !shooting_lines.is_empty() {
            o.push_str("  --- Shooting Settings ---\n");
            for line in &shooting_lines {
                o.push_str(line);
                o.push('\n');
            }
            o.push('\n');
        }

        if !other_lines.is_empty() {
            o.push_str("  --- Other ---\n");
            for line in &other_lines {
                o.push_str(line);
                o.push('\n');
            }
            o.push('\n');
        }
    }

    // GPS section
    if !gps_lines.is_empty() {
        o.push_str("═══════════════════════════════════════════════════════════════\n");
        o.push_str("                        GPS DATA\n");
        o.push_str("═══════════════════════════════════════════════════════════════\n\n");

        for line in &gps_lines {
            o.push_str(line);
            o.push('\n');
        }
        o.push('\n');
    }

    if o.is_empty() {
        None
    } else {
        Some(o)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_name(format: image::ImageFormat) -> String {
    match format {
        image::ImageFormat::Jpeg => "JPEG".to_string(),
        image::ImageFormat::Png => "PNG".to_string(),
        image::ImageFormat::Gif => "GIF".to_string(),
        image::ImageFormat::Bmp => "BMP".to_string(),
        image::ImageFormat::WebP => "WebP".to_string(),
        image::ImageFormat::Tiff => "TIFF".to_string(),
        image::ImageFormat::Ico => "ICO/CUR".to_string(),
        image::ImageFormat::Avif => "AVIF".to_string(),
        image::ImageFormat::Tga => "TGA (Targa)".to_string(),
        image::ImageFormat::Dds => "DDS (DirectDraw Surface)".to_string(),
        image::ImageFormat::Hdr => "HDR (Radiance RGBE)".to_string(),
        image::ImageFormat::OpenExr => "OpenEXR".to_string(),
        image::ImageFormat::Pnm => "PNM (Portable Any Map)".to_string(),
        image::ImageFormat::Qoi => "QOI (Quite OK Image)".to_string(),
        image::ImageFormat::Farbfeld => "Farbfeld".to_string(),
        _ => format!("{:?}", format),
    }
}

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

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}
