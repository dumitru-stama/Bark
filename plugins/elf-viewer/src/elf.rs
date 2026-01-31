//! ELF file parsing and rich ASCII rendering module

use std::fs::File;
use std::io::Read;

/// ELF magic bytes
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// Check if the given bytes are ELF magic
pub fn is_elf(magic: &[u8]) -> bool {
    magic.len() >= 4 && magic[..4] == ELF_MAGIC
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

struct ElfReader<'a> {
    data: &'a [u8],
    is_le: bool,
    is_64bit: bool,
}

impl<'a> ElfReader<'a> {
    fn new(data: &'a [u8]) -> Result<Self, String> {
        if data.len() < 16 {
            return Err("File too small for ELF header".into());
        }
        if !is_elf(data) {
            return Err("Not an ELF file".into());
        }
        let is_64bit = data[4] == 2;
        let is_le = data[5] == 1;
        let min_size = if is_64bit { 64 } else { 52 };
        if data.len() < min_size {
            return Err("File too small for ELF header".into());
        }
        Ok(Self {
            data,
            is_le,
            is_64bit,
        })
    }

    fn u16(&self, off: usize) -> u16 {
        if off + 2 > self.data.len() {
            return 0;
        }
        let b = &self.data[off..off + 2];
        if self.is_le {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        }
    }

    fn u32(&self, off: usize) -> u32 {
        if off + 4 > self.data.len() {
            return 0;
        }
        let b = &self.data[off..off + 4];
        if self.is_le {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        }
    }

    fn u64(&self, off: usize) -> u64 {
        if off + 8 > self.data.len() {
            return 0;
        }
        let b = &self.data[off..off + 8];
        if self.is_le {
            u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        } else {
            u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        }
    }

    #[allow(dead_code)]
    /// Read a pointer-sized value (32 or 64 bit) as u64
    fn ptr(&self, off: usize) -> u64 {
        if self.is_64bit {
            self.u64(off)
        } else {
            self.u32(off) as u64
        }
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
}

#[derive(Clone)]
struct ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
    p_memsz: u64,
}

#[derive(Clone)]
struct SectionHeader {
    name: String,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_entsize: u64,
}

#[derive(Clone)]
#[allow(dead_code)]
struct Symbol {
    name: String,
    value: u64,
    size: u64,
    sym_type: u8,
    bind: u8,
    shndx: u16,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_program_headers(r: &ElfReader) -> Vec<ProgramHeader> {
    let (phoff, phentsize, phnum) = if r.is_64bit {
        (r.u64(32), r.u16(54) as usize, r.u16(56) as usize)
    } else {
        (r.u32(28) as u64, r.u16(42) as usize, r.u16(44) as usize)
    };
    if phoff == 0 || phnum == 0 || phentsize == 0 {
        return Vec::new();
    }
    let mut phs = Vec::new();
    for i in 0..phnum.min(64) {
        let base = phoff as usize + i * phentsize;
        if r.is_64bit {
            if base + 56 > r.data.len() {
                break;
            }
            phs.push(ProgramHeader {
                p_type: r.u32(base),
                p_flags: r.u32(base + 4),
                p_offset: r.u64(base + 8),
                p_vaddr: r.u64(base + 16),
                p_filesz: r.u64(base + 32),
                p_memsz: r.u64(base + 40),
            });
        } else {
            if base + 32 > r.data.len() {
                break;
            }
            phs.push(ProgramHeader {
                p_type: r.u32(base),
                p_offset: r.u32(base + 4) as u64,
                p_vaddr: r.u32(base + 8) as u64,
                p_filesz: r.u32(base + 16) as u64,
                p_memsz: r.u32(base + 20) as u64,
                p_flags: r.u32(base + 24),
            });
        }
    }
    phs
}

fn parse_section_headers(r: &ElfReader) -> Vec<SectionHeader> {
    let (shoff, shentsize, shnum, shstrndx) = if r.is_64bit {
        (
            r.u64(40),
            r.u16(58) as usize,
            r.u16(60) as usize,
            r.u16(62) as usize,
        )
    } else {
        (
            r.u32(32) as u64,
            r.u16(46) as usize,
            r.u16(48) as usize,
            r.u16(50) as usize,
        )
    };
    if shoff == 0 || shnum == 0 || shentsize == 0 {
        return Vec::new();
    }

    // First pass: read raw section headers to find shstrtab
    let entry_size = if r.is_64bit { 64 } else { 40 };
    let _ = entry_size; // we use shentsize from the file

    struct RawSh {
        name_idx: u32,
        sh_type: u32,
        sh_flags: u64,
        sh_addr: u64,
        sh_offset: u64,
        sh_size: u64,
        sh_link: u32,
        sh_entsize: u64,
    }

    let mut raw = Vec::new();
    for i in 0..shnum.min(256) {
        let base = shoff as usize + i * shentsize;
        if r.is_64bit {
            if base + 64 > r.data.len() {
                break;
            }
            raw.push(RawSh {
                name_idx: r.u32(base),
                sh_type: r.u32(base + 4),
                sh_flags: r.u64(base + 8),
                sh_addr: r.u64(base + 16),
                sh_offset: r.u64(base + 24),
                sh_size: r.u64(base + 32),
                sh_link: r.u32(base + 40),
                sh_entsize: r.u64(base + 56),
            });
        } else {
            if base + 40 > r.data.len() {
                break;
            }
            raw.push(RawSh {
                name_idx: r.u32(base),
                sh_type: r.u32(base + 4),
                sh_flags: r.u32(base + 8) as u64,
                sh_addr: r.u32(base + 12) as u64,
                sh_offset: r.u32(base + 16) as u64,
                sh_size: r.u32(base + 20) as u64,
                sh_link: r.u32(base + 24),
                sh_entsize: r.u32(base + 36) as u64,
            });
        }
    }

    // Get shstrtab offset
    let strtab_off = if shstrndx < raw.len() {
        raw[shstrndx].sh_offset as usize
    } else {
        0
    };

    raw.iter()
        .map(|s| {
            let name = if strtab_off > 0 {
                r.read_cstr(strtab_off + s.name_idx as usize)
            } else {
                String::new()
            };
            SectionHeader {
                name,
                sh_type: s.sh_type,
                sh_flags: s.sh_flags,
                sh_addr: s.sh_addr,
                sh_offset: s.sh_offset,
                sh_size: s.sh_size,
                sh_link: s.sh_link,
                sh_entsize: s.sh_entsize,
            }
        })
        .collect()
}

fn parse_symbols(r: &ElfReader, sections: &[SectionHeader]) -> Vec<Symbol> {
    let mut syms = Vec::new();

    for sh in sections {
        // SHT_SYMTAB = 2, SHT_DYNSYM = 11
        if sh.sh_type != 2 && sh.sh_type != 11 {
            continue;
        }
        if sh.sh_entsize == 0 {
            continue;
        }

        // Linked string table
        let strtab_off = if (sh.sh_link as usize) < sections.len() {
            sections[sh.sh_link as usize].sh_offset as usize
        } else {
            0
        };

        let count = (sh.sh_size / sh.sh_entsize) as usize;
        for i in 0..count.min(2000) {
            let base = sh.sh_offset as usize + i * sh.sh_entsize as usize;
            let (name_idx, value, size, info, shndx) = if r.is_64bit {
                if base + 24 > r.data.len() {
                    break;
                }
                (
                    r.u32(base),
                    r.u64(base + 8),
                    r.u64(base + 16),
                    r.data.get(base + 4).copied().unwrap_or(0),
                    r.u16(base + 6),
                )
            } else {
                if base + 16 > r.data.len() {
                    break;
                }
                (
                    r.u32(base),
                    r.u32(base + 4) as u64,
                    r.u32(base + 8) as u64,
                    r.data.get(base + 12).copied().unwrap_or(0),
                    r.u16(base + 14),
                )
            };

            let sym_type = info & 0xf;
            let bind = info >> 4;

            // Skip FILE (4) and SECTION (3) symbols
            if sym_type == 3 || sym_type == 4 {
                continue;
            }

            let name = if strtab_off > 0 && name_idx > 0 {
                r.read_cstr(strtab_off + name_idx as usize)
            } else {
                String::new()
            };

            syms.push(Symbol {
                name,
                value,
                size,
                sym_type,
                bind,
                shndx,
            });
        }
    }

    // Sort by address
    syms.sort_by_key(|s| s.value);
    syms
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

fn phdr_flags_str(flags: u32) -> String {
    format!(
        "{}{}{}",
        if flags & 4 != 0 { "R" } else { "-" },
        if flags & 2 != 0 { "W" } else { "-" },
        if flags & 1 != 0 { "X" } else { "-" },
    )
}

fn section_flags_str(flags: u64) -> String {
    let mut s = String::new();
    if flags & 0x2 != 0 {
        s.push('A');
    }
    if flags & 0x1 != 0 {
        s.push('W');
    }
    if flags & 0x4 != 0 {
        s.push('X');
    }
    if flags & 0x10 != 0 {
        s.push('M');
    }
    if flags & 0x20 != 0 {
        s.push('S');
    }
    if flags & 0x40 != 0 {
        s.push('I');
    }
    if flags & 0x80 != 0 {
        s.push('L');
    }
    if flags & 0x200 != 0 {
        s.push('G');
    }
    if flags & 0x400 != 0 {
        s.push('T');
    }
    s
}

fn section_type_str(t: u32) -> &'static str {
    match t {
        0 => "NULL",
        1 => "PROGBITS",
        2 => "SYMTAB",
        3 => "STRTAB",
        4 => "RELA",
        5 => "HASH",
        6 => "DYNAMIC",
        7 => "NOTE",
        8 => "NOBITS",
        9 => "REL",
        11 => "DYNSYM",
        14 => "INIT_ARRAY",
        15 => "FINI_ARRAY",
        16 => "PREINIT_ARRAY",
        17 => "GROUP",
        0x6ffffff6 => "GNU_HASH",
        0x6ffffffd => "VERDEF",
        0x6ffffffe => "VERNEED",
        0x6fffffff => "VERSYM",
        _ => "OTHER",
    }
}

fn symbol_type_str(t: u8) -> &'static str {
    match t {
        0 => "NOTYPE",
        1 => "OBJECT",
        2 => "FUNC",
        3 => "SECTION",
        4 => "FILE",
        5 => "COMMON",
        6 => "TLS",
        10 => "GNU_IFUNC",
        _ => "OTHER",
    }
}

fn symbol_bind_str(b: u8) -> &'static str {
    match b {
        0 => "LOCAL",
        1 => "GLOBAL",
        2 => "WEAK",
        10 => "GNU_UNIQUE",
        _ => "OTHER",
    }
}

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

/// Compute the end of structured ELF data (max file offset across segments and sections)
fn compute_elf_end_of_image(phs: &[ProgramHeader], sections: &[SectionHeader]) -> u64 {
    let seg_end = phs
        .iter()
        .map(|p| p.p_offset.saturating_add(p.p_filesz))
        .max()
        .unwrap_or(0);
    let sec_end = sections
        .iter()
        .filter(|s| s.sh_type != 8) // skip SHT_NOBITS (.bss)
        .map(|s| s.sh_offset.saturating_add(s.sh_size))
        .max()
        .unwrap_or(0);
    seg_end.max(sec_end)
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
// Rendering: ELF Header
// ---------------------------------------------------------------------------

fn format_header(r: &ElfReader) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                         ELF HEADER\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str(&format!(
        "  Class:        {}\n",
        if r.is_64bit { "ELF64" } else { "ELF32" }
    ));
    o.push_str(&format!(
        "  Data:         {}\n",
        if r.is_le {
            "Little endian"
        } else {
            "Big endian"
        }
    ));
    o.push_str(&format!(
        "  OS/ABI:       {}\n",
        match r.data[7] {
            0 => "UNIX - System V",
            1 => "HP-UX",
            2 => "NetBSD",
            3 => "Linux",
            6 => "Solaris",
            9 => "FreeBSD",
            12 => "OpenBSD",
            _ => "Unknown",
        }
    ));

    let e_type = r.u16(16);
    o.push_str(&format!(
        "  Type:         {}\n",
        match e_type {
            0 => "NONE",
            1 => "REL (Relocatable)",
            2 => "EXEC (Executable)",
            3 => "DYN (Shared object / PIE)",
            4 => "CORE",
            _ => "Unknown",
        }
    ));

    let e_machine = r.u16(18);
    o.push_str(&format!(
        "  Machine:      {}\n",
        match e_machine {
            0 => "None",
            3 => "Intel 80386 (x86)",
            8 => "MIPS",
            20 => "PowerPC",
            40 => "ARM",
            62 => "AMD x86-64",
            183 => "AArch64 (ARM64)",
            243 => "RISC-V",
            _ => "Unknown",
        }
    ));

    let entry = if r.is_64bit {
        r.u64(24)
    } else {
        r.u32(24) as u64
    };
    o.push_str(&format!("  Entry point:  0x{:x}\n", entry));

    let flags = if r.is_64bit {
        r.u32(48)
    } else {
        r.u32(36)
    };
    if flags != 0 {
        o.push_str(&format!("  Flags:        0x{:x}\n", flags));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Memory Map
// ---------------------------------------------------------------------------

const INNER_W: usize = 34;
const OUTER_W: usize = INNER_W + 6; // 40

fn render_memory_map(
    phs: &[ProgramHeader],
    sections: &[SectionHeader],
    entry: u64,
    file_size: u64,
) -> String {
    let mut o = String::new();

    // Only LOAD segments (type == 1), sorted by vaddr
    let mut loads: Vec<&ProgramHeader> = phs.iter().filter(|p| p.p_type == 1).collect();
    if loads.is_empty() {
        return o;
    }
    loads.sort_by_key(|p| p.p_vaddr);

    // End of structured data = max file offset across all segments and sections
    let end_of_image = compute_elf_end_of_image(phs, sections);
    let overlay_size = file_size.saturating_sub(end_of_image);

    // Determine hex width from max address across all segments and sections
    let max_addr = loads
        .iter()
        .map(|p| p.p_vaddr.saturating_add(p.p_memsz).max(p.p_offset.saturating_add(p.p_filesz)))
        .max()
        .unwrap_or(0)
        .max(file_size);
    let hw = hex_width(max_addr);

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       MEMORY LAYOUT\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let pad_left = hw + 4; // "  0x" + hex + "  "
    let _pad_right = hw + 4;

    // Header labels
    o.push_str(&format!(
        "{:>width$}  {:^outer$}  {}\n",
        "File Offset",
        "",
        "Virtual Addr",
        width = pad_left,
        outer = OUTER_W,
    ));

    for (seg_idx, seg) in loads.iter().enumerate() {
        let seg_start = seg.p_vaddr;
        let seg_end = seg_start.saturating_add(seg.p_memsz);

        // Find sections in this segment
        let mut seg_sections: Vec<&SectionHeader> = sections
            .iter()
            .filter(|s| {
                if s.name.is_empty() || s.sh_type == 0 {
                    return false;
                }
                // Sections with addr=0 and flags without ALLOC (0x2) are metadata
                if s.sh_addr == 0 && (s.sh_flags & 0x2) == 0 {
                    return false;
                }
                s.sh_addr >= seg_start && s.sh_addr < seg_end
            })
            .collect();
        seg_sections.sort_by_key(|s| s.sh_addr);
        let seg_sections = if seg_sections.len() > 30 {
            &seg_sections[..30]
        } else {
            &seg_sections[..]
        };

        let flags = phdr_flags_str(seg.p_flags);
        let size_h = human_size(seg.p_memsz);

        // Top of segment box (or separator between segments)
        if seg_idx == 0 {
            // Top border
            let blank_l = " ".repeat(pad_left + 2);
            o.push_str(&format!(
                "{}╔{}╗\n",
                blank_l,
                "═".repeat(OUTER_W - 2)
            ));
        } else {
            // Separator between segments
            let blank_l = " ".repeat(pad_left + 2);
            o.push_str(&format!(
                "{}╠{}╣\n",
                blank_l,
                "═".repeat(OUTER_W - 2)
            ));
        }

        // Segment label line: ║  LOAD  R-X                    2.5 MB ║
        {
            let label = format!("LOAD  {}", flags);
            let remaining = OUTER_W - 4 - label.len() - size_h.len();
            let blank_l = " ".repeat(pad_left + 2);
            o.push_str(&format!(
                "{}║ {}{}{} ║\n",
                blank_l,
                label,
                " ".repeat(if remaining > 0 { remaining } else { 1 }),
                size_h,
            ));
        }

        if seg_sections.is_empty() {
            // Empty segment — show address range
            let file_off = format!("0x{:0>w$X}", seg.p_offset, w = hw);
            let vaddr = format!("0x{:0>w$X}", seg_start, w = hw);
            o.push_str(&format!(
                "  {}  ║ {:^inner_w$} ║  {}\n",
                file_off,
                "(no sections)",
                vaddr,
                inner_w = OUTER_W - 4,
            ));
        } else {
            for (i, sec) in seg_sections.iter().enumerate() {
                let is_nobits = sec.sh_type == 8; // SHT_NOBITS (.bss)
                let sec_file_off = if is_nobits { None } else { Some(sec.sh_offset) };
                let sec_end_vaddr = sec.sh_addr.saturating_add(sec.sh_size);
                let sec_end_file = sec.sh_offset.saturating_add(sec.sh_size);

                // Section top border
                let file_off_str = match sec_file_off {
                    Some(fo) => format!("0x{:0>w$X}", fo, w = hw),
                    None => " ".repeat(hw + 2),
                };
                let vaddr_str = format!("0x{:0>w$X}", sec.sh_addr, w = hw);

                if i == 0 {
                    o.push_str(&format!(
                        "  {}  ║ ┌{}┐ ║  {}\n",
                        file_off_str,
                        "─".repeat(INNER_W),
                        vaddr_str,
                    ));
                } else {
                    o.push_str(&format!(
                        "  {}  ║ ├{}┤ ║  {}\n",
                        file_off_str,
                        "─".repeat(INNER_W),
                        vaddr_str,
                    ));
                }

                // Section content line: │ .text                1,234,567 B │
                {
                    let size_str = format!("{}", format_size_commas(sec.sh_size));
                    let name = &sec.name;
                    let avail = INNER_W - 2; // spaces inside │ ... │
                    let name_part = if name.len() + size_str.len() + 1 > avail {
                        let max_name = avail.saturating_sub(size_str.len() + 1);
                        &name[..max_name.min(name.len())]
                    } else {
                        name.as_str()
                    };
                    let gap = avail.saturating_sub(name_part.len() + size_str.len());
                    let blank_l = " ".repeat(pad_left + 2);
                    o.push_str(&format!(
                        "{}║ │ {}{}{} │ ║\n",
                        blank_l,
                        name_part,
                        " ".repeat(gap),
                        size_str,
                    ));
                }

                // Entry point marker if it falls in this section
                if entry >= sec.sh_addr && entry < sec_end_vaddr {
                    let marker = format!("► entry @ 0x{:X}", entry);
                    let avail = INNER_W - 3; // " ► ..."
                    let blank_l = " ".repeat(pad_left + 2);
                    o.push_str(&format!(
                        "{}║ │  {:<w$} │ ║\n",
                        blank_l,
                        marker,
                        w = avail,
                    ));
                }

                // Bottom border for last section
                if i == seg_sections.len() - 1 {
                    let file_end_str = if is_nobits {
                        " ".repeat(hw + 2)
                    } else {
                        format!("0x{:0>w$X}", sec_end_file, w = hw)
                    };
                    let vaddr_end_str = format!("0x{:0>w$X}", sec_end_vaddr, w = hw);
                    o.push_str(&format!(
                        "  {}  ║ └{}┘ ║  {}\n",
                        file_end_str,
                        "─".repeat(INNER_W),
                        vaddr_end_str,
                    ));
                }
            }
        }

        // Bottom border of last segment (only if no overlay follows)
        if seg_idx == loads.len() - 1 && overlay_size == 0 {
            let blank_l = " ".repeat(pad_left + 2);
            o.push_str(&format!(
                "{}╚{}╝\n",
                blank_l,
                "═".repeat(OUTER_W - 2)
            ));
        }
    }

    // Appended data (overlay) after the last segment
    if overlay_size > 0 {
        let blank_l = " ".repeat(pad_left + 2);
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

        o.push_str(&format!(
            "{}╚{}╝\n",
            blank_l,
            "═".repeat(OUTER_W - 2)
        ));
    }

    o.push('\n');
    o
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

// ---------------------------------------------------------------------------
// Rendering: Section Headers Table
// ---------------------------------------------------------------------------

fn format_section_table(sections: &[SectionHeader], is_64bit: bool, data: &[u8]) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════════════════\n");
    o.push_str("                            SECTION HEADERS\n");
    o.push_str("═══════════════════════════════════════════════════════════════════════════\n\n");

    if is_64bit {
        o.push_str("  [Nr] Name               Type         Address          Offset   Size     Entropy Flg\n");
        o.push_str("  ---- ----               ----         -------          ------   ----     ------- ---\n");
    } else {
        o.push_str("  [Nr] Name               Type         Address  Offset   Size     Entropy Flg\n");
        o.push_str("  ---- ----               ----         -------  ------   ----     ------- ---\n");
    }

    for (i, sh) in sections.iter().enumerate() {
        let name = if sh.name.is_empty() {
            ""
        } else {
            &sh.name
        };
        let name_trunc = if name.len() > 18 {
            &name[..18]
        } else {
            name
        };
        let type_str = section_type_str(sh.sh_type);
        let flags_str = section_flags_str(sh.sh_flags);

        let entropy = if sh.sh_type != 8 && sh.sh_size > 0 {
            // Not NOBITS
            let start = sh.sh_offset as usize;
            let end = (start + sh.sh_size as usize).min(data.len());
            if start < data.len() {
                format!("{:.2}", shannon_entropy(&data[start..end]))
            } else {
                "  -  ".into()
            }
        } else {
            "  -  ".into()
        };

        if is_64bit {
            o.push_str(&format!(
                "  [{:>2}] {:<18} {:<12} {:016X} {:08X} {:08X} {:>5}   {}\n",
                i, name_trunc, type_str, sh.sh_addr, sh.sh_offset, sh.sh_size, entropy, flags_str,
            ));
        } else {
            o.push_str(&format!(
                "  [{:>2}] {:<18} {:<12} {:08X} {:08X} {:08X} {:>5}   {}\n",
                i, name_trunc, type_str, sh.sh_addr, sh.sh_offset, sh.sh_size, entropy, flags_str,
            ));
        }
    }
    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Non-loaded sections
// ---------------------------------------------------------------------------

fn format_nonloaded_sections(
    sections: &[SectionHeader],
    loads: &[ProgramHeader],
) -> String {
    let mut o = String::new();

    let non_loaded: Vec<&SectionHeader> = sections
        .iter()
        .filter(|s| {
            if s.sh_type == 0 || s.name.is_empty() {
                return false;
            }
            // Check if section addr falls in any LOAD segment
            if s.sh_addr == 0 {
                return true; // metadata section, not loaded
            }
            !loads.iter().any(|p| {
                p.p_type == 1
                    && s.sh_addr >= p.p_vaddr
                    && s.sh_addr < p.p_vaddr.saturating_add(p.p_memsz)
            })
        })
        .collect();

    if non_loaded.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("              NON-LOADED SECTIONS (metadata)\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    for s in &non_loaded {
        o.push_str(&format!(
            "  {:<20} {:<12} offset 0x{:X}  size {}\n",
            s.name,
            section_type_str(s.sh_type),
            s.sh_offset,
            human_size(s.sh_size),
        ));
    }
    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Symbol Table
// ---------------------------------------------------------------------------

fn format_symbol_table(symbols: &[Symbol], is_64bit: bool) -> String {
    let mut o = String::new();
    if symbols.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str(&format!(
        "                  SYMBOL TABLE ({} symbols)\n",
        symbols.len()
    ));
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    let vw = if is_64bit { 16 } else { 8 };
    o.push_str(&format!(
        "  {:>w$}  {:>5}  {:<8} {:<8} Name\n",
        "Value",
        "Size",
        "Type",
        "Bind",
        w = vw + 2,
    ));
    o.push_str(&format!(
        "  {:>w$}  {:>5}  {:<8} {:<8} ----\n",
        "-----",
        "----",
        "----",
        "----",
        w = vw + 2,
    ));

    for sym in symbols {
        let name = if sym.name.is_empty() {
            "(unnamed)"
        } else if sym.name.len() > 50 {
            &sym.name[..50]
        } else {
            &sym.name
        };

        o.push_str(&format!(
            "  0x{:0>w$X}  {:>5}  {:<8} {:<8} {}\n",
            sym.value,
            sym.size,
            symbol_type_str(sym.sym_type),
            symbol_bind_str(sym.bind),
            name,
            w = vw,
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Parsing & Rendering: Dynamic Section (DT_NEEDED, RPATH, RUNPATH)
// ---------------------------------------------------------------------------

struct DynamicInfo {
    needed: Vec<String>,
    rpath: Option<String>,
    runpath: Option<String>,
    soname: Option<String>,
    bind_now: bool,
    _flags: u64,
    flags_1: u64,
}

fn parse_dynamic_section(r: &ElfReader, sections: &[SectionHeader]) -> Option<DynamicInfo> {
    // Find .dynamic section (SHT_DYNAMIC = 6)
    let dyn_sec = sections.iter().find(|s| s.sh_type == 6)?;

    // The linked string table
    let strtab_off = if (dyn_sec.sh_link as usize) < sections.len() {
        sections[dyn_sec.sh_link as usize].sh_offset as usize
    } else {
        0
    };

    let entry_size = if r.is_64bit { 16 } else { 8 };
    let count = (dyn_sec.sh_size / entry_size as u64) as usize;

    let mut needed = Vec::new();
    let mut rpath = None;
    let mut runpath = None;
    let mut soname = None;
    let mut bind_now = false;
    let mut flags: u64 = 0;
    let mut flags_1: u64 = 0;

    for i in 0..count.min(512) {
        let base = dyn_sec.sh_offset as usize + i * entry_size;
        let (tag, val) = if r.is_64bit {
            if base + 16 > r.data.len() { break; }
            (r.u64(base) as i64, r.u64(base + 8))
        } else {
            if base + 8 > r.data.len() { break; }
            (r.u32(base) as i32 as i64, r.u32(base + 4) as u64)
        };

        match tag {
            0 => break, // DT_NULL
            1 => { // DT_NEEDED
                if strtab_off > 0 {
                    needed.push(r.read_cstr(strtab_off + val as usize));
                }
            }
            14 => { // DT_SONAME
                if strtab_off > 0 {
                    soname = Some(r.read_cstr(strtab_off + val as usize));
                }
            }
            15 => { // DT_RPATH
                if strtab_off > 0 {
                    rpath = Some(r.read_cstr(strtab_off + val as usize));
                }
            }
            24 => { // DT_BIND_NOW
                bind_now = true;
            }
            29 => { // DT_RUNPATH
                if strtab_off > 0 {
                    runpath = Some(r.read_cstr(strtab_off + val as usize));
                }
            }
            30 => { // DT_FLAGS
                flags = val;
                if val & 0x8 != 0 { // DF_BIND_NOW
                    bind_now = true;
                }
            }
            0x6ffffffb => { // DT_FLAGS_1
                flags_1 = val;
                if val & 0x1 != 0 { // DF_1_NOW
                    bind_now = true;
                }
            }
            _ => {}
        }
    }

    Some(DynamicInfo {
        needed,
        rpath,
        runpath,
        soname,
        bind_now,
        _flags: flags,
        flags_1,
    })
}

fn format_dynamic_info(di: &DynamicInfo) -> String {
    let mut o = String::new();

    // Only show if there's something to display
    if di.needed.is_empty() && di.rpath.is_none() && di.runpath.is_none() && di.soname.is_none() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                    DYNAMIC LIBRARIES\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    if let Some(ref sn) = di.soname {
        o.push_str(&format!("  SONAME:   {}\n\n", sn));
    }

    if !di.needed.is_empty() {
        o.push_str(&format!("  Needed shared libraries ({}):\n", di.needed.len()));
        for lib in &di.needed {
            o.push_str(&format!("    {}\n", lib));
        }
        o.push('\n');
    }

    if let Some(ref rp) = di.rpath {
        o.push_str(&format!("  RPATH:    {}  ⚠ (deprecated, prefer RUNPATH)\n", rp));
    }
    if let Some(ref rp) = di.runpath {
        o.push_str(&format!("  RUNPATH:  {}\n", rp));
    }
    if di.rpath.is_some() || di.runpath.is_some() {
        o.push('\n');
    }

    o
}

// ---------------------------------------------------------------------------
// Parsing: Interpreter (PT_INTERP)
// ---------------------------------------------------------------------------

fn parse_interpreter(r: &ElfReader, phs: &[ProgramHeader]) -> Option<String> {
    // PT_INTERP = 3
    let interp = phs.iter().find(|p| p.p_type == 3)?;
    let off = interp.p_offset as usize;
    let size = interp.p_filesz as usize;
    if off + size > r.data.len() || size == 0 {
        return None;
    }
    // Remove trailing null
    let end = if r.data[off + size - 1] == 0 { off + size - 1 } else { off + size };
    Some(String::from_utf8_lossy(&r.data[off..end]).into_owned())
}

// ---------------------------------------------------------------------------
// Parsing: Build ID (.note.gnu.build-id)
// ---------------------------------------------------------------------------

fn parse_build_id(r: &ElfReader, sections: &[SectionHeader]) -> Option<String> {
    // Look for .note.gnu.build-id section
    let note_sec = sections.iter().find(|s| s.name == ".note.gnu.build-id")?;
    let off = note_sec.sh_offset as usize;
    let size = note_sec.sh_size as usize;
    if off + size > r.data.len() || size < 16 {
        return None;
    }

    // Note format: namesz(4), descsz(4), type(4), name(aligned), desc
    let namesz = r.u32(off) as usize;
    let descsz = r.u32(off + 4) as usize;
    let note_type = r.u32(off + 8);

    if note_type != 3 {
        // NT_GNU_BUILD_ID = 3
        return None;
    }

    let name_aligned = (namesz + 3) & !3;
    let desc_off = off + 12 + name_aligned;
    if desc_off + descsz > r.data.len() {
        return None;
    }

    let id: String = r.data[desc_off..desc_off + descsz]
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    Some(id)
}

// ---------------------------------------------------------------------------
// Parsing: GNU Notes
// ---------------------------------------------------------------------------

struct GnuNote {
    name: String,
    note_type: u32,
    description: String,
}

fn parse_gnu_notes(r: &ElfReader, phs: &[ProgramHeader]) -> Vec<GnuNote> {
    let mut notes = Vec::new();

    // PT_NOTE = 4
    for ph in phs.iter().filter(|p| p.p_type == 4) {
        let off = ph.p_offset as usize;
        let end = off + ph.p_filesz as usize;
        if end > r.data.len() {
            continue;
        }

        let mut pos = off;
        while pos + 12 <= end {
            let namesz = r.u32(pos) as usize;
            let descsz = r.u32(pos + 4) as usize;
            let note_type = r.u32(pos + 8);
            pos += 12;

            let name_aligned = (namesz + 3) & !3;
            let desc_aligned = (descsz + 3) & !3;

            if pos + name_aligned + desc_aligned > end {
                break;
            }

            let name = if namesz > 0 {
                let end_n = if namesz > 0 && r.data.get(pos + namesz - 1) == Some(&0) {
                    pos + namesz - 1
                } else {
                    pos + namesz
                };
                String::from_utf8_lossy(&r.data[pos..end_n]).into_owned()
            } else {
                String::new()
            };

            let desc_start = pos + name_aligned;
            let description = if name == "GNU" && note_type == 1 && descsz >= 16 {
                // NT_GNU_ABI_TAG
                let os = match r.u32(desc_start) {
                    0 => "Linux",
                    1 => "Hurd",
                    2 => "Solaris",
                    3 => "FreeBSD",
                    _ => "Unknown",
                };
                let major = r.u32(desc_start + 4);
                let minor = r.u32(desc_start + 8);
                let patch = r.u32(desc_start + 12);
                format!("{} {}.{}.{}", os, major, minor, patch)
            } else if name == "GNU" && note_type == 3 {
                // NT_GNU_BUILD_ID — handled separately
                pos += name_aligned + desc_aligned;
                continue;
            } else if name == "GNU" && note_type == 4 && descsz >= 4 {
                // NT_GNU_GOLD_VERSION or NT_GNU_PROPERTY_TYPE_0
                format!("(property, {} bytes)", descsz)
            } else {
                format!("({} bytes)", descsz)
            };

            notes.push(GnuNote {
                name,
                note_type,
                description,
            });

            pos += name_aligned + desc_aligned;
        }
    }

    notes
}

// ---------------------------------------------------------------------------
// Init/Fini Arrays
// ---------------------------------------------------------------------------

struct InitFiniInfo {
    init_array_count: usize,
    fini_array_count: usize,
    has_init: bool,
    has_fini: bool,
}

fn parse_init_fini(sections: &[SectionHeader], is_64bit: bool) -> InitFiniInfo {
    let ptr_size = if is_64bit { 8 } else { 4 };

    let init_array_count = sections
        .iter()
        .find(|s| s.sh_type == 14) // SHT_INIT_ARRAY
        .map(|s| (s.sh_size as usize) / ptr_size)
        .unwrap_or(0);

    let fini_array_count = sections
        .iter()
        .find(|s| s.sh_type == 15) // SHT_FINI_ARRAY
        .map(|s| (s.sh_size as usize) / ptr_size)
        .unwrap_or(0);

    let has_init = sections.iter().any(|s| s.name == ".init");
    let has_fini = sections.iter().any(|s| s.name == ".fini");

    InitFiniInfo {
        init_array_count,
        fini_array_count,
        has_init,
        has_fini,
    }
}

// ---------------------------------------------------------------------------
// Rendering: Security Summary (checksec)
// ---------------------------------------------------------------------------

fn format_security_summary(
    r: &ElfReader,
    phs: &[ProgramHeader],
    symbols: &[Symbol],
    dyn_info: Option<&DynamicInfo>,
) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     SECURITY FEATURES\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // RELRO: check for PT_GNU_RELRO (type 0x6474e552)
    let has_relro = phs.iter().any(|p| p.p_type == 0x6474e552);
    let bind_now = dyn_info.map(|d| d.bind_now).unwrap_or(false);
    let relro = if has_relro && bind_now {
        "Full"
    } else if has_relro {
        "Partial"
    } else {
        "No"
    };
    o.push_str(&format!("  RELRO:          {}\n", relro));

    // Stack canary: check for __stack_chk_fail in symbols
    let has_canary = symbols.iter().any(|s| s.name == "__stack_chk_fail" || s.name == "__stack_chk_guard");
    o.push_str(&format!("  Stack canary:   {}\n", if has_canary { "Yes" } else { "No" }));

    // NX/DEP: check PT_GNU_STACK (type 0x6474e551) — NX if not executable
    let gnu_stack = phs.iter().find(|p| p.p_type == 0x6474e551);
    let nx = match gnu_stack {
        Some(seg) => seg.p_flags & 1 == 0, // PF_X not set = NX enabled
        None => true, // No GNU_STACK usually means NX
    };
    o.push_str(&format!("  NX (DEP):       {}\n", if nx { "Yes" } else { "No (stack executable)" }));

    // PIE: check if e_type is ET_DYN (3) and has PT_INTERP (not just a shared lib)
    let e_type = r.u16(16);
    let has_interp = phs.iter().any(|p| p.p_type == 3);
    let pie = e_type == 3 && has_interp; // DYN + INTERP = PIE executable
    let pie_str = if e_type == 3 {
        if has_interp { "Yes (PIE executable)" } else { "DSO (shared object)" }
    } else if e_type == 2 {
        "No (static executable)"
    } else {
        "N/A"
    };
    o.push_str(&format!("  PIE:            {}\n", pie_str));

    // FORTIFY: check for _chk functions in symbols (e.g., __memcpy_chk, __printf_chk)
    let fortify_count = symbols.iter().filter(|s| s.name.ends_with("_chk")).count();
    if fortify_count > 0 {
        o.push_str(&format!("  FORTIFY:        Yes ({} protected functions)\n", fortify_count));
    } else {
        o.push_str("  FORTIFY:        No\n");
    }

    // BIND_NOW
    if bind_now {
        o.push_str("  BIND_NOW:       Yes\n");
    }

    // Check for interesting DT_FLAGS_1
    if let Some(di) = dyn_info {
        if di.flags_1 & 0x08000000 != 0 {
            o.push_str("  PIE flag:       Yes (DF_1_PIE)\n");
        }
    }

    let _ = pie; // used above

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: ELF Info Summary (interpreter, build ID, notes, init/fini)
// ---------------------------------------------------------------------------

fn format_elf_info(
    interpreter: Option<&str>,
    build_id: Option<&str>,
    notes: &[GnuNote],
    init_fini: &InitFiniInfo,
) -> String {
    let mut o = String::new();

    let has_content = interpreter.is_some()
        || build_id.is_some()
        || !notes.is_empty()
        || init_fini.init_array_count > 0
        || init_fini.fini_array_count > 0
        || init_fini.has_init
        || init_fini.has_fini;

    if !has_content {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       ELF INFO\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    if let Some(interp) = interpreter {
        o.push_str(&format!("  Interpreter:    {}\n", interp));
    }
    if let Some(bid) = build_id {
        o.push_str(&format!("  Build ID:       {}\n", bid));
    }

    // GNU notes
    for note in notes {
        let type_str = match (note.name.as_str(), note.note_type) {
            ("GNU", 1) => "ABI tag",
            ("GNU", 2) => "hwcap",
            ("GNU", 4) => "property",
            _ => "note",
        };
        o.push_str(&format!("  GNU {}:  {}\n", type_str, note.description));
    }

    // Init/Fini
    if init_fini.has_init || init_fini.init_array_count > 0 {
        let mut parts = Vec::new();
        if init_fini.has_init {
            parts.push(".init".to_string());
        }
        if init_fini.init_array_count > 0 {
            parts.push(format!(".init_array[{}]", init_fini.init_array_count));
        }
        o.push_str(&format!("  Init:           {}\n", parts.join(", ")));
    }
    if init_fini.has_fini || init_fini.fini_array_count > 0 {
        let mut parts = Vec::new();
        if init_fini.has_fini {
            parts.push(".fini".to_string());
        }
        if init_fini.fini_array_count > 0 {
            parts.push(format!(".fini_array[{}]", init_fini.fini_array_count));
        }
        o.push_str(&format!("  Fini:           {}\n", parts.join(", ")));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse an ELF file and return formatted output
pub fn parse_elf(path: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|e| e.to_string())?;
    parse_elf_bytes(&data)
}

/// Parse ELF data from bytes
pub fn parse_elf_bytes(data: &[u8]) -> Result<String, String> {
    let r = ElfReader::new(data)?;

    let entry = if r.is_64bit {
        r.u64(24)
    } else {
        r.u32(24) as u64
    };

    let phs = parse_program_headers(&r);
    let sections = parse_section_headers(&r);
    let symbols = parse_symbols(&r, &sections);
    let dyn_info = parse_dynamic_section(&r, &sections);
    let interpreter = parse_interpreter(&r, &phs);
    let build_id = parse_build_id(&r, &sections);
    let gnu_notes = parse_gnu_notes(&r, &phs);
    let init_fini = parse_init_fini(&sections, r.is_64bit);

    let mut output = String::new();

    // 1. ELF Header
    output.push_str(&format_header(&r));

    // 2. Security Features (checksec)
    output.push_str(&format_security_summary(&r, &phs, &symbols, dyn_info.as_ref()));

    // 3. ELF Info (interpreter, build ID, notes, init/fini)
    output.push_str(&format_elf_info(
        interpreter.as_deref(),
        build_id.as_deref(),
        &gnu_notes,
        &init_fini,
    ));

    // 4. Dynamic Libraries (DT_NEEDED, RPATH, RUNPATH)
    if let Some(ref di) = dyn_info {
        output.push_str(&format_dynamic_info(di));
    }

    // 5. Memory Layout (LOAD segments + sections)
    let file_size = data.len() as u64;
    output.push_str(&render_memory_map(&phs, &sections, entry, file_size));

    // 6. Section Headers Table (with entropy)
    if !sections.is_empty() {
        output.push_str(&format_section_table(&sections, r.is_64bit, data));
    }

    // 7. Non-loaded sections
    if !sections.is_empty() && !phs.is_empty() {
        output.push_str(&format_nonloaded_sections(&sections, &phs));
    }

    // 8. Symbol Table
    output.push_str(&format_symbol_table(&symbols, r.is_64bit));

    Ok(output)
}
