use super::cp437::CP437_TABLE;

/// Compute line byte offsets for a text file
pub fn compute_line_offsets(bytes: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(bytes.len() / 40); // Estimate ~40 chars per line
    offsets.push(0); // First line starts at byte 0

    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' && i + 1 < bytes.len() {
            offsets.push(i + 1);
        }
    }

    offsets
}

/// Format a range of bytes as hex dump lines
pub fn format_hex_dump_range(bytes: &[u8], term_width: usize, start_byte: usize, end_byte: usize) -> Vec<String> {
    // Calculate bytes per line based on terminal width
    let calc_bytes = if term_width > 20 {
        ((term_width.saturating_sub(12)) * 8) / 33
    } else {
        8
    };
    let bytes_per_line = (calc_bytes / 8 * 8).clamp(8, 64);
    let group_size = 8;
    let num_groups = bytes_per_line / group_size;

    let mut lines = Vec::new();
    let start_line = start_byte / bytes_per_line;
    let actual_start = start_line * bytes_per_line;

    for (i, chunk) in bytes[actual_start..end_byte.min(bytes.len())]
        .chunks(bytes_per_line)
        .enumerate() {
        let mut line = String::with_capacity(term_width);
        let offset = actual_start + i * bytes_per_line;

        // Offset (uppercase)
        line.push_str(&format!("{:08X}  ", offset));

        // Hex bytes with group separators (uppercase)
        for (j, byte) in chunk.iter().enumerate() {
            line.push_str(&format!("{:02X} ", byte));
            if (j + 1) % group_size == 0 && (j + 1) / group_size < num_groups {
                line.push(' ');
            }
        }

        // Padding for incomplete lines
        for j in chunk.len()..bytes_per_line {
            line.push_str("   ");
            if (j + 1) % group_size == 0 && (j + 1) / group_size < num_groups {
                line.push(' ');
            }
        }

        // CP437 representation
        line.push_str(" |");
        for byte in chunk {
            line.push(CP437_TABLE[*byte as usize]);
        }
        line.push('|');

        lines.push(line);
    }
    lines
}

/// Format a range of bytes as CP437 text lines
pub fn format_cp437_range(bytes: &[u8], term_width: usize, start_line: usize, num_lines: usize) -> Vec<String> {
    let chars_per_line = term_width;
    let mut lines = Vec::new();
    let start_byte = start_line * chars_per_line;

    if start_byte >= bytes.len() {
        return lines;
    }

    let end_byte = ((start_line + num_lines) * chars_per_line).min(bytes.len());

    for chunk in bytes[start_byte..end_byte].chunks(chars_per_line) {
        let mut line = String::with_capacity(chars_per_line);
        for byte in chunk {
            line.push(CP437_TABLE[*byte as usize]);
        }
        lines.push(line);
    }

    lines
}

/// Calculate number of lines for CP437 view
pub fn cp437_line_count(bytes_len: usize, term_width: usize) -> usize {
    if bytes_len == 0 || term_width == 0 {
        0
    } else {
        bytes_len.div_ceil(term_width)
    }
}
