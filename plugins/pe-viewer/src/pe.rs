//! PE (Portable Executable) file parsing and rich ASCII rendering module

use std::fs::File;
use std::io::Read;

/// DOS MZ magic bytes
const MZ_MAGIC: [u8; 2] = [0x4D, 0x5A];

/// PE signature bytes
const PE_SIG: [u8; 4] = [b'P', b'E', 0, 0];

/// Check if the given bytes start with MZ magic
pub fn is_pe_magic(magic: &[u8]) -> bool {
    magic.len() >= 2 && magic[..2] == MZ_MAGIC
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

struct PeReader<'a> {
    data: &'a [u8],
}

impl<'a> PeReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    #[allow(dead_code)]
    fn u8(&self, off: usize) -> u8 {
        if off >= self.data.len() {
            return 0;
        }
        self.data[off]
    }

    fn u16(&self, off: usize) -> u16 {
        if off + 2 > self.data.len() {
            return 0;
        }
        u16::from_le_bytes([self.data[off], self.data[off + 1]])
    }

    fn u32(&self, off: usize) -> u32 {
        if off + 4 > self.data.len() {
            return 0;
        }
        u32::from_le_bytes([
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
        ])
    }

    fn u64(&self, off: usize) -> u64 {
        if off + 8 > self.data.len() {
            return 0;
        }
        u64::from_le_bytes([
            self.data[off],
            self.data[off + 1],
            self.data[off + 2],
            self.data[off + 3],
            self.data[off + 4],
            self.data[off + 5],
            self.data[off + 6],
            self.data[off + 7],
        ])
    }

    fn read_cstr(&self, offset: usize) -> String {
        if offset >= self.data.len() {
            return String::new();
        }
        let mut end = offset;
        while end < self.data.len() && self.data[end] != 0 {
            end += 1;
        }
        String::from_utf8_lossy(&self.data[offset..end]).into_owned()
    }

    fn len(&self) -> usize {
        self.data.len()
    }
}

#[derive(Clone)]
struct CoffHeader {
    machine: u16,
    num_sections: u16,
    timestamp: u32,
    characteristics: u16,
}

#[derive(Clone)]
struct OptionalHeader {
    magic: u16, // 0x10b = PE32, 0x20b = PE32+
    entry_point: u32,
    image_base: u64,
    section_alignment: u32,
    file_alignment: u32,
    subsystem: u16,
    dll_characteristics: u16,
    size_of_headers: u32,
    data_directories: Vec<DataDirectory>,
}

impl OptionalHeader {
    fn is_pe32plus(&self) -> bool {
        self.magic == 0x20b
    }
}

#[derive(Clone)]
struct DataDirectory {
    rva: u32,
    size: u32,
}

#[derive(Clone)]
struct SectionHeader {
    name: String,
    virtual_size: u32,
    virtual_addr: u32,
    raw_size: u32,
    raw_addr: u32,
    characteristics: u32,
}

#[derive(Clone)]
struct ImportEntry {
    dll_name: String,
    functions: Vec<String>,
}

#[derive(Clone)]
struct ExportEntry {
    ordinal: u16,
    rva: u32,
    name: String,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_coff_header(r: &PeReader, offset: usize) -> Result<CoffHeader, String> {
    if offset + 20 > r.len() {
        return Err("File too small for COFF header".into());
    }
    Ok(CoffHeader {
        machine: r.u16(offset),
        num_sections: r.u16(offset + 2),
        timestamp: r.u32(offset + 4),
        characteristics: r.u16(offset + 18),
    })
}

fn parse_optional_header(
    r: &PeReader,
    offset: usize,
    size: u16,
) -> Result<OptionalHeader, String> {
    if (offset + size as usize) > r.len() || size < 2 {
        return Err("File too small for optional header".into());
    }

    let magic = r.u16(offset);
    let is_pe32plus = magic == 0x20b;

    let entry_point = r.u32(offset + 16);

    let (image_base, subsystem_off, dll_chars_off, num_rva_off) = if is_pe32plus {
        // PE32+: image base is 8 bytes at offset+24
        (r.u64(offset + 24), offset + 68, offset + 70, offset + 108)
    } else {
        // PE32: image base is 4 bytes at offset+28
        (
            r.u32(offset + 28) as u64,
            offset + 68,
            offset + 70,
            offset + 96,
        )
    };

    let section_alignment = r.u32(offset + 32);
    let file_alignment = r.u32(offset + 36);
    let size_of_headers = r.u32(offset + 60);
    let subsystem = r.u16(subsystem_off);
    let dll_characteristics = r.u16(dll_chars_off);

    // Data directories
    let num_rva = r.u32(num_rva_off) as usize;
    let dd_start = num_rva_off + 4;
    let mut data_directories = Vec::new();
    for i in 0..num_rva.min(16) {
        let base = dd_start + i * 8;
        if base + 8 > r.len() {
            break;
        }
        data_directories.push(DataDirectory {
            rva: r.u32(base),
            size: r.u32(base + 4),
        });
    }

    Ok(OptionalHeader {
        magic,
        entry_point,
        image_base,
        section_alignment,
        file_alignment,
        subsystem,
        dll_characteristics,
        size_of_headers,
        data_directories,
    })
}

fn parse_section_headers(
    r: &PeReader,
    offset: usize,
    count: u16,
) -> Vec<SectionHeader> {
    let mut sections = Vec::new();
    for i in 0..count as usize {
        let base = offset + i * 40;
        if base + 40 > r.len() {
            break;
        }

        // Section name: 8 bytes, null-padded
        let name_bytes = &r.data[base..base + 8];
        let name_end = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
        let name = String::from_utf8_lossy(&name_bytes[..name_end]).into_owned();

        sections.push(SectionHeader {
            name,
            virtual_size: r.u32(base + 8),
            virtual_addr: r.u32(base + 12),
            raw_size: r.u32(base + 16),
            raw_addr: r.u32(base + 20),
            characteristics: r.u32(base + 36),
        });
    }
    sections
}

fn rva_to_file_offset(sections: &[SectionHeader], rva: u32) -> Option<usize> {
    for s in sections {
        if rva >= s.virtual_addr && rva < s.virtual_addr + s.raw_size {
            return Some((rva - s.virtual_addr + s.raw_addr) as usize);
        }
    }
    None
}


fn parse_imports_full(
    r: &PeReader,
    sections: &[SectionHeader],
    data_dirs: &[DataDirectory],
    is_pe32plus: bool,
) -> Vec<ImportEntry> {
    if data_dirs.len() <= 1 || data_dirs[1].rva == 0 || data_dirs[1].size == 0 {
        return Vec::new();
    }

    let import_rva = data_dirs[1].rva;
    let import_off = match rva_to_file_offset(sections, import_rva) {
        Some(o) => o,
        None => return Vec::new(),
    };

    let entry_size: usize = if is_pe32plus { 8 } else { 4 };
    let ordinal_flag: u64 = if is_pe32plus {
        0x8000_0000_0000_0000
    } else {
        0x8000_0000
    };

    let mut imports = Vec::new();
    let mut idx = 0;

    loop {
        let base = import_off + idx * 20;
        if base + 20 > r.len() {
            break;
        }

        let ilt_rva = r.u32(base);
        let name_rva = r.u32(base + 12);

        if ilt_rva == 0 && name_rva == 0 {
            break;
        }

        let dll_name = match rva_to_file_offset(sections, name_rva) {
            Some(off) => r.read_cstr(off),
            None => String::from("(unknown)"),
        };

        let mut functions = Vec::new();

        if let Some(ilt_off) = rva_to_file_offset(sections, ilt_rva) {
            let mut fi = 0;
            loop {
                let e_off = ilt_off + fi * entry_size;
                if e_off + entry_size > r.len() {
                    break;
                }

                let entry_val = if is_pe32plus {
                    r.u64(e_off)
                } else {
                    r.u32(e_off) as u64
                };

                if entry_val == 0 {
                    break;
                }

                if entry_val & ordinal_flag != 0 {
                    // Import by ordinal
                    let ord = entry_val & 0xFFFF;
                    functions.push(format!("Ordinal {}", ord));
                } else {
                    // Import by name — hint/name table entry
                    let hint_rva = (entry_val & 0x7FFF_FFFF) as u32;
                    if let Some(hint_off) = rva_to_file_offset(sections, hint_rva) {
                        // Skip 2-byte hint
                        let fname = r.read_cstr(hint_off + 2);
                        if !fname.is_empty() {
                            functions.push(fname);
                        } else {
                            functions.push(format!("(hint RVA 0x{:08X})", hint_rva));
                        }
                    } else {
                        functions.push(format!("(RVA 0x{:08X})", hint_rva));
                    }
                }

                fi += 1;
                if fi > 4096 {
                    break;
                }
            }
        }

        imports.push(ImportEntry {
            dll_name,
            functions,
        });
        idx += 1;

        if idx > 512 {
            break;
        }
    }

    imports
}

fn parse_exports(
    r: &PeReader,
    sections: &[SectionHeader],
    data_dirs: &[DataDirectory],
) -> Vec<ExportEntry> {
    // Export table is data directory index 0
    if data_dirs.is_empty() || data_dirs[0].rva == 0 || data_dirs[0].size == 0 {
        return Vec::new();
    }

    let export_rva = data_dirs[0].rva;
    let export_off = match rva_to_file_offset(sections, export_rva) {
        Some(o) => o,
        None => return Vec::new(),
    };

    if export_off + 40 > r.len() {
        return Vec::new();
    }

    let ordinal_base = r.u32(export_off + 16) as u16;
    let num_functions = r.u32(export_off + 20) as usize;
    let num_names = r.u32(export_off + 24) as usize;
    let addr_table_rva = r.u32(export_off + 28);
    let name_ptr_rva = r.u32(export_off + 32);
    let ordinal_table_rva = r.u32(export_off + 36);

    let addr_off = match rva_to_file_offset(sections, addr_table_rva) {
        Some(o) => o,
        None => return Vec::new(),
    };
    let name_off = match rva_to_file_offset(sections, name_ptr_rva) {
        Some(o) => o,
        None => return Vec::new(),
    };
    let ord_off = match rva_to_file_offset(sections, ordinal_table_rva) {
        Some(o) => o,
        None => return Vec::new(),
    };

    // Build name-to-ordinal mapping
    let mut ordinal_names: Vec<(u16, String)> = Vec::new();
    for i in 0..num_names.min(4096) {
        let name_rva_val = r.u32(name_off + i * 4);
        let ordinal_index = r.u16(ord_off + i * 2);
        let name = match rva_to_file_offset(sections, name_rva_val) {
            Some(off) => r.read_cstr(off),
            None => String::new(),
        };
        ordinal_names.push((ordinal_index, name));
    }

    let mut exports = Vec::new();
    for i in 0..num_functions.min(4096) {
        let func_rva = r.u32(addr_off + i * 4);
        if func_rva == 0 {
            continue;
        }
        let ordinal = ordinal_base + i as u16;
        let name = ordinal_names
            .iter()
            .find(|(idx, _)| *idx == i as u16)
            .map(|(_, n)| n.clone())
            .unwrap_or_default();

        exports.push(ExportEntry {
            ordinal,
            rva: func_rva,
            name,
        });
    }

    exports
}

// ---------------------------------------------------------------------------
// Authenticode signature parsing
// ---------------------------------------------------------------------------

struct CertInfo {
    subject: String,
    issuer: String,
    serial: String,
    not_before: String,
    not_after: String,
    is_ca: bool,
    is_self_signed: bool,
    sig_algorithm: String,
}

struct SignatureInfo {
    digest_algorithm: String,
    authenticode_digest: String,
    certs: Vec<CertInfo>,
    status: String,
}

fn parse_signature(data: &[u8]) -> Option<SignatureInfo> {
    use pesign::{PeSign, PeSignStatus, VerifyOption};

    let pesign = match PeSign::from_pe_data(data) {
        Ok(Some(ps)) => ps,
        Ok(None) => return None,
        Err(_) => return None,
    };

    let digest_algorithm = format!("{:?}", pesign.authenticode_digest_algorithm);
    let authenticode_digest = pesign.authenticode_digest.clone();

    let certs: Vec<CertInfo> = pesign
        .signed_data
        .cert_list
        .iter()
        .map(|cert| {
            let serial = cert
                .serial_number
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(":");

            CertInfo {
                subject: cert.subject.to_string(),
                issuer: cert.issuer.to_string(),
                serial,
                not_before: cert.validity.not_before.to_string(),
                not_after: cert.validity.not_after.to_string(),
                is_ca: cert.is_ca(),
                is_self_signed: cert.is_selfsigned(),
                sig_algorithm: format!("{:?}", cert.signature_algorithm),
            }
        })
        .collect();

    // Verify signature
    let verify_opt = VerifyOption {
        check_time: false,
        ..Default::default()
    };
    let status = match pesign.verify_pe_data(data, &verify_opt) {
        Ok(PeSignStatus::Valid) => "VALID".to_string(),
        Ok(PeSignStatus::Expired) => "VALID (certificate expired)".to_string(),
        Ok(PeSignStatus::Invalid) => "INVALID (signature mismatch)".to_string(),
        Ok(PeSignStatus::UntrustedCertificateChain) => "VALID (untrusted chain)".to_string(),
        Err(e) => format!("ERROR ({})", e),
    };

    // Re-verify with time check
    let verify_opt_time = VerifyOption {
        check_time: true,
        ..Default::default()
    };
    let status_with_time = match pesign.verify_pe_data(data, &verify_opt_time) {
        Ok(PeSignStatus::Valid) => status,
        Ok(PeSignStatus::Expired) => {
            if status.starts_with("VALID") {
                "VALID (certificate expired)".to_string()
            } else {
                status
            }
        }
        _ => status,
    };

    Some(SignatureInfo {
        digest_algorithm,
        authenticode_digest,
        certs,
        status: status_with_time,
    })
}

// ---------------------------------------------------------------------------
// Rendering: Digital Signature
// ---------------------------------------------------------------------------

fn format_signature(sig: &SignatureInfo) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                    DIGITAL SIGNATURE\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str(&format!("  Status:       {}\n", sig.status));
    o.push_str(&format!("  Digest algo:  {}\n", sig.digest_algorithm));
    o.push_str(&format!("  Digest:       {}\n", sig.authenticode_digest));
    o.push('\n');

    // Separate signer certs from CA certs
    let signers: Vec<&CertInfo> = sig.certs.iter().filter(|c| !c.is_ca).collect();
    let cas: Vec<&CertInfo> = sig.certs.iter().filter(|c| c.is_ca).collect();

    if !signers.is_empty() {
        o.push_str("  Signer certificate(s):\n");
        for cert in &signers {
            format_cert_block(&mut o, cert);
        }
    }

    if !cas.is_empty() {
        o.push_str("  CA certificate(s):\n");
        for cert in &cas {
            format_cert_block(&mut o, cert);
        }
    }

    // If all certs are CA (e.g. self-signed), just list them all
    if signers.is_empty() && !cas.is_empty() {
        // Already printed above
    } else if sig.certs.is_empty() {
        o.push_str("  (no certificates found in signature)\n\n");
    }

    o
}

fn format_cert_block(o: &mut String, cert: &CertInfo) {
    o.push_str("  ┌─────────────────────────────────────────────────────────┐\n");
    o.push_str(&format!("  │ Subject:    {}\n", cert.subject));
    o.push_str(&format!("  │ Issuer:     {}\n", cert.issuer));
    o.push_str(&format!("  │ Serial:     {}\n", cert.serial));
    o.push_str(&format!("  │ Valid from: {}\n", cert.not_before));
    o.push_str(&format!("  │ Valid to:   {}\n", cert.not_after));
    o.push_str(&format!("  │ Algorithm:  {}\n", cert.sig_algorithm));
    let mut flags = Vec::new();
    if cert.is_ca {
        flags.push("CA");
    }
    if cert.is_self_signed {
        flags.push("Self-signed");
    }
    if !flags.is_empty() {
        o.push_str(&format!("  │ Flags:      {}\n", flags.join(", ")));
    }
    o.push_str("  └─────────────────────────────────────────────────────────┘\n");
}

// ---------------------------------------------------------------------------
// Entropy calculation
// ---------------------------------------------------------------------------

/// Compute Shannon entropy of a byte slice (0.0 = uniform, 8.0 = max random)
fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let len = data.len() as f64;
    let mut entropy = 0.0;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}

// ---------------------------------------------------------------------------
// Formatting helpers
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

fn format_size_commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut result = String::new();
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(b as char);
    }
    format!("{} B", result)
}

fn machine_str(machine: u16) -> &'static str {
    match machine {
        0x0 => "Unknown",
        0x14c => "Intel 386 (x86)",
        0x166 => "MIPS R4000",
        0x1a2 => "Hitachi SH3",
        0x1a6 => "Hitachi SH4",
        0x1c0 => "ARM",
        0x1c4 => "ARM Thumb-2 (ARMv7)",
        0x5064 => "RISC-V (64-bit)",
        0x8664 => "AMD x86-64",
        0xaa64 => "AArch64 (ARM64)",
        _ => "Unknown",
    }
}

fn subsystem_str(subsystem: u16) -> &'static str {
    match subsystem {
        0 => "Unknown",
        1 => "Native",
        2 => "Windows GUI",
        3 => "Windows Console",
        5 => "OS/2 Console",
        7 => "POSIX Console",
        9 => "Windows CE GUI",
        10 => "EFI Application",
        11 => "EFI Boot Service Driver",
        12 => "EFI Runtime Driver",
        13 => "EFI ROM",
        14 => "Xbox",
        16 => "Windows Boot Application",
        _ => "Unknown",
    }
}

fn pe_type_str(characteristics: u16) -> &'static str {
    if characteristics & 0x2000 != 0 {
        "DLL (Dynamic Link Library)"
    } else if characteristics & 0x0002 != 0 {
        "EXE (Executable)"
    } else {
        "Unknown"
    }
}

fn dll_characteristics_str(chars: u16) -> String {
    let mut flags = Vec::new();
    if chars & 0x0020 != 0 {
        flags.push("HIGH_ENTROPY_VA");
    }
    if chars & 0x0040 != 0 {
        flags.push("DYNAMIC_BASE");
    }
    if chars & 0x0080 != 0 {
        flags.push("FORCE_INTEGRITY");
    }
    if chars & 0x0100 != 0 {
        flags.push("NX_COMPAT");
    }
    if chars & 0x0200 != 0 {
        flags.push("NO_ISOLATION");
    }
    if chars & 0x0400 != 0 {
        flags.push("NO_SEH");
    }
    if chars & 0x0800 != 0 {
        flags.push("NO_BIND");
    }
    if chars & 0x1000 != 0 {
        flags.push("APPCONTAINER");
    }
    if chars & 0x2000 != 0 {
        flags.push("WDM_DRIVER");
    }
    if chars & 0x4000 != 0 {
        flags.push("GUARD_CF");
    }
    if chars & 0x8000 != 0 {
        flags.push("TERMINAL_SERVER_AWARE");
    }
    if flags.is_empty() {
        String::from("(none)")
    } else {
        flags.join(" | ")
    }
}

fn section_chars_str(chars: u32) -> String {
    let mut s = String::new();
    // R/W/X
    if chars & 0x4000_0000 != 0 {
        s.push('R');
    } else {
        s.push('-');
    }
    if chars & 0x8000_0000 != 0 {
        s.push('W');
    } else {
        s.push('-');
    }
    if chars & 0x2000_0000 != 0 {
        s.push('X');
    } else {
        s.push('-');
    }

    // Content flags
    if chars & 0x0000_0020 != 0 {
        s.push_str(" CODE");
    }
    if chars & 0x0000_0040 != 0 {
        s.push_str(" IDATA");
    }
    if chars & 0x0000_0080 != 0 {
        s.push_str(" UDATA");
    }
    if chars & 0x0200_0000 != 0 {
        s.push_str(" DISCARDABLE");
    }
    if chars & 0x0400_0000 != 0 {
        s.push_str(" NOT_CACHED");
    }
    if chars & 0x0800_0000 != 0 {
        s.push_str(" NOT_PAGED");
    }
    if chars & 0x1000_0000 != 0 {
        s.push_str(" SHARED");
    }

    s
}

fn data_dir_name(index: usize) -> &'static str {
    match index {
        0 => "Export Table",
        1 => "Import Table",
        2 => "Resource Table",
        3 => "Exception Table",
        4 => "Certificate Table",
        5 => "Base Relocation Table",
        6 => "Debug",
        7 => "Architecture",
        8 => "Global Ptr",
        9 => "TLS Table",
        10 => "Load Config Table",
        11 => "Bound Import",
        12 => "IAT",
        13 => "Delay Import Descriptor",
        14 => "CLR Runtime Header",
        15 => "Reserved",
        _ => "(unknown)",
    }
}

fn format_timestamp(timestamp: u32) -> String {
    if timestamp == 0 {
        return String::from("(not set)");
    }
    // Simple UTC conversion: seconds since 1970-01-01
    let secs = timestamp as u64;
    // Days since epoch
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Convert days to date using a simple algorithm
    let (year, month, day) = days_to_date(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Minimum hex digit width needed for a value (at least 8)
fn hex_width(val: u64) -> usize {
    if val > 0xFFFF_FFFF {
        16
    } else {
        8
    }
}

// ---------------------------------------------------------------------------
// Rendering: PE Header
// ---------------------------------------------------------------------------

fn format_header(coff: &CoffHeader, opt: &OptionalHeader) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                         PE HEADER\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str(&format!(
        "  Type:         {}\n",
        pe_type_str(coff.characteristics)
    ));
    o.push_str(&format!(
        "  Machine:      {}\n",
        machine_str(coff.machine)
    ));
    o.push_str(&format!(
        "  Format:       {}\n",
        if opt.is_pe32plus() {
            "PE32+ (64-bit)"
        } else {
            "PE32 (32-bit)"
        }
    ));
    o.push_str(&format!(
        "  Subsystem:    {}\n",
        subsystem_str(opt.subsystem)
    ));
    o.push_str(&format!(
        "  DLL Chars:    {}\n",
        dll_characteristics_str(opt.dll_characteristics)
    ));
    o.push_str(&format!(
        "  Entry point:  0x{:08X}\n",
        opt.entry_point
    ));
    o.push_str(&format!(
        "  Image base:   0x{:016X}\n",
        opt.image_base
    ));
    o.push_str(&format!(
        "  Sections:     {}\n",
        coff.num_sections
    ));
    o.push_str(&format!(
        "  Timestamp:    {}\n",
        format_timestamp(coff.timestamp)
    ));
    o.push_str(&format!(
        "  Sect align:   0x{:X}\n",
        opt.section_alignment
    ));
    o.push_str(&format!(
        "  File align:   0x{:X}\n",
        opt.file_alignment
    ));

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Memory Map
// ---------------------------------------------------------------------------

const INNER_W: usize = 34;
const OUTER_W: usize = INNER_W + 6; // 40

fn render_memory_map(
    sections: &[SectionHeader],
    opt: &OptionalHeader,
    file_size: u64,
) -> String {
    let mut o = String::new();

    if sections.is_empty() {
        return o;
    }

    let entry = opt.entry_point;
    let image_base = opt.image_base;

    // End of structured PE data = max (raw_addr + raw_size) across all sections
    let end_of_image = sections
        .iter()
        .map(|s| s.raw_addr as u64 + s.raw_size as u64)
        .max()
        .unwrap_or(0);
    let overlay_size = file_size.saturating_sub(end_of_image);

    // Determine hex width from max address
    let max_addr = sections
        .iter()
        .map(|s| {
            let va = image_base + s.virtual_addr as u64 + s.virtual_size as u64;
            let fo = s.raw_addr as u64 + s.raw_size as u64;
            va.max(fo)
        })
        .max()
        .unwrap_or(0)
        .max(file_size);
    let hw = hex_width(max_addr);

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       MEMORY LAYOUT\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let pad_left = hw + 4; // "  0x" + hex + "  "

    // Header labels
    o.push_str(&format!(
        "{:>width$}  {:^outer$}  {}\n",
        "File Offset",
        "",
        "Virtual Addr",
        width = pad_left,
        outer = OUTER_W,
    ));

    // Headers pseudo-section
    let headers_size = opt.size_of_headers;

    // Top border
    let blank_l = " ".repeat(pad_left + 2);
    o.push_str(&format!(
        "{}╔{}╗\n",
        blank_l,
        "═".repeat(OUTER_W - 2)
    ));

    // Headers block
    {
        let size_h = human_size(headers_size as u64);
        let label = "Headers";
        let remaining = OUTER_W - 4 - label.len() - size_h.len();
        o.push_str(&format!(
            "{}║ {}{}{} ║\n",
            blank_l,
            label,
            " ".repeat(if remaining > 0 { remaining } else { 1 }),
            size_h,
        ));
    }

    // Sections
    for (i, sec) in sections.iter().enumerate() {
        let sec_file_off = sec.raw_addr;
        let sec_vaddr = image_base + sec.virtual_addr as u64;
        let display_size = if sec.raw_size > 0 {
            sec.raw_size
        } else {
            sec.virtual_size
        };

        let sec_end_file = sec.raw_addr as u64 + sec.raw_size as u64;
        let sec_end_vaddr = image_base + sec.virtual_addr as u64 + sec.virtual_size as u64;

        // Separator
        let file_off_str = format!("0x{:0>w$X}", sec_file_off, w = hw);
        let vaddr_str = format!("0x{:0>w$X}", sec_vaddr, w = hw);

        o.push_str(&format!(
            "  {}  ╠{}╣  {}\n",
            file_off_str,
            "═".repeat(OUTER_W - 2),
            vaddr_str,
        ));

        // Section inner box top
        let chars = section_chars_short(sec.characteristics);
        o.push_str(&format!(
            "{}║ ┌{}┐ ║\n",
            blank_l,
            "─".repeat(INNER_W),
        ));

        // Section content line
        {
            let size_str = format_size_commas(display_size as u64);
            let name = &sec.name;
            let avail = INNER_W - 2;
            let name_part = if name.len() + size_str.len() + 1 > avail {
                let max_name = avail.saturating_sub(size_str.len() + 1);
                &name[..max_name.min(name.len())]
            } else {
                name.as_str()
            };
            let gap = avail.saturating_sub(name_part.len() + size_str.len());
            o.push_str(&format!(
                "{}║ │ {}{}{} │ ║\n",
                blank_l,
                name_part,
                " ".repeat(gap),
                size_str,
            ));
        }

        // Characteristics line
        {
            let chars_label = format!("  {}", chars);
            let avail = INNER_W - 2;
            o.push_str(&format!(
                "{}║ │ {:<w$} │ ║\n",
                blank_l,
                chars_label,
                w = avail,
            ));
        }

        // Entry point marker if it falls in this section
        if entry >= sec.virtual_addr
            && entry < sec.virtual_addr + sec.virtual_size
        {
            let marker = format!("► entry @ 0x{:08X}", entry);
            let avail = INNER_W - 3;
            o.push_str(&format!(
                "{}║ │  {:<w$} │ ║\n",
                blank_l,
                marker,
                w = avail,
            ));
        }

        // Section inner box bottom
        if i == sections.len() - 1 {
            let file_end_str = if sec.raw_size > 0 {
                format!("0x{:0>w$X}", sec_end_file, w = hw)
            } else {
                " ".repeat(hw + 2)
            };
            let vaddr_end_str = format!("0x{:0>w$X}", sec_end_vaddr, w = hw);
            o.push_str(&format!(
                "  {}  ║ └{}┘ ║  {}\n",
                file_end_str,
                "─".repeat(INNER_W),
                vaddr_end_str,
            ));
        } else {
            o.push_str(&format!(
                "{}║ └{}┘ ║\n",
                blank_l,
                "─".repeat(INNER_W),
            ));
        }
    }

    // Appended data (overlay) after the last section
    if overlay_size > 0 {
        let file_off_str = format!("0x{:0>w$X}", end_of_image, w = hw);

        // Separator
        o.push_str(&format!(
            "  {}  ╠{}╣\n",
            file_off_str,
            "═".repeat(OUTER_W - 2),
        ));

        // Overlay inner box
        o.push_str(&format!(
            "{}║ ┌{}┐ ║\n",
            blank_l,
            "─".repeat(INNER_W),
        ));

        {
            let size_str = format_size_commas(overlay_size);
            let label = "APPENDED (overlay)";
            let avail = INNER_W - 2;
            let gap = avail.saturating_sub(label.len() + size_str.len());
            o.push_str(&format!(
                "{}║ │ {}{}{} │ ║\n",
                blank_l,
                label,
                " ".repeat(gap),
                size_str,
            ));
        }

        // End offset
        let file_end_str = format!("0x{:0>w$X}", file_size, w = hw);
        o.push_str(&format!(
            "  {}  ║ └{}┘ ║\n",
            file_end_str,
            "─".repeat(INNER_W),
        ));
    }

    // Bottom border
    o.push_str(&format!(
        "{}╚{}╝\n",
        blank_l,
        "═".repeat(OUTER_W - 2)
    ));

    o.push('\n');
    o
}

fn section_chars_short(chars: u32) -> String {
    let mut s = String::new();
    if chars & 0x4000_0000 != 0 {
        s.push('R');
    } else {
        s.push('-');
    }
    if chars & 0x8000_0000 != 0 {
        s.push('W');
    } else {
        s.push('-');
    }
    if chars & 0x2000_0000 != 0 {
        s.push('X');
    } else {
        s.push('-');
    }
    if chars & 0x0000_0020 != 0 {
        s.push_str(" CODE");
    }
    if chars & 0x0000_0040 != 0 {
        s.push_str(" IDATA");
    }
    if chars & 0x0000_0080 != 0 {
        s.push_str(" UDATA");
    }
    s
}

// ---------------------------------------------------------------------------
// Rendering: Section Table
// ---------------------------------------------------------------------------

fn format_section_table(sections: &[SectionHeader], data: &[u8]) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════════════\n");
    o.push_str("                          SECTION HEADERS\n");
    o.push_str("═══════════════════════════════════════════════════════════════════════\n\n");

    o.push_str("  [Nr] Name       VirtSize VirtAddr   RawSize  RawAddr    Entropy  Characteristics\n");
    o.push_str("  ---- ----       -------- --------   -------  -------    -------  ---------------\n");

    for (i, sh) in sections.iter().enumerate() {
        let name = if sh.name.len() > 10 {
            &sh.name[..10]
        } else {
            &sh.name
        };
        let entropy = if sh.raw_size > 0 {
            let start = sh.raw_addr as usize;
            let end = (start + sh.raw_size as usize).min(data.len());
            if start < data.len() {
                format!("{:.2}", shannon_entropy(&data[start..end]))
            } else {
                "  -  ".into()
            }
        } else {
            "  -  ".into()
        };
        o.push_str(&format!(
            "  [{:>2}] {:<10} {:08X} {:08X}   {:08X} {:08X}   {:>5}  {}\n",
            i,
            name,
            sh.virtual_size,
            sh.virtual_addr,
            sh.raw_size,
            sh.raw_addr,
            entropy,
            section_chars_str(sh.characteristics),
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Data Directories
// ---------------------------------------------------------------------------

fn format_data_directories(data_dirs: &[DataDirectory]) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     DATA DIRECTORIES\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str("  Directory                  RVA        Size\n");
    o.push_str("  ---------                  ---        ----\n");

    for (i, dd) in data_dirs.iter().enumerate() {
        o.push_str(&format!(
            "  {:<26} {:08X}   {:08X}\n",
            data_dir_name(i),
            dd.rva,
            dd.size,
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Import Table
// ---------------------------------------------------------------------------

fn format_import_table(imports: &[ImportEntry]) -> String {
    let mut o = String::new();
    if imports.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      IMPORT TABLE\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    for entry in imports {
        let count = entry.functions.len();
        if count > 0 {
            o.push_str(&format!(
                "  {} ({} functions)\n",
                entry.dll_name, count
            ));
        } else {
            o.push_str(&format!("  {}\n", entry.dll_name));
        }
        for func in &entry.functions {
            o.push_str(&format!("    {}\n", func));
        }
        o.push('\n');
    }

    o
}

// ---------------------------------------------------------------------------
// Rendering: Export Table
// ---------------------------------------------------------------------------

fn format_export_table(exports: &[ExportEntry]) -> String {
    let mut o = String::new();
    if exports.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str(&format!(
        "                  EXPORT TABLE ({} exports)\n",
        exports.len()
    ));
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str("  Ordinal  RVA        Name\n");
    o.push_str("  -------  ---        ----\n");

    for exp in exports {
        let name = if exp.name.is_empty() {
            "(unnamed)"
        } else if exp.name.len() > 60 {
            &exp.name[..60]
        } else {
            &exp.name
        };
        o.push_str(&format!(
            "  {:>7}  {:08X}   {}\n",
            exp.ordinal, exp.rva, name,
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Security Summary
// ---------------------------------------------------------------------------

fn format_security_summary(opt: &OptionalHeader, r: &PeReader, sections: &[SectionHeader]) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     SECURITY FEATURES\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let dc = opt.dll_characteristics;
    let aslr = dc & 0x0040 != 0; // DYNAMIC_BASE
    let heva = dc & 0x0020 != 0; // HIGH_ENTROPY_VA
    let dep = dc & 0x0100 != 0;  // NX_COMPAT
    let cfg = dc & 0x4000 != 0;  // GUARD_CF
    let seh = dc & 0x0400 == 0;  // NO_SEH inverted — SEH is enabled when flag is NOT set
    let force_integrity = dc & 0x0080 != 0;
    let appcontainer = dc & 0x1000 != 0;

    // Parse Load Config for CET/Shadow Stack if available
    let (cet_shadow_stack, rf_guard, rf_strict) = parse_load_config_security(opt, r, sections);

    o.push_str(&format!("  ASLR:             {}\n",
        if aslr && heva { "Yes (high entropy)" } else if aslr { "Yes" } else { "No" }));
    o.push_str(&format!("  DEP/NX:           {}\n", if dep { "Yes" } else { "No" }));
    o.push_str(&format!("  CFG:              {}\n", if cfg { "Yes" } else { "No" }));
    o.push_str(&format!("  SEH:              {}\n",
        if !seh { "No (NO_SEH)" } else { "Yes" }));
    o.push_str(&format!("  Force integrity:  {}\n", if force_integrity { "Yes" } else { "No" }));
    if appcontainer {
        o.push_str("  AppContainer:     Yes\n");
    }
    if cet_shadow_stack {
        o.push_str("  CET Shadow Stack: Yes\n");
    }
    if rf_guard {
        o.push_str(&format!("  RF Guard:         {}\n", if rf_strict { "Strict" } else { "Yes" }));
    }

    o.push('\n');
    o
}

fn parse_load_config_security(
    opt: &OptionalHeader,
    r: &PeReader,
    sections: &[SectionHeader],
) -> (bool, bool, bool) {
    // Load Config is data directory index 10
    if opt.data_directories.len() <= 10 {
        return (false, false, false);
    }
    let dd = &opt.data_directories[10];
    if dd.rva == 0 || dd.size == 0 {
        return (false, false, false);
    }
    let off = match rva_to_file_offset(sections, dd.rva) {
        Some(o) => o,
        None => return (false, false, false),
    };

    // GuardFlags is at offset 0x58 in the load config (PE32+)
    // or 0x48 (PE32). We need at least that much.
    let guard_flags_off = if opt.is_pe32plus() { off + 0x58 } else { off + 0x48 };
    if guard_flags_off + 4 > r.len() {
        return (false, false, false);
    }
    let guard_flags = r.u32(guard_flags_off);

    // IMAGE_GUARD_CF_INSTRUMENTED = 0x00000100
    // IMAGE_GUARD_RF_INSTRUMENTED = 0x00020000
    // IMAGE_GUARD_RF_STRICT = 0x00040000
    let rf_guard = guard_flags & 0x00020000 != 0;
    let rf_strict = guard_flags & 0x00040000 != 0;

    // CET shadow stack — check for GuardFlags bit or look at extended field
    // IMAGE_GUARD_SECURITY_COOKIE_UNUSED = 0x00000800
    // For CET, we check if the load config is large enough to contain
    // GuardEHContinuationTable (offset ~0xC8 for PE32+)
    let cet = if opt.is_pe32plus() && dd.size >= 0xD0 {
        // Check if GuardEHContinuationCount is nonzero or CET flag bits
        let cet_off = off + 0xC8;
        if cet_off + 8 <= r.len() {
            r.u64(cet_off) != 0
        } else {
            false
        }
    } else {
        false
    };

    (cet, rf_guard, rf_strict)
}

// ---------------------------------------------------------------------------
// Parsing & Rendering: Rich Header
// ---------------------------------------------------------------------------

struct RichEntry {
    tool_id: u16,
    build_ver: u16,
    use_count: u32,
}

fn parse_rich_header(data: &[u8], pe_offset: usize) -> Option<Vec<RichEntry>> {
    // Rich header lives between the DOS stub and PE signature.
    // It ends with "Rich" followed by a 4-byte XOR key.
    // Search backwards from pe_offset for "Rich"
    if pe_offset < 0x80 || pe_offset > data.len() {
        return None;
    }

    let search_region = &data[0x80..pe_offset];
    let mut rich_pos = None;
    for i in (0..search_region.len().saturating_sub(3)).rev() {
        if &search_region[i..i + 4] == b"Rich" {
            rich_pos = Some(0x80 + i);
            break;
        }
    }
    let rich_pos = rich_pos?;

    if rich_pos + 8 > data.len() {
        return None;
    }

    let key = u32::from_le_bytes([
        data[rich_pos + 4],
        data[rich_pos + 5],
        data[rich_pos + 6],
        data[rich_pos + 7],
    ]);

    // Find "DanS" marker (XOR'd with key) — should be at 0x80
    let dans_pos = 0x80;
    if dans_pos + 16 > data.len() {
        return None;
    }
    let dans_check = u32::from_le_bytes([
        data[dans_pos],
        data[dans_pos + 1],
        data[dans_pos + 2],
        data[dans_pos + 3],
    ]) ^ key;
    if dans_check != 0x536E6144 {
        // "DanS" in LE
        // Try scanning for DanS
        return None;
    }

    // Entries start after "DanS" + 3 padding DWORDs (12 bytes) = offset 0x80 + 16
    let entries_start = dans_pos + 16;
    let entries_end = rich_pos;

    if entries_end <= entries_start || (entries_end - entries_start) % 8 != 0 {
        return None;
    }

    let mut entries = Vec::new();
    let mut pos = entries_start;
    while pos + 8 <= entries_end {
        let comp_id = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) ^ key;
        let count = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) ^ key;

        let build_ver = (comp_id & 0xFFFF) as u16;
        let tool_id = ((comp_id >> 16) & 0xFFFF) as u16;

        if tool_id != 0 || build_ver != 0 || count != 0 {
            entries.push(RichEntry {
                tool_id,
                build_ver,
                use_count: count,
            });
        }
        pos += 8;
    }

    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn rich_tool_name(id: u16) -> &'static str {
    // Known Microsoft tool IDs from Rich header
    match id {
        0x00 => "Unknown",
        0x01 => "Import",
        0x02 => "Linker",
        0x03 => "CVTOMF",
        0x04 => "C Compiler",
        0x05 => "C Compiler",
        0x06 => "C++ Compiler",
        0x07 => "C++ Compiler",
        0x0A => "ASM (MASM)",
        0x0B => "ASM (MASM)",
        0x0F => "Linker",
        0x19 => "CVTRES",
        0x1C => "EXPORT",
        0x3D => "Linker",
        0x3F => "Linker",
        0x40 => "Linker",
        0x5D => "Linker",
        0x5E => "ASM (MASM)",
        0x5F => "C Compiler",
        0x60 => "C++ Compiler",
        0x83 => "Linker",
        0x84 => "ASM (MASM)",
        0x85 => "C Compiler",
        0x86 => "C++ Compiler",
        0x91 => "Linker",
        0x92 => "ASM (MASM)",
        0x93 => "C Compiler",
        0x94 => "C++ Compiler",
        0x95 => "Linker",
        0xAA => "Linker",
        0xAB => "ASM (MASM)",
        0xAC => "C Compiler",
        0xAD => "C++ Compiler",
        0x100 => "CVTRES",
        0x101 => "EXPORT",
        0x102 => "Linker",
        0x103 => "ASM (MASM)",
        0x104 => "C Compiler",
        0x105 => "C++ Compiler",
        0x106 => "Linker (LTCG)",
        0x107 => "C (LTCG)",
        0x108 => "C++ (LTCG)",
        _ => "Unknown",
    }
}

fn format_rich_header(entries: &[RichEntry]) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      RICH HEADER\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str("  Tool ID  Build   Count  Tool\n");
    o.push_str("  -------  -----   -----  ----\n");

    for e in entries {
        o.push_str(&format!(
            "  0x{:04X}   {:>5}   {:>5}  {}\n",
            e.tool_id,
            e.build_ver,
            e.use_count,
            rich_tool_name(e.tool_id),
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Parsing & Rendering: Debug Directory
// ---------------------------------------------------------------------------

struct DebugInfo {
    debug_type: u32,
    pdb_path: Option<String>,
    pdb_guid: Option<String>,
    pdb_age: Option<u32>,
}

fn parse_debug_directory(
    r: &PeReader,
    sections: &[SectionHeader],
    data_dirs: &[DataDirectory],
) -> Vec<DebugInfo> {
    // Debug directory is index 6
    if data_dirs.len() <= 6 || data_dirs[6].rva == 0 || data_dirs[6].size == 0 {
        return Vec::new();
    }

    let dd = &data_dirs[6];
    let off = match rva_to_file_offset(sections, dd.rva) {
        Some(o) => o,
        None => return Vec::new(),
    };

    let count = (dd.size as usize) / 28; // Each debug directory entry is 28 bytes
    let mut entries = Vec::new();

    for i in 0..count.min(16) {
        let base = off + i * 28;
        if base + 28 > r.len() {
            break;
        }

        let debug_type = r.u32(base + 12);
        let pointer_to_raw = r.u32(base + 24) as usize;

        let mut pdb_path = None;
        let mut pdb_guid = None;
        let mut pdb_age = None;

        // Type 2 = IMAGE_DEBUG_TYPE_CODEVIEW
        if debug_type == 2 && pointer_to_raw > 0 && pointer_to_raw + 4 <= r.len() {
            let cv_sig = r.u32(pointer_to_raw);
            if cv_sig == 0x53445352 {
                // "RSDS" — CodeView PDB 7.0
                if pointer_to_raw + 24 < r.len() {
                    // GUID: 16 bytes at offset +4
                    let g = &r.data[pointer_to_raw + 4..pointer_to_raw + 20];
                    let guid = format!(
                        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                        u32::from_le_bytes([g[0], g[1], g[2], g[3]]),
                        u16::from_le_bytes([g[4], g[5]]),
                        u16::from_le_bytes([g[6], g[7]]),
                        g[8], g[9], g[10], g[11], g[12], g[13], g[14], g[15],
                    );
                    pdb_guid = Some(guid);
                    pdb_age = Some(r.u32(pointer_to_raw + 20));
                    pdb_path = Some(r.read_cstr(pointer_to_raw + 24));
                }
            } else if cv_sig == 0x3031424E {
                // "NB10" — CodeView PDB 2.0
                if pointer_to_raw + 16 < r.len() {
                    pdb_path = Some(r.read_cstr(pointer_to_raw + 16));
                }
            }
        }

        entries.push(DebugInfo {
            debug_type,
            pdb_path,
            pdb_guid,
            pdb_age,
        });
    }

    entries
}

fn debug_type_str(t: u32) -> &'static str {
    match t {
        0 => "UNKNOWN",
        1 => "COFF",
        2 => "CODEVIEW",
        3 => "FPO",
        4 => "MISC",
        5 => "EXCEPTION",
        6 => "FIXUP",
        7 => "OMAP_TO_SRC",
        8 => "OMAP_FROM_SRC",
        9 => "BORLAND",
        10 => "RESERVED10",
        11 => "CLSID",
        12 => "VC_FEATURE",
        13 => "POGO",
        14 => "ILTCG",
        15 => "MPX",
        16 => "REPRO",
        20 => "EX_DLLCHARACTERISTICS",
        _ => "OTHER",
    }
}

fn format_debug_directory(entries: &[DebugInfo]) -> String {
    let mut o = String::new();
    if entries.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     DEBUG DIRECTORY\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    for (i, di) in entries.iter().enumerate() {
        if entries.len() > 1 {
            o.push_str(&format!("  Entry {}:\n", i));
        }
        o.push_str(&format!("  Type:      {} ({})\n", debug_type_str(di.debug_type), di.debug_type));
        if let Some(ref path) = di.pdb_path {
            o.push_str(&format!("  PDB path:  {}\n", path));
        }
        if let Some(ref guid) = di.pdb_guid {
            o.push_str(&format!("  PDB GUID:  {}\n", guid));
        }
        if let Some(age) = di.pdb_age {
            o.push_str(&format!("  PDB age:   {}\n", age));
        }
        if entries.len() > 1 {
            o.push('\n');
        }
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Parsing & Rendering: TLS Callbacks
// ---------------------------------------------------------------------------

fn parse_tls_callbacks(
    r: &PeReader,
    sections: &[SectionHeader],
    opt: &OptionalHeader,
) -> Vec<u64> {
    // TLS directory is index 9
    if opt.data_directories.len() <= 9 {
        return Vec::new();
    }
    let dd = &opt.data_directories[9];
    if dd.rva == 0 || dd.size == 0 {
        return Vec::new();
    }

    let off = match rva_to_file_offset(sections, dd.rva) {
        Some(o) => o,
        None => return Vec::new(),
    };

    // TLS directory structure:
    // PE32+: StartAddressOfRawData(8), EndAddressOfRawData(8),
    //        AddressOfIndex(8), AddressOfCallBacks(8), ...
    // PE32:  StartAddressOfRawData(4), EndAddressOfRawData(4),
    //        AddressOfIndex(4), AddressOfCallBacks(4), ...

    let callbacks_va = if opt.is_pe32plus() {
        if off + 32 > r.len() {
            return Vec::new();
        }
        r.u64(off + 24)
    } else {
        if off + 16 > r.len() {
            return Vec::new();
        }
        r.u32(off + 12) as u64
    };

    if callbacks_va == 0 {
        return Vec::new();
    }

    // Convert VA to RVA, then to file offset
    let callbacks_rva = callbacks_va.wrapping_sub(opt.image_base) as u32;
    let callbacks_off = match rva_to_file_offset(sections, callbacks_rva) {
        Some(o) => o,
        None => return Vec::new(),
    };

    let mut callbacks = Vec::new();
    let entry_size = if opt.is_pe32plus() { 8 } else { 4 };

    for i in 0..64 {
        let pos = callbacks_off + i * entry_size;
        if pos + entry_size > r.len() {
            break;
        }
        let addr = if opt.is_pe32plus() {
            r.u64(pos)
        } else {
            r.u32(pos) as u64
        };
        if addr == 0 {
            break;
        }
        callbacks.push(addr);
    }

    callbacks
}

fn format_tls_callbacks(callbacks: &[u64], image_base: u64) -> String {
    let mut o = String::new();
    if callbacks.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      TLS CALLBACKS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str("  ⚠ TLS callbacks execute BEFORE the entry point!\n\n");

    for (i, &addr) in callbacks.iter().enumerate() {
        let rva = addr.wrapping_sub(image_base);
        o.push_str(&format!("  [{}]  VA 0x{:016X}  (RVA 0x{:08X})\n", i, addr, rva));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Parsing & Rendering: Delay Imports
// ---------------------------------------------------------------------------

fn parse_delay_imports(
    r: &PeReader,
    sections: &[SectionHeader],
    data_dirs: &[DataDirectory],
    is_pe32plus: bool,
) -> Vec<ImportEntry> {
    // Delay import directory is index 13
    if data_dirs.len() <= 13 || data_dirs[13].rva == 0 || data_dirs[13].size == 0 {
        return Vec::new();
    }

    let delay_rva = data_dirs[13].rva;
    let delay_off = match rva_to_file_offset(sections, delay_rva) {
        Some(o) => o,
        None => return Vec::new(),
    };

    let entry_size: usize = if is_pe32plus { 8 } else { 4 };
    let ordinal_flag: u64 = if is_pe32plus {
        0x8000_0000_0000_0000
    } else {
        0x8000_0000
    };

    let mut imports = Vec::new();
    let mut idx = 0;

    loop {
        let base = delay_off + idx * 32; // Delay import descriptor is 32 bytes
        if base + 32 > r.len() {
            break;
        }

        // Attributes (should be 1 for new-style RVA-based)
        let _attrs = r.u32(base);
        let name_rva = r.u32(base + 4);
        let _module_handle_rva = r.u32(base + 8);
        let ilt_rva = r.u32(base + 16);

        if name_rva == 0 && ilt_rva == 0 {
            break;
        }

        let dll_name = match rva_to_file_offset(sections, name_rva) {
            Some(off) => r.read_cstr(off),
            None => String::from("(unknown)"),
        };

        let mut functions = Vec::new();

        if let Some(ilt_off) = rva_to_file_offset(sections, ilt_rva) {
            let mut fi = 0;
            loop {
                let e_off = ilt_off + fi * entry_size;
                if e_off + entry_size > r.len() {
                    break;
                }

                let entry_val = if is_pe32plus {
                    r.u64(e_off)
                } else {
                    r.u32(e_off) as u64
                };

                if entry_val == 0 {
                    break;
                }

                if entry_val & ordinal_flag != 0 {
                    let ord = entry_val & 0xFFFF;
                    functions.push(format!("Ordinal {}", ord));
                } else {
                    let hint_rva = (entry_val & 0x7FFF_FFFF) as u32;
                    if let Some(hint_off) = rva_to_file_offset(sections, hint_rva) {
                        let fname = r.read_cstr(hint_off + 2);
                        if !fname.is_empty() {
                            functions.push(fname);
                        } else {
                            functions.push(format!("(hint RVA 0x{:08X})", hint_rva));
                        }
                    } else {
                        functions.push(format!("(RVA 0x{:08X})", hint_rva));
                    }
                }

                fi += 1;
                if fi > 4096 {
                    break;
                }
            }
        }

        imports.push(ImportEntry {
            dll_name,
            functions,
        });
        idx += 1;

        if idx > 512 {
            break;
        }
    }

    imports
}

fn format_delay_import_table(imports: &[ImportEntry]) -> String {
    let mut o = String::new();
    if imports.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                   DELAY IMPORT TABLE\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    for entry in imports {
        let count = entry.functions.len();
        if count > 0 {
            o.push_str(&format!(
                "  {} ({} functions)\n",
                entry.dll_name, count
            ));
        } else {
            o.push_str(&format!("  {}\n", entry.dll_name));
        }
        for func in &entry.functions {
            o.push_str(&format!("    {}\n", func));
        }
        o.push('\n');
    }

    o
}

// ---------------------------------------------------------------------------
// Parsing & Rendering: Resources (Version Info)
// ---------------------------------------------------------------------------

struct VersionInfo {
    file_version: String,
    product_version: String,
    entries: Vec<(String, String)>, // (key, value) pairs like CompanyName, etc.
}

fn parse_version_info(
    r: &PeReader,
    sections: &[SectionHeader],
    data_dirs: &[DataDirectory],
) -> Option<VersionInfo> {
    // Resource table is data directory index 2
    if data_dirs.len() <= 2 || data_dirs[2].rva == 0 || data_dirs[2].size == 0 {
        return None;
    }

    let rsrc_rva = data_dirs[2].rva;
    let rsrc_off = rva_to_file_offset(sections, rsrc_rva)?;

    // Navigate resource tree: Type (RT_VERSION=16) -> ID -> Language
    let version_type_id = 16u32;

    // Level 1: resource type directory
    let num_named = r.u16(rsrc_off + 12) as usize;
    let num_id = r.u16(rsrc_off + 14) as usize;
    let total = num_named + num_id;

    let mut version_subdir_off = None;
    for i in 0..total.min(64) {
        let entry_off = rsrc_off + 16 + i * 8;
        if entry_off + 8 > r.len() {
            break;
        }
        let name_or_id = r.u32(entry_off);
        let offset_to_data = r.u32(entry_off + 4);

        if name_or_id == version_type_id && offset_to_data & 0x80000000 != 0 {
            version_subdir_off = Some(rsrc_off + (offset_to_data & 0x7FFFFFFF) as usize);
            break;
        }
    }

    let subdir_off = version_subdir_off?;

    // Level 2: ID directory
    let num_named2 = r.u16(subdir_off + 12) as usize;
    let num_id2 = r.u16(subdir_off + 14) as usize;
    let total2 = num_named2 + num_id2;

    let mut lang_subdir_off = None;
    for i in 0..total2.min(16) {
        let entry_off = subdir_off + 16 + i * 8;
        if entry_off + 8 > r.len() {
            break;
        }
        let offset_to_data = r.u32(entry_off + 4);
        if offset_to_data & 0x80000000 != 0 {
            lang_subdir_off = Some(rsrc_off + (offset_to_data & 0x7FFFFFFF) as usize);
            break;
        }
    }

    let lang_off = lang_subdir_off?;

    // Level 3: language directory
    let num_named3 = r.u16(lang_off + 12) as usize;
    let num_id3 = r.u16(lang_off + 14) as usize;
    let total3 = num_named3 + num_id3;

    if total3 == 0 {
        return None;
    }

    let entry_off = lang_off + 16;
    if entry_off + 8 > r.len() {
        return None;
    }
    let offset_to_data = r.u32(entry_off + 4);
    if offset_to_data & 0x80000000 != 0 {
        return None; // Should be a data entry, not a directory
    }

    // Data entry: RVA(4), Size(4), CodePage(4), Reserved(4)
    let data_entry_off = rsrc_off + offset_to_data as usize;
    if data_entry_off + 16 > r.len() {
        return None;
    }
    let data_rva = r.u32(data_entry_off);
    let data_size = r.u32(data_entry_off + 4) as usize;

    let data_off = rva_to_file_offset(sections, data_rva)?;
    if data_off + data_size > r.len() {
        return None;
    }

    let vdata = &r.data[data_off..data_off + data_size];
    parse_vs_version_info(vdata)
}

fn parse_vs_version_info(data: &[u8]) -> Option<VersionInfo> {
    if data.len() < 92 {
        return None;
    }

    // VS_VERSION_INFO header: wLength(2), wValueLength(2), wType(2), szKey (unicode "VS_VERSION_INFO\0")
    // Check for VS_FIXEDFILEINFO signature at aligned position after the key
    // Key "VS_VERSION_INFO" is 16 chars * 2 = 32 bytes + 2 null = 34 bytes, starting at offset 6
    // Total header = 6 + 34 = 40, then align to 4 = 40

    let value_length = u16::from_le_bytes([data[2], data[3]]) as usize;

    // Find VS_FIXEDFILEINFO — signature 0xFEEF04BD
    let mut fixed_off = None;
    for i in (36..data.len().saturating_sub(52)).step_by(4) {
        if i + 4 <= data.len() {
            let sig = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            if sig == 0xFEEF04BD {
                fixed_off = Some(i);
                break;
            }
        }
    }

    let mut file_version = String::new();
    let mut product_version = String::new();

    if let Some(fo) = fixed_off {
        if fo + 52 <= data.len() && value_length >= 52 {
            let file_ver_ms = u32::from_le_bytes([data[fo + 8], data[fo + 9], data[fo + 10], data[fo + 11]]);
            let file_ver_ls = u32::from_le_bytes([data[fo + 12], data[fo + 13], data[fo + 14], data[fo + 15]]);
            file_version = format!("{}.{}.{}.{}",
                file_ver_ms >> 16, file_ver_ms & 0xFFFF,
                file_ver_ls >> 16, file_ver_ls & 0xFFFF);

            let prod_ver_ms = u32::from_le_bytes([data[fo + 16], data[fo + 17], data[fo + 18], data[fo + 19]]);
            let prod_ver_ls = u32::from_le_bytes([data[fo + 20], data[fo + 21], data[fo + 22], data[fo + 23]]);
            product_version = format!("{}.{}.{}.{}",
                prod_ver_ms >> 16, prod_ver_ms & 0xFFFF,
                prod_ver_ls >> 16, prod_ver_ls & 0xFFFF);
        }
    }

    // Parse StringFileInfo — scan for known key strings in UTF-16LE
    let entries = extract_version_strings(data);

    Some(VersionInfo {
        file_version,
        product_version,
        entries,
    })
}

fn extract_version_strings(data: &[u8]) -> Vec<(String, String)> {
    let known_keys = [
        "CompanyName", "FileDescription", "FileVersion", "InternalName",
        "LegalCopyright", "OriginalFilename", "ProductName", "ProductVersion",
        "Comments", "LegalTrademarks",
    ];

    let mut result = Vec::new();

    for key in &known_keys {
        if let Some(val) = find_version_string(data, key) {
            if !val.is_empty() {
                result.push((key.to_string(), val));
            }
        }
    }

    result
}

fn find_version_string(data: &[u8], key: &str) -> Option<String> {
    // Encode key as UTF-16LE and search for it
    let key_utf16: Vec<u8> = key
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();

    // Search for key in data
    let key_len = key_utf16.len();
    for i in 0..data.len().saturating_sub(key_len + 2) {
        if data[i..i + key_len] == key_utf16[..] {
            // Check null terminator after key
            if i + key_len + 2 <= data.len()
                && data[i + key_len] == 0
                && data[i + key_len + 1] == 0
            {
                // Value follows after alignment to 4-byte boundary
                let after_key = i + key_len + 2;
                let aligned = (after_key + 3) & !3;
                if aligned < data.len() {
                    return Some(read_utf16le_str(&data[aligned..]));
                }
            }
        }
    }
    None
}

fn read_utf16le_str(data: &[u8]) -> String {
    let mut chars = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let c = u16::from_le_bytes([data[i], data[i + 1]]);
        if c == 0 {
            break;
        }
        chars.push(c);
        i += 2;
        if chars.len() > 512 {
            break;
        }
    }
    String::from_utf16_lossy(&chars)
}

fn format_version_info(vi: &VersionInfo) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     VERSION INFO\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    if !vi.file_version.is_empty() {
        o.push_str(&format!("  File version:     {}\n", vi.file_version));
    }
    if !vi.product_version.is_empty() {
        o.push_str(&format!("  Product version:  {}\n", vi.product_version));
    }

    for (key, val) in &vi.entries {
        o.push_str(&format!("  {:<18} {}\n", format!("{}:", key), val));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a PE file and return formatted output
pub fn parse_pe(path: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|e| e.to_string())?;
    parse_pe_bytes(&data)
}

/// Parse PE data from bytes
pub fn parse_pe_bytes(data: &[u8]) -> Result<String, String> {
    let r = PeReader::new(data);

    // Check MZ magic
    if data.len() < 2 || !is_pe_magic(data) {
        return Err("Not a PE file (missing MZ magic)".into());
    }

    // Get PE header offset from DOS header at 0x3C
    if data.len() < 0x40 {
        return Err("File too small for DOS header".into());
    }
    let pe_offset = r.u32(0x3C) as usize;

    // Check for PE signature
    if pe_offset + 4 > data.len() {
        return Err("Invalid PE header offset".into());
    }
    if data[pe_offset..pe_offset + 4] != PE_SIG {
        // DOS executable without PE header
        return Ok(format!(
            "═══════════════════════════════════════════════════════════════\n\
             \x20                        DOS EXECUTABLE\n\
             ═══════════════════════════════════════════════════════════════\n\n\
             \x20 This is a DOS executable (MZ header present) but has no PE\n\
             \x20 signature. It is a 16-bit DOS program.\n\n\
             \x20 File size: {}\n",
            human_size(data.len() as u64)
        ));
    }

    // COFF header starts right after PE signature
    let coff_offset = pe_offset + 4;
    let coff = parse_coff_header(&r, coff_offset)?;

    // Optional header
    let opt_size = r.u16(coff_offset + 16);
    let opt_offset = coff_offset + 20;
    let opt = parse_optional_header(&r, opt_offset, opt_size)?;

    // Section headers follow optional header
    let sections_offset = opt_offset + opt_size as usize;
    let sections = parse_section_headers(&r, sections_offset, coff.num_sections);

    // Parse imports and exports
    let imports = parse_imports_full(&r, &sections, &opt.data_directories, opt.is_pe32plus());
    let exports = parse_exports(&r, &sections, &opt.data_directories);
    let delay_imports = parse_delay_imports(&r, &sections, &opt.data_directories, opt.is_pe32plus());
    let debug_entries = parse_debug_directory(&r, &sections, &opt.data_directories);
    let tls_callbacks = parse_tls_callbacks(&r, &sections, &opt);
    let rich_entries = parse_rich_header(data, pe_offset);
    let version_info = parse_version_info(&r, &sections, &opt.data_directories);

    let mut output = String::new();

    // 1. PE Header
    output.push_str(&format_header(&coff, &opt));

    // 2. Security Features (checksec-style)
    output.push_str(&format_security_summary(&opt, &r, &sections));

    // 3. Digital Signature (Authenticode)
    match parse_signature(data) {
        Some(sig) => output.push_str(&format_signature(&sig)),
        None => {
            output.push_str("═══════════════════════════════════════════════════════════════\n");
            output.push_str("                    DIGITAL SIGNATURE\n");
            output.push_str("═══════════════════════════════════════════════════════════════\n\n");
            output.push_str("  Not signed\n\n");
        }
    }

    // 4. Version Info (from resources)
    if let Some(ref vi) = version_info {
        output.push_str(&format_version_info(vi));
    }

    // 5. Rich Header
    if let Some(ref entries) = rich_entries {
        output.push_str(&format_rich_header(entries));
    }

    // 6. Memory Layout
    let file_size = data.len() as u64;
    output.push_str(&render_memory_map(&sections, &opt, file_size));

    // 7. Section Headers Table (with entropy)
    if !sections.is_empty() {
        output.push_str(&format_section_table(&sections, data));
    }

    // 8. Data Directories
    if !opt.data_directories.is_empty() {
        output.push_str(&format_data_directories(&opt.data_directories));
    }

    // 9. Debug Directory
    if !debug_entries.is_empty() {
        output.push_str(&format_debug_directory(&debug_entries));
    }

    // 10. TLS Callbacks
    if !tls_callbacks.is_empty() {
        output.push_str(&format_tls_callbacks(&tls_callbacks, opt.image_base));
    }

    // 11. Export Table
    output.push_str(&format_export_table(&exports));

    // 12. Import Table
    output.push_str(&format_import_table(&imports));

    // 13. Delay Import Table
    output.push_str(&format_delay_import_table(&delay_imports));

    Ok(output)
}
