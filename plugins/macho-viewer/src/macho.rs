//! Mach-O file parsing and rich ASCII rendering module

use std::fs::File;
use std::io::Read;

// ---------------------------------------------------------------------------
// Mach-O magic constants
// ---------------------------------------------------------------------------

const MH_MAGIC: u32 = 0xFEEDFACE; // 32-bit little-endian
const MH_CIGAM: u32 = 0xCEFAEDFE; // 32-bit big-endian
const MH_MAGIC_64: u32 = 0xFEEDFACF; // 64-bit little-endian
const MH_CIGAM_64: u32 = 0xCFFAEDFE; // 64-bit big-endian
const FAT_MAGIC: u32 = 0xCAFEBABE; // Universal binary big-endian
const FAT_CIGAM: u32 = 0xBEBAFECA; // Universal binary little-endian

// Load command types
const LC_SEGMENT: u32 = 0x01;
const LC_SYMTAB: u32 = 0x02;
const LC_UNIXTHREAD: u32 = 0x05;
#[allow(dead_code)]
const LC_DYSYMTAB: u32 = 0x0B;
const LC_LOAD_DYLIB: u32 = 0x0C;
const LC_ID_DYLIB: u32 = 0x0D;
const LC_LOAD_WEAK_DYLIB: u32 = 0x80000018;
const LC_SEGMENT_64: u32 = 0x19;
const LC_UUID: u32 = 0x1B;
const LC_RPATH: u32 = 0x8000001C;
const LC_CODE_SIGNATURE: u32 = 0x1D;
const LC_ENCRYPTION_INFO: u32 = 0x21;
#[allow(dead_code)]
const LC_DYLD_INFO: u32 = 0x22;
#[allow(dead_code)]
const LC_DYLD_INFO_ONLY: u32 = 0x80000022;
const LC_VERSION_MIN_MACOSX: u32 = 0x24;
const LC_VERSION_MIN_IPHONEOS: u32 = 0x25;
#[allow(dead_code)]
const LC_FUNCTION_STARTS: u32 = 0x26;
#[allow(dead_code)]
const LC_DATA_IN_CODE: u32 = 0x29;
const LC_SOURCE_VERSION: u32 = 0x2A;
const LC_ENCRYPTION_INFO_64: u32 = 0x2C;
const LC_VERSION_MIN_TVOS: u32 = 0x2F;
const LC_VERSION_MIN_WATCHOS: u32 = 0x30;
const LC_BUILD_VERSION: u32 = 0x32;
const LC_MAIN: u32 = 0x80000028;
const LC_REEXPORT_DYLIB: u32 = 0x8000001F;
const LC_LAZY_LOAD_DYLIB: u32 = 0x20;
const LC_LOAD_UPWARD_DYLIB: u32 = 0x80000023;

// Mach-O header flags
const MH_PIE: u32 = 0x00200000;
const MH_NO_HEAP_EXECUTION: u32 = 0x01000000;
const MH_TWOLEVEL: u32 = 0x00000080;
const MH_ALLOW_STACK_EXECUTION: u32 = 0x00020000;

// Code signature constants
const CSMAGIC_EMBEDDED_SIGNATURE: u32 = 0xFADE0CC0;
const CSMAGIC_CODEDIRECTORY: u32 = 0xFADE0C02;
const CSMAGIC_ENTITLEMENTS: u32 = 0xFADE7171;
const CS_RUNTIME: u32 = 0x00010000;
const CS_REQUIRE_LV: u32 = 0x00002000;

/// Check if the given bytes are a Mach-O magic
pub fn is_macho(magic: &[u8]) -> bool {
    if magic.len() < 4 {
        return false;
    }
    let m = u32::from_be_bytes([magic[0], magic[1], magic[2], magic[3]]);
    matches!(
        m,
        MH_MAGIC | MH_CIGAM | MH_MAGIC_64 | MH_CIGAM_64 | FAT_MAGIC | FAT_CIGAM
    )
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

struct MachoReader<'a> {
    data: &'a [u8],
    is_le: bool,
    is_64bit: bool,
}

impl<'a> MachoReader<'a> {
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

    /// Read a fixed-size string field (like segment/section names, padded with nulls)
    fn read_fixed_str(&self, offset: usize, max_len: usize) -> String {
        if offset >= self.data.len() {
            return String::new();
        }
        let end = (offset + max_len).min(self.data.len());
        let slice = &self.data[offset..end];
        let nul = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
        String::from_utf8_lossy(&slice[..nul]).into_owned()
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct MachoHeader {
    magic: u32,
    cpu_type: i32,
    cpu_subtype: i32,
    file_type: u32,
    n_cmds: u32,
    size_of_cmds: u32,
    flags: u32,
}

struct FatArch {
    cpu_type: i32,
    cpu_subtype: i32,
    offset: u64,
    size: u64,
    align: u32,
}

#[derive(Clone)]
#[allow(dead_code)]
struct Section {
    sect_name: String,
    #[allow(dead_code)]
    seg_name: String,
    addr: u64,
    size: u64,
    offset: u32,
    flags: u32,
}

#[derive(Clone)]
struct Segment {
    name: String,
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
    maxprot: i32,
    initprot: i32,
    sections: Vec<Section>,
}

#[derive(Clone)]
struct LinkedLib {
    name: String,
    current_version: u32,
    compat_version: u32,
    kind: LibKind,
}

#[derive(Clone)]
enum LibKind {
    Required,
    Weak,
    Reexport,
    Lazy,
    Upward,
    Id,
}

#[derive(Clone)]
#[allow(dead_code)]
struct Symbol {
    name: String,
    n_type: u8,
    n_sect: u8,
    n_desc: i16,
    value: u64,
}

struct CodeSignature {
    hash_type: Option<String>,
    team_id: Option<String>,
    cd_flags: u32,
    entitlement_keys: Vec<String>,
    signed: bool,
}

struct BuildVersion {
    platform: u32,
    minos: u32,
    sdk: u32,
    tools: Vec<(u32, u32)>, // (tool, version)
}

struct EntryPoint {
    kind: EntryKind,
    value: u64,
}

enum EntryKind {
    LcMain,
    UnixThread,
}

struct EncryptionInfo {
    crypt_offset: u32,
    crypt_size: u32,
    crypt_id: u32,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn detect_endianness_and_bits(magic_bytes: &[u8; 4]) -> Option<(bool, bool)> {
    let be = u32::from_be_bytes(*magic_bytes);
    match be {
        MH_MAGIC => Some((false, false)),   // BE, 32-bit (raw bytes FE ED FA CE)
        MH_CIGAM => Some((true, false)),    // LE, 32-bit (raw bytes CE FA ED FE)
        MH_MAGIC_64 => Some((false, true)), // BE, 64-bit (raw bytes FE ED FA CF)
        MH_CIGAM_64 => Some((true, true)),  // LE, 64-bit (raw bytes CF FA ED FE)
        _ => None,
    }
}

fn parse_header(r: &MachoReader) -> MachoHeader {
    MachoHeader {
        magic: r.u32(0),
        cpu_type: r.u32(4) as i32,
        cpu_subtype: r.u32(8) as i32,
        file_type: r.u32(12),
        n_cmds: r.u32(16),
        size_of_cmds: r.u32(20),
        flags: r.u32(24),
    }
}

fn parse_fat_header(data: &[u8]) -> Option<Vec<FatArch>> {
    if data.len() < 8 {
        return None;
    }
    // Fat headers are always big-endian
    let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if magic != FAT_MAGIC && magic != FAT_CIGAM {
        return None;
    }
    let is_le = magic == FAT_CIGAM;
    let nfat = if is_le {
        u32::from_le_bytes([data[4], data[5], data[6], data[7]])
    } else {
        u32::from_be_bytes([data[4], data[5], data[6], data[7]])
    } as usize;

    let mut archs = Vec::new();
    for i in 0..nfat.min(16) {
        let base = 8 + i * 20;
        if base + 20 > data.len() {
            break;
        }
        let read_u32 = |off: usize| -> u32 {
            if is_le {
                u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
            } else {
                u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
            }
        };
        archs.push(FatArch {
            cpu_type: read_u32(base) as i32,
            cpu_subtype: read_u32(base + 4) as i32,
            offset: read_u32(base + 8) as u64,
            size: read_u32(base + 12) as u64,
            align: read_u32(base + 16),
        });
    }
    Some(archs)
}

fn parse_segments(r: &MachoReader, header: &MachoHeader) -> Vec<Segment> {
    let mut segments = Vec::new();
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;
    let end = header_size + header.size_of_cmds as usize;

    for _ in 0..header.n_cmds {
        if off + 8 > end || off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if cmd == LC_SEGMENT_64 && r.is_64bit {
            if off + 72 > r.data.len() {
                off += cmdsize;
                continue;
            }
            let name = r.read_fixed_str(off + 8, 16);
            let vmaddr = r.u64(off + 24);
            let vmsize = r.u64(off + 32);
            let fileoff = r.u64(off + 40);
            let filesize = r.u64(off + 48);
            let maxprot = r.u32(off + 56) as i32;
            let initprot = r.u32(off + 60) as i32;
            let nsects = r.u32(off + 64) as usize;

            let mut sections = Vec::new();
            let mut sec_off = off + 72;
            for _ in 0..nsects.min(256) {
                if sec_off + 80 > r.data.len() {
                    break;
                }
                sections.push(Section {
                    sect_name: r.read_fixed_str(sec_off, 16),
                    seg_name: r.read_fixed_str(sec_off + 16, 16),
                    addr: r.u64(sec_off + 32),
                    size: r.u64(sec_off + 40),
                    offset: r.u32(sec_off + 48),
                    flags: r.u32(sec_off + 64),
                });
                sec_off += 80;
            }

            segments.push(Segment {
                name,
                vmaddr,
                vmsize,
                fileoff,
                filesize,
                maxprot,
                initprot,
                sections,
            });
        } else if cmd == LC_SEGMENT && !r.is_64bit {
            if off + 56 > r.data.len() {
                off += cmdsize;
                continue;
            }
            let name = r.read_fixed_str(off + 8, 16);
            let vmaddr = r.u32(off + 24) as u64;
            let vmsize = r.u32(off + 28) as u64;
            let fileoff = r.u32(off + 32) as u64;
            let filesize = r.u32(off + 36) as u64;
            let maxprot = r.u32(off + 40) as i32;
            let initprot = r.u32(off + 44) as i32;
            let nsects = r.u32(off + 48) as usize;

            let mut sections = Vec::new();
            let mut sec_off = off + 56;
            for _ in 0..nsects.min(256) {
                if sec_off + 68 > r.data.len() {
                    break;
                }
                sections.push(Section {
                    sect_name: r.read_fixed_str(sec_off, 16),
                    seg_name: r.read_fixed_str(sec_off + 16, 16),
                    addr: r.u32(sec_off + 32) as u64,
                    size: r.u32(sec_off + 36) as u64,
                    offset: r.u32(sec_off + 40),
                    flags: r.u32(sec_off + 56),
                });
                sec_off += 68;
            }

            segments.push(Segment {
                name,
                vmaddr,
                vmsize,
                fileoff,
                filesize,
                maxprot,
                initprot,
                sections,
            });
        }

        off += cmdsize;
    }
    segments
}

fn parse_dylibs(r: &MachoReader, header: &MachoHeader) -> Vec<LinkedLib> {
    let mut libs = Vec::new();
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        let kind = match cmd {
            LC_LOAD_DYLIB => Some(LibKind::Required),
            LC_LOAD_WEAK_DYLIB => Some(LibKind::Weak),
            LC_REEXPORT_DYLIB => Some(LibKind::Reexport),
            LC_LAZY_LOAD_DYLIB => Some(LibKind::Lazy),
            LC_LOAD_UPWARD_DYLIB => Some(LibKind::Upward),
            LC_ID_DYLIB => Some(LibKind::Id),
            _ => None,
        };

        if let Some(kind) = kind {
            if off + 24 <= r.data.len() {
                let name_offset = r.u32(off + 8) as usize;
                let current_version = r.u32(off + 16);
                let compat_version = r.u32(off + 20);
                let name = if name_offset < cmdsize {
                    r.read_cstr(off + name_offset)
                } else {
                    String::from("(unknown)")
                };
                libs.push(LinkedLib {
                    name,
                    current_version,
                    compat_version,
                    kind,
                });
            }
        }

        off += cmdsize;
    }
    libs
}

fn parse_rpaths(r: &MachoReader, header: &MachoHeader) -> Vec<String> {
    let mut rpaths = Vec::new();
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if cmd == LC_RPATH && off + 12 <= r.data.len() {
            let path_offset = r.u32(off + 8) as usize;
            if path_offset < cmdsize {
                rpaths.push(r.read_cstr(off + path_offset));
            }
        }

        off += cmdsize;
    }
    rpaths
}

fn parse_uuid(r: &MachoReader, header: &MachoHeader) -> Option<String> {
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if cmd == LC_UUID && off + 24 <= r.data.len() {
            let uuid_bytes = &r.data[off + 8..off + 24];
            let s = format!(
                "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                uuid_bytes[0], uuid_bytes[1], uuid_bytes[2], uuid_bytes[3],
                uuid_bytes[4], uuid_bytes[5],
                uuid_bytes[6], uuid_bytes[7],
                uuid_bytes[8], uuid_bytes[9],
                uuid_bytes[10], uuid_bytes[11], uuid_bytes[12], uuid_bytes[13], uuid_bytes[14], uuid_bytes[15],
            );
            return Some(s);
        }

        off += cmdsize;
    }
    None
}

fn parse_build_version(r: &MachoReader, header: &MachoHeader) -> Option<BuildVersion> {
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if cmd == LC_BUILD_VERSION && off + 24 <= r.data.len() {
            let platform = r.u32(off + 8);
            let minos = r.u32(off + 12);
            let sdk = r.u32(off + 16);
            let ntools = r.u32(off + 20) as usize;
            let mut tools = Vec::new();
            for i in 0..ntools.min(16) {
                let toff = off + 24 + i * 8;
                if toff + 8 > r.data.len() {
                    break;
                }
                tools.push((r.u32(toff), r.u32(toff + 4)));
            }
            return Some(BuildVersion {
                platform,
                minos,
                sdk,
                tools,
            });
        }

        off += cmdsize;
    }
    None
}

/// Parse LC_VERSION_MIN_* commands as a fallback when LC_BUILD_VERSION is absent
fn parse_version_min(r: &MachoReader, header: &MachoHeader) -> Option<(u32, u32, u32)> {
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        let platform = match cmd {
            LC_VERSION_MIN_MACOSX => Some(1),
            LC_VERSION_MIN_IPHONEOS => Some(2),
            LC_VERSION_MIN_TVOS => Some(3),
            LC_VERSION_MIN_WATCHOS => Some(4),
            _ => None,
        };

        if let Some(plat) = platform {
            if off + 16 <= r.data.len() {
                let version = r.u32(off + 8);
                let sdk = r.u32(off + 12);
                return Some((plat, version, sdk));
            }
        }

        off += cmdsize;
    }
    None
}

fn parse_source_version(r: &MachoReader, header: &MachoHeader) -> Option<u64> {
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if cmd == LC_SOURCE_VERSION && off + 16 <= r.data.len() {
            return Some(r.u64(off + 8));
        }

        off += cmdsize;
    }
    None
}

fn parse_entry_point(r: &MachoReader, header: &MachoHeader) -> Option<EntryPoint> {
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if cmd == LC_MAIN && off + 16 <= r.data.len() {
            return Some(EntryPoint {
                kind: EntryKind::LcMain,
                value: r.u64(off + 8),
            });
        }

        if cmd == LC_UNIXTHREAD {
            // The thread state follows: flavor(4), count(4), then registers
            // For x86_64: RIP is at offset 16*8 = 128 from start of state
            // For ARM64: PC is at register 32 = offset 32*8 = 256 from state start
            // For x86 (32-bit): EIP is at offset 10*4 = 40 from start of state
            // State starts at off+16 (after cmd+cmdsize+flavor+count)
            if off + 24 <= r.data.len() {
                let flavor = r.u32(off + 8);
                let _count = r.u32(off + 12);
                let state_off = off + 16;
                let pc = match (header.cpu_type, flavor) {
                    (0x01000007, _) => {
                        // x86_64: RIP at index 16 (128 bytes into state)
                        if state_off + 136 <= r.data.len() {
                            Some(r.u64(state_off + 128))
                        } else {
                            None
                        }
                    }
                    (0x0100000C, _) => {
                        // ARM64: PC at index 32 (256 bytes into state)
                        if state_off + 264 <= r.data.len() {
                            Some(r.u64(state_off + 256))
                        } else {
                            None
                        }
                    }
                    (7, _) => {
                        // x86 (32-bit): EIP at index 10 (40 bytes into state)
                        if state_off + 44 <= r.data.len() {
                            Some(r.u32(state_off + 40) as u64)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(val) = pc {
                    return Some(EntryPoint {
                        kind: EntryKind::UnixThread,
                        value: val,
                    });
                }
            }
        }

        off += cmdsize;
    }
    None
}

fn parse_encryption_info(r: &MachoReader, header: &MachoHeader) -> Option<EncryptionInfo> {
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if (cmd == LC_ENCRYPTION_INFO || cmd == LC_ENCRYPTION_INFO_64) && off + 24 <= r.data.len()
        {
            return Some(EncryptionInfo {
                crypt_offset: r.u32(off + 8),
                crypt_size: r.u32(off + 12),
                crypt_id: r.u32(off + 16),
            });
        }

        off += cmdsize;
    }
    None
}

fn parse_symbols(r: &MachoReader, header: &MachoHeader) -> Vec<Symbol> {
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;
    let mut symtab_off = 0u32;
    let mut symtab_n = 0u32;
    let mut strtab_off = 0u32;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if cmd == LC_SYMTAB && off + 24 <= r.data.len() {
            symtab_off = r.u32(off + 8);
            symtab_n = r.u32(off + 12);
            strtab_off = r.u32(off + 16);
            break;
        }

        off += cmdsize;
    }

    if symtab_off == 0 || symtab_n == 0 {
        return Vec::new();
    }

    let entry_size = if r.is_64bit { 16usize } else { 12usize };
    let mut syms = Vec::new();
    let cap = (symtab_n as usize).min(2000);

    for i in 0..cap {
        let base = symtab_off as usize + i * entry_size;
        if base + entry_size > r.data.len() {
            break;
        }

        let str_idx = r.u32(base);
        let n_type = r.data.get(base + 4).copied().unwrap_or(0);
        let n_sect = r.data.get(base + 5).copied().unwrap_or(0);
        let n_desc = r.u16(base + 6) as i16;
        let value = if r.is_64bit {
            r.u64(base + 8)
        } else {
            r.u32(base + 8) as u64
        };

        // Skip debug symbols (N_STAB)
        if n_type & 0xE0 != 0 {
            continue;
        }

        let name = if str_idx > 0 {
            r.read_cstr(strtab_off as usize + str_idx as usize)
        } else {
            String::new()
        };

        syms.push(Symbol {
            name,
            n_type,
            n_sect,
            n_desc,
            value,
        });
    }

    syms.sort_by_key(|s| s.value);
    syms
}

fn parse_code_signature(r: &MachoReader, header: &MachoHeader) -> Option<CodeSignature> {
    let header_size = if r.is_64bit { 32 } else { 28 };
    let mut off = header_size;
    let mut cs_offset = 0u32;
    let mut cs_size = 0u32;

    for _ in 0..header.n_cmds {
        if off + 8 > r.data.len() {
            break;
        }
        let cmd = r.u32(off);
        let cmdsize = r.u32(off + 4) as usize;
        if cmdsize < 8 {
            break;
        }

        if cmd == LC_CODE_SIGNATURE && off + 16 <= r.data.len() {
            cs_offset = r.u32(off + 8);
            cs_size = r.u32(off + 12);
            break;
        }

        off += cmdsize;
    }

    if cs_offset == 0 || cs_size == 0 {
        return None;
    }

    let cs_start = cs_offset as usize;
    let cs_end = (cs_start + cs_size as usize).min(r.data.len());
    if cs_start + 12 > cs_end {
        return Some(CodeSignature {
            hash_type: None,
            team_id: None,
            cd_flags: 0,
            entitlement_keys: Vec::new(),
            signed: true,
        });
    }

    // Read super blob (always big-endian)
    let sb_magic = u32::from_be_bytes([
        r.data[cs_start],
        r.data[cs_start + 1],
        r.data[cs_start + 2],
        r.data[cs_start + 3],
    ]);

    if sb_magic != CSMAGIC_EMBEDDED_SIGNATURE {
        return Some(CodeSignature {
            hash_type: None,
            team_id: None,
            cd_flags: 0,
            entitlement_keys: Vec::new(),
            signed: true,
        });
    }

    let blob_count = u32::from_be_bytes([
        r.data[cs_start + 8],
        r.data[cs_start + 9],
        r.data[cs_start + 10],
        r.data[cs_start + 11],
    ]) as usize;

    let mut hash_type = None;
    let mut team_id = None;
    let mut cd_flags = 0u32;
    let mut entitlement_keys = Vec::new();

    for i in 0..blob_count.min(16) {
        let idx_off = cs_start + 12 + i * 8;
        if idx_off + 8 > cs_end {
            break;
        }
        let _blob_type = u32::from_be_bytes([
            r.data[idx_off],
            r.data[idx_off + 1],
            r.data[idx_off + 2],
            r.data[idx_off + 3],
        ]);
        let blob_off = u32::from_be_bytes([
            r.data[idx_off + 4],
            r.data[idx_off + 5],
            r.data[idx_off + 6],
            r.data[idx_off + 7],
        ]) as usize;

        let abs_off = cs_start + blob_off;
        if abs_off + 8 > cs_end {
            continue;
        }

        let magic = u32::from_be_bytes([
            r.data[abs_off],
            r.data[abs_off + 1],
            r.data[abs_off + 2],
            r.data[abs_off + 3],
        ]);

        if magic == CSMAGIC_CODEDIRECTORY {
            // Parse code directory
            if abs_off + 44 > cs_end {
                continue;
            }
            let _length = u32::from_be_bytes([
                r.data[abs_off + 4],
                r.data[abs_off + 5],
                r.data[abs_off + 6],
                r.data[abs_off + 7],
            ]);
            cd_flags = u32::from_be_bytes([
                r.data[abs_off + 12],
                r.data[abs_off + 13],
                r.data[abs_off + 14],
                r.data[abs_off + 15],
            ]);
            let hash_type_byte = r.data.get(abs_off + 36).copied().unwrap_or(0);
            hash_type = Some(match hash_type_byte {
                1 => "SHA-1".to_string(),
                2 => "SHA-256".to_string(),
                3 => "SHA-256 (truncated)".to_string(),
                4 => "SHA-384".to_string(),
                5 => "SHA-512".to_string(),
                _ => format!("Unknown ({})", hash_type_byte),
            });

            // Team ID (if version >= 0x20200)
            if abs_off + 44 <= cs_end {
                let version = u32::from_be_bytes([
                    r.data[abs_off + 8],
                    r.data[abs_off + 9],
                    r.data[abs_off + 10],
                    r.data[abs_off + 11],
                ]);
                if version >= 0x20200 && abs_off + 48 <= cs_end {
                    let team_off = u32::from_be_bytes([
                        r.data[abs_off + 44],
                        r.data[abs_off + 45],
                        r.data[abs_off + 46],
                        r.data[abs_off + 47],
                    ]) as usize;
                    if team_off > 0 {
                        let team_abs = abs_off + team_off;
                        if team_abs < cs_end {
                            let mut end = team_abs;
                            while end < cs_end && r.data[end] != 0 {
                                end += 1;
                            }
                            let tid =
                                String::from_utf8_lossy(&r.data[team_abs..end]).into_owned();
                            if !tid.is_empty() {
                                team_id = Some(tid);
                            }
                        }
                    }
                }
            }
        } else if magic == CSMAGIC_ENTITLEMENTS {
            // Parse entitlements XML
            let ent_len = u32::from_be_bytes([
                r.data[abs_off + 4],
                r.data[abs_off + 5],
                r.data[abs_off + 6],
                r.data[abs_off + 7],
            ]) as usize;
            let xml_start = abs_off + 8;
            let xml_end = (abs_off + ent_len).min(cs_end);
            if xml_start < xml_end {
                let xml = String::from_utf8_lossy(&r.data[xml_start..xml_end]);
                // Simple key extraction from plist XML
                for line in xml.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("<key>") {
                        if let Some(key) = rest.strip_suffix("</key>") {
                            entitlement_keys.push(key.to_string());
                        }
                    }
                }
            }
        }
    }

    Some(CodeSignature {
        hash_type,
        team_id,
        cd_flags,
        entitlement_keys,
        signed: true,
    })
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

fn cpu_type_str(cpu_type: i32) -> &'static str {
    match cpu_type {
        1 => "VAX",
        6 => "MC680x0",
        7 => "x86 (i386)",
        0x01000007 => "x86_64",
        10 => "MC98000",
        11 => "HPPA",
        12 => "ARM",
        0x0100000C => "ARM64",
        0x0200000C => "ARM64_32",
        13 => "MC88000",
        14 => "SPARC",
        15 => "i860",
        18 => "PowerPC",
        0x01000012 => "PowerPC64",
        _ => "Unknown",
    }
}

fn cpu_subtype_str(cpu_type: i32, cpu_subtype: i32) -> &'static str {
    let sub = cpu_subtype & 0x00FFFFFF; // mask out capability bits
    match cpu_type {
        0x01000007 => match sub {
            // x86_64
            3 => "ALL",
            4 => "Haswell",
            8 => "x86_64h",
            _ => "",
        },
        0x0100000C => match sub {
            // ARM64
            0 => "ALL",
            1 => "ARM64v8",
            2 => "ARM64E",
            _ => "",
        },
        12 => match sub {
            // ARM
            0 => "ALL",
            5 => "ARMv4T",
            6 => "ARMv6",
            7 => "ARMv5TEJ",
            8 => "ARMv6M",
            9 => "ARMv7",
            10 => "ARMv7F",
            11 => "ARMv7S",
            12 => "ARMv7K",
            14 => "ARMv8",
            _ => "",
        },
        7 => match sub {
            // x86
            3 => "ALL",
            _ => "",
        },
        _ => "",
    }
}

fn file_type_str(ft: u32) -> &'static str {
    match ft {
        1 => "MH_OBJECT (relocatable)",
        2 => "MH_EXECUTE (executable)",
        3 => "MH_FVMLIB",
        4 => "MH_CORE (core dump)",
        5 => "MH_PRELOAD",
        6 => "MH_DYLIB (dynamic library)",
        7 => "MH_DYLINKER (dynamic linker)",
        8 => "MH_BUNDLE (bundle/plugin)",
        9 => "MH_DYLIB_STUB",
        10 => "MH_DSYM (debug symbols)",
        11 => "MH_KEXT_BUNDLE (kernel extension)",
        12 => "MH_FILESET",
        _ => "Unknown",
    }
}

fn prot_str(prot: i32) -> String {
    format!(
        "{}{}{}",
        if prot & 1 != 0 { "R" } else { "-" },
        if prot & 2 != 0 { "W" } else { "-" },
        if prot & 4 != 0 { "X" } else { "-" },
    )
}

fn format_version(v: u32) -> String {
    format!("{}.{}.{}", v >> 16, (v >> 8) & 0xFF, v & 0xFF)
}

fn format_source_version(v: u64) -> String {
    let a = (v >> 40) & 0xFFFFFF;
    let b = (v >> 30) & 0x3FF;
    let c = (v >> 20) & 0x3FF;
    let d = (v >> 10) & 0x3FF;
    let e = v & 0x3FF;
    format!("{}.{}.{}.{}.{}", a, b, c, d, e)
}

fn platform_str(p: u32) -> &'static str {
    match p {
        1 => "macOS",
        2 => "iOS",
        3 => "tvOS",
        4 => "watchOS",
        5 => "bridgeOS",
        6 => "Mac Catalyst",
        7 => "iOS Simulator",
        8 => "tvOS Simulator",
        9 => "watchOS Simulator",
        10 => "DriverKit",
        11 => "visionOS",
        12 => "visionOS Simulator",
        _ => "Unknown",
    }
}

fn build_tool_str(t: u32) -> &'static str {
    match t {
        1 => "clang",
        2 => "swift",
        3 => "ld",
        4 => "lld",
        _ => "unknown",
    }
}

fn hex_width(val: u64) -> usize {
    if val > 0xFFFF_FFFF {
        16
    } else {
        8
    }
}

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

fn format_header_flags(flags: u32) -> Vec<&'static str> {
    let mut v = Vec::new();
    if flags & 0x1 != 0 {
        v.push("NOUNDEFS");
    }
    if flags & 0x2 != 0 {
        v.push("INCRLINK");
    }
    if flags & 0x4 != 0 {
        v.push("DYLDLINK");
    }
    if flags & 0x8 != 0 {
        v.push("BINDATLOAD");
    }
    if flags & 0x10 != 0 {
        v.push("PREBOUND");
    }
    if flags & 0x20 != 0 {
        v.push("SPLIT_SEGS");
    }
    if flags & MH_TWOLEVEL != 0 {
        v.push("TWOLEVEL");
    }
    if flags & 0x100 != 0 {
        v.push("FORCE_FLAT");
    }
    if flags & 0x200 != 0 {
        v.push("NOMULTIDEFS");
    }
    if flags & 0x400 != 0 {
        v.push("NOFIXPREBINDING");
    }
    if flags & 0x2000 != 0 {
        v.push("SUBSECTIONS_VIA_SYMBOLS");
    }
    if flags & MH_ALLOW_STACK_EXECUTION != 0 {
        v.push("ALLOW_STACK_EXECUTION");
    }
    if flags & MH_PIE != 0 {
        v.push("PIE");
    }
    if flags & MH_NO_HEAP_EXECUTION != 0 {
        v.push("NO_HEAP_EXECUTION");
    }
    if flags & 0x02000000 != 0 {
        v.push("HAS_TLV_DESCRIPTORS");
    }
    if flags & 0x04000000 != 0 {
        v.push("NO_REEXPORTED_DYLIBS");
    }
    v
}

// ---------------------------------------------------------------------------
// Rendering: Mach-O Header
// ---------------------------------------------------------------------------

fn format_macho_header(header: &MachoHeader, is_64bit: bool, is_le: bool) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       MACH-O HEADER\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str(&format!(
        "  Format:       {}\n",
        if is_64bit { "Mach-O 64-bit" } else { "Mach-O 32-bit" }
    ));
    o.push_str(&format!(
        "  Byte order:   {}\n",
        if is_le { "Little endian" } else { "Big endian" }
    ));

    let cpu_sub = cpu_subtype_str(header.cpu_type, header.cpu_subtype);
    if cpu_sub.is_empty() {
        o.push_str(&format!(
            "  CPU type:     {}\n",
            cpu_type_str(header.cpu_type)
        ));
    } else {
        o.push_str(&format!(
            "  CPU type:     {} ({})\n",
            cpu_type_str(header.cpu_type),
            cpu_sub
        ));
    }

    o.push_str(&format!(
        "  File type:    {}\n",
        file_type_str(header.file_type)
    ));
    o.push_str(&format!("  Load cmds:    {}\n", header.n_cmds));
    o.push_str(&format!(
        "  Cmds size:    {}\n",
        human_size(header.size_of_cmds as u64)
    ));

    let flag_strs = format_header_flags(header.flags);
    if !flag_strs.is_empty() {
        o.push_str(&format!(
            "  Flags:        0x{:08X} ({})\n",
            header.flags,
            flag_strs.join(", ")
        ));
    } else {
        o.push_str(&format!("  Flags:        0x{:08X}\n", header.flags));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Security Features
// ---------------------------------------------------------------------------

fn format_security(
    header: &MachoHeader,
    symbols: &[Symbol],
    code_sig: Option<&CodeSignature>,
    encryption: Option<&EncryptionInfo>,
) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     SECURITY FEATURES\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // PIE
    let pie = header.flags & MH_PIE != 0;
    o.push_str(&format!(
        "  PIE:                {}\n",
        if pie { "Yes" } else { "No" }
    ));

    // Stack canary
    let has_canary = symbols
        .iter()
        .any(|s| s.name == "___stack_chk_fail" || s.name == "___stack_chk_guard");
    o.push_str(&format!(
        "  Stack canary:       {}\n",
        if has_canary { "Yes" } else { "No" }
    ));

    // ARC
    let has_arc = symbols.iter().any(|s| s.name == "_objc_release");
    o.push_str(&format!(
        "  ARC:                {}\n",
        if has_arc { "Yes" } else { "No" }
    ));

    // Code signed
    let signed = code_sig.map(|cs| cs.signed).unwrap_or(false);
    o.push_str(&format!(
        "  Code signed:        {}\n",
        if signed { "Yes" } else { "No" }
    ));

    // Hardened runtime
    if let Some(cs) = code_sig {
        let hardened = cs.cd_flags & CS_RUNTIME != 0;
        o.push_str(&format!(
            "  Hardened runtime:   {}\n",
            if hardened { "Yes" } else { "No" }
        ));

        let lib_validation = cs.cd_flags & CS_REQUIRE_LV != 0;
        o.push_str(&format!(
            "  Library validation: {}\n",
            if lib_validation { "Yes" } else { "No" }
        ));
    }

    // Restrict (NO_HEAP_EXECUTION)
    let restrict = header.flags & MH_NO_HEAP_EXECUTION != 0;
    o.push_str(&format!(
        "  Restrict:           {}\n",
        if restrict { "Yes" } else { "No" }
    ));

    // Stack execution
    let stack_exec = header.flags & MH_ALLOW_STACK_EXECUTION != 0;
    if stack_exec {
        o.push_str("  Stack execution:    ALLOWED (insecure)\n");
    }

    // Encrypted
    if let Some(enc) = encryption {
        let encrypted = enc.crypt_id != 0;
        o.push_str(&format!(
            "  Encrypted:          {}\n",
            if encrypted {
                format!("Yes (id={}, offset=0x{:X}, size={})", enc.crypt_id, enc.crypt_offset, human_size(enc.crypt_size as u64))
            } else {
                "No (decrypted)".to_string()
            }
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Universal Binary
// ---------------------------------------------------------------------------

fn format_fat_archs(archs: &[FatArch]) -> String {
    let mut o = String::new();
    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     UNIVERSAL BINARY\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str(&format!(
        "  {} architecture{}:\n\n",
        archs.len(),
        if archs.len() == 1 { "" } else { "s" }
    ));
    o.push_str("  CPU Type         Offset       Size         Align\n");
    o.push_str("  --------         ------       ----         -----\n");

    for arch in archs {
        let sub = cpu_subtype_str(arch.cpu_type, arch.cpu_subtype);
        let cpu = if sub.is_empty() {
            cpu_type_str(arch.cpu_type).to_string()
        } else {
            format!("{} ({})", cpu_type_str(arch.cpu_type), sub)
        };
        o.push_str(&format!(
            "  {:<18} 0x{:<10X} {:<12} 2^{}\n",
            cpu,
            arch.offset,
            human_size(arch.size),
            arch.align
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Build Info
// ---------------------------------------------------------------------------

fn format_build_info(
    uuid: Option<&str>,
    build_ver: Option<&BuildVersion>,
    version_min: Option<(u32, u32, u32)>,
    source_ver: Option<u64>,
) -> String {
    let mut o = String::new();

    let has_content = uuid.is_some() || build_ver.is_some() || version_min.is_some() || source_ver.is_some();
    if !has_content {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                        BUILD INFO\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    if let Some(uid) = uuid {
        o.push_str(&format!("  UUID:           {}\n", uid));
    }

    if let Some(bv) = build_ver {
        o.push_str(&format!("  Platform:       {}\n", platform_str(bv.platform)));
        o.push_str(&format!(
            "  Min OS:         {}\n",
            format_version(bv.minos)
        ));
        o.push_str(&format!("  SDK:            {}\n", format_version(bv.sdk)));

        for (tool, ver) in &bv.tools {
            o.push_str(&format!(
                "  Build tool:     {} {}\n",
                build_tool_str(*tool),
                format_version(*ver)
            ));
        }
    } else if let Some((plat, ver, sdk)) = version_min {
        o.push_str(&format!("  Platform:       {}\n", platform_str(plat)));
        o.push_str(&format!("  Min OS:         {}\n", format_version(ver)));
        o.push_str(&format!("  SDK:            {}\n", format_version(sdk)));
    }

    if let Some(sv) = source_ver {
        if sv != 0 {
            o.push_str(&format!(
                "  Source version:  {}\n",
                format_source_version(sv)
            ));
        }
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Memory Layout
// ---------------------------------------------------------------------------

const INNER_W: usize = 34;
const OUTER_W: usize = INNER_W + 6; // 40

fn render_memory_map(segments: &[Segment], entry: Option<&EntryPoint>, _is_64bit: bool) -> String {
    let mut o = String::new();

    if segments.is_empty() {
        return o;
    }

    let max_addr = segments
        .iter()
        .map(|s| s.vmaddr.saturating_add(s.vmsize))
        .max()
        .unwrap_or(0);
    let hw = hex_width(max_addr);
    let pad_left = hw + 4;

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       MEMORY LAYOUT\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // Header labels
    o.push_str(&format!(
        "{:>width$}  {:^outer$}  {}\n",
        "File Offset",
        "",
        "Virtual Addr",
        width = pad_left,
        outer = OUTER_W,
    ));

    // Resolve entry point address
    let entry_addr = match entry {
        Some(ep) => match ep.kind {
            EntryKind::UnixThread => Some(ep.value),
            EntryKind::LcMain => {
                // LC_MAIN value is offset from __TEXT segment base
                let text_seg = segments.iter().find(|s| s.name == "__TEXT");
                text_seg.map(|ts| ts.vmaddr.saturating_add(ep.value))
            }
        },
        None => None,
    };

    for (seg_idx, seg) in segments.iter().enumerate() {
        let flags = prot_str(seg.initprot);
        let size_h = human_size(seg.vmsize);

        // Top of segment box
        let blank_l = " ".repeat(pad_left + 2);
        if seg_idx == 0 {
            o.push_str(&format!(
                "{}╔{}╗\n",
                blank_l,
                "═".repeat(OUTER_W - 2)
            ));
        } else {
            o.push_str(&format!(
                "{}╠{}╣\n",
                blank_l,
                "═".repeat(OUTER_W - 2)
            ));
        }

        // Segment label line
        {
            let label = format!("{}  {}", seg.name, flags);
            let remaining = OUTER_W - 4 - label.len() - size_h.len();
            o.push_str(&format!(
                "{}║ {}{}{} ║\n",
                blank_l,
                label,
                " ".repeat(if remaining > 0 { remaining } else { 1 }),
                size_h,
            ));
        }

        if seg.sections.is_empty() {
            let file_off = format!("0x{:0>w$X}", seg.fileoff, w = hw);
            let vaddr = format!("0x{:0>w$X}", seg.vmaddr, w = hw);
            o.push_str(&format!(
                "  {}  ║ {:^inner_w$} ║  {}\n",
                file_off,
                "(no sections)",
                vaddr,
                inner_w = OUTER_W - 4,
            ));
        } else {
            for (i, sec) in seg.sections.iter().enumerate() {
                let sec_file_off = sec.offset as u64;
                let sec_end_vaddr = sec.addr.saturating_add(sec.size);
                let sec_end_file = sec_file_off.saturating_add(sec.size);
                let is_zerofill = (sec.flags & 0xFF) == 1; // S_ZEROFILL

                let file_off_str = if is_zerofill {
                    " ".repeat(hw + 2)
                } else {
                    format!("0x{:0>w$X}", sec_file_off, w = hw)
                };
                let vaddr_str = format!("0x{:0>w$X}", sec.addr, w = hw);

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

                // Section content line
                {
                    let size_str = format_size_commas(sec.size);
                    let name = &sec.sect_name;
                    let avail = INNER_W - 2;
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

                // Entry point marker
                if let Some(ea) = entry_addr {
                    if ea >= sec.addr && ea < sec_end_vaddr {
                        let marker = format!("► entry @ 0x{:X}", ea);
                        let avail = INNER_W - 3;
                        let blank_l = " ".repeat(pad_left + 2);
                        o.push_str(&format!(
                            "{}║ │  {:<w$} │ ║\n",
                            blank_l,
                            marker,
                            w = avail,
                        ));
                    }
                }

                // Bottom border for last section
                if i == seg.sections.len() - 1 {
                    let file_end_str = if is_zerofill {
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

        // Bottom border of last segment
        if seg_idx == segments.len() - 1 {
            let blank_l = " ".repeat(pad_left + 2);
            o.push_str(&format!(
                "{}╚{}╝\n",
                blank_l,
                "═".repeat(OUTER_W - 2)
            ));
        }
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Segments & Sections Table
// ---------------------------------------------------------------------------

fn format_segments_table(segments: &[Segment], is_64bit: bool, data: &[u8]) -> String {
    let mut o = String::new();
    if segments.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════════════════\n");
    o.push_str("                         SEGMENTS & SECTIONS\n");
    o.push_str("═══════════════════════════════════════════════════════════════════════════\n\n");

    if is_64bit {
        o.push_str("  Name                 Address          Offset   Size     Entropy Prot\n");
        o.push_str("  ----                 -------          ------   ----     ------- ----\n");
    } else {
        o.push_str("  Name                 Address  Offset   Size     Entropy Prot\n");
        o.push_str("  ----                 -------  ------   ----     ------- ----\n");
    }

    for seg in segments {
        let prot = format!("{}/{}", prot_str(seg.initprot), prot_str(seg.maxprot));
        if is_64bit {
            o.push_str(&format!(
                "  {:<20} {:016X} {:08X} {:08X}         {}\n",
                seg.name, seg.vmaddr, seg.fileoff, seg.filesize as u32, prot,
            ));
        } else {
            o.push_str(&format!(
                "  {:<20} {:08X} {:08X} {:08X}         {}\n",
                seg.name, seg.vmaddr as u32, seg.fileoff as u32, seg.filesize as u32, prot,
            ));
        }

        for sec in &seg.sections {
            let is_zerofill = (sec.flags & 0xFF) == 1;
            let entropy = if !is_zerofill && sec.size > 0 {
                let start = sec.offset as usize;
                let end = (start + sec.size as usize).min(data.len());
                if start < data.len() && end > start {
                    format!("{:.2}", shannon_entropy(&data[start..end]))
                } else {
                    "  -  ".into()
                }
            } else {
                "  -  ".into()
            };

            if is_64bit {
                o.push_str(&format!(
                    "    {:<18} {:016X} {:08X} {:08X} {:>5}\n",
                    sec.sect_name, sec.addr, sec.offset, sec.size as u32, entropy,
                ));
            } else {
                o.push_str(&format!(
                    "    {:<18} {:08X} {:08X} {:08X} {:>5}\n",
                    sec.sect_name, sec.addr as u32, sec.offset, sec.size as u32, entropy,
                ));
            }
        }
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Linked Libraries
// ---------------------------------------------------------------------------

fn format_linked_libs(libs: &[LinkedLib]) -> String {
    let mut o = String::new();
    let display_libs: Vec<&LinkedLib> = libs
        .iter()
        .filter(|l| !matches!(l.kind, LibKind::Id))
        .collect();
    if display_libs.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                     LINKED LIBRARIES\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // Show ID dylib if present
    if let Some(id) = libs.iter().find(|l| matches!(l.kind, LibKind::Id)) {
        o.push_str(&format!("  ID:  {} ({})\n\n", id.name, format_version(id.current_version)));
    }

    o.push_str(&format!("  {} linked libraries:\n\n", display_libs.len()));

    for lib in &display_libs {
        let tag = match lib.kind {
            LibKind::Required => "",
            LibKind::Weak => " [weak]",
            LibKind::Reexport => " [re-export]",
            LibKind::Lazy => " [lazy]",
            LibKind::Upward => " [upward]",
            LibKind::Id => "",
        };
        o.push_str(&format!(
            "    {} (compat {}, current {}){}\n",
            lib.name,
            format_version(lib.compat_version),
            format_version(lib.current_version),
            tag,
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: RPaths
// ---------------------------------------------------------------------------

fn format_rpaths(rpaths: &[String]) -> String {
    let mut o = String::new();
    if rpaths.is_empty() {
        return o;
    }

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                          RPATHS\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    for rp in rpaths {
        o.push_str(&format!("    {}\n", rp));
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
        "  {:>w$}  {:<8} {:<8} Name\n",
        "Value",
        "Type",
        "Scope",
        w = vw + 2,
    ));
    o.push_str(&format!(
        "  {:>w$}  {:<8} {:<8} ----\n",
        "-----",
        "----",
        "-----",
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

        let type_str = if sym.n_type & 0x0E == 0x0E {
            // N_SECT — defined in section
            "SECT"
        } else if sym.n_type & 0x0E == 0x0A {
            // N_INDR — indirect
            "INDR"
        } else if sym.n_type & 0x0E == 0x02 {
            // N_ABS — absolute
            "ABS"
        } else if sym.n_type & 0x01 != 0 {
            // N_EXT and undefined
            "UNDEF"
        } else {
            "OTHER"
        };

        let scope = if sym.n_type & 0x01 != 0 {
            "external"
        } else if sym.n_type & 0x0E == 0x0E {
            "local"
        } else {
            ""
        };

        o.push_str(&format!(
            "  0x{:0>w$X}  {:<8} {:<8} {}\n",
            sym.value, type_str, scope, name, w = vw,
        ));
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Code Signature
// ---------------------------------------------------------------------------

fn format_code_signature(cs: &CodeSignature) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                      CODE SIGNATURE\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    o.push_str(&format!(
        "  Signed:             {}\n",
        if cs.signed { "Yes" } else { "No" }
    ));

    if let Some(ref ht) = cs.hash_type {
        o.push_str(&format!("  Hash type:          {}\n", ht));
    }

    if cs.cd_flags != 0 {
        let mut flag_names = Vec::new();
        if cs.cd_flags & 0x0001 != 0 {
            flag_names.push("VALID");
        }
        if cs.cd_flags & 0x0002 != 0 {
            flag_names.push("ADHOC");
        }
        if cs.cd_flags & 0x0004 != 0 {
            flag_names.push("FORCED_LV");
        }
        if cs.cd_flags & 0x0008 != 0 {
            flag_names.push("INSTALLER");
        }
        if cs.cd_flags & 0x0200 != 0 {
            flag_names.push("HARD");
        }
        if cs.cd_flags & 0x0400 != 0 {
            flag_names.push("KILL");
        }
        if cs.cd_flags & CS_REQUIRE_LV != 0 {
            flag_names.push("REQUIRE_LV");
        }
        if cs.cd_flags & CS_RUNTIME != 0 {
            flag_names.push("RUNTIME");
        }
        if cs.cd_flags & 0x00020000 != 0 {
            flag_names.push("LINKER_SIGNED");
        }
        o.push_str(&format!(
            "  Flags:              0x{:X} ({})\n",
            cs.cd_flags,
            flag_names.join(", ")
        ));
    }

    if let Some(ref tid) = cs.team_id {
        o.push_str(&format!("  Team ID:            {}\n", tid));
    }

    if !cs.entitlement_keys.is_empty() {
        o.push_str(&format!(
            "\n  Entitlements ({}):\n",
            cs.entitlement_keys.len()
        ));
        for key in &cs.entitlement_keys {
            o.push_str(&format!("    {}\n", key));
        }
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Rendering: Entry Point
// ---------------------------------------------------------------------------

fn format_entry_point(ep: &EntryPoint, segments: &[Segment]) -> String {
    let mut o = String::new();

    o.push_str("═══════════════════════════════════════════════════════════════\n");
    o.push_str("                       ENTRY POINT\n");
    o.push_str("═══════════════════════════════════════════════════════════════\n\n");

    match ep.kind {
        EntryKind::LcMain => {
            o.push_str(&format!("  LC_MAIN offset:     0x{:X}\n", ep.value));
            // Resolve to virtual address
            if let Some(text) = segments.iter().find(|s| s.name == "__TEXT") {
                let addr = text.vmaddr.saturating_add(ep.value);
                o.push_str(&format!("  Virtual address:    0x{:X}\n", addr));
            }
        }
        EntryKind::UnixThread => {
            o.push_str(&format!("  LC_UNIXTHREAD PC:   0x{:X}\n", ep.value));
        }
    }

    o.push('\n');
    o
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a Mach-O file and return formatted output
pub fn parse_macho(path: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|e| e.to_string())?;
    parse_macho_bytes(&data)
}

/// Parse Mach-O data from bytes
fn parse_macho_bytes(data: &[u8]) -> Result<String, String> {
    if data.len() < 4 {
        return Err("File too small".into());
    }

    let magic_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
    let magic_be = u32::from_be_bytes(magic_bytes);

    // Check for fat/universal binary
    let (slice, fat_archs) = if magic_be == FAT_MAGIC || magic_be == FAT_CIGAM {
        let archs = parse_fat_header(data).ok_or("Invalid fat header")?;
        if archs.is_empty() {
            return Err("Empty universal binary".into());
        }
        // Use the first architecture slice
        let first = &archs[0];
        let start = first.offset as usize;
        let end = start + first.size as usize;
        if end > data.len() {
            return Err("Fat arch slice extends beyond file".into());
        }
        (&data[start..end], Some(archs))
    } else {
        (data, None)
    };

    if slice.len() < 4 {
        return Err("Mach-O slice too small".into());
    }

    let slice_magic: [u8; 4] = [slice[0], slice[1], slice[2], slice[3]];
    let (is_le, is_64bit) =
        detect_endianness_and_bits(&slice_magic).ok_or("Not a valid Mach-O file")?;

    let min_header = if is_64bit { 32 } else { 28 };
    if slice.len() < min_header {
        return Err("File too small for Mach-O header".into());
    }

    let r = MachoReader {
        data: slice,
        is_le,
        is_64bit,
    };

    let header = parse_header(&r);
    let segments = parse_segments(&r, &header);
    let dylibs = parse_dylibs(&r, &header);
    let rpaths = parse_rpaths(&r, &header);
    let uuid = parse_uuid(&r, &header);
    let build_ver = parse_build_version(&r, &header);
    let version_min = parse_version_min(&r, &header);
    let source_ver = parse_source_version(&r, &header);
    let entry = parse_entry_point(&r, &header);
    let encryption = parse_encryption_info(&r, &header);
    let symbols = parse_symbols(&r, &header);
    let code_sig = parse_code_signature(&r, &header);

    let mut output = String::new();

    // 1. Mach-O Header
    output.push_str(&format_macho_header(&header, is_64bit, is_le));

    // 2. Security Features
    output.push_str(&format_security(
        &header,
        &symbols,
        code_sig.as_ref(),
        encryption.as_ref(),
    ));

    // 3. Universal Binary (conditional)
    if let Some(ref archs) = fat_archs {
        output.push_str(&format_fat_archs(archs));
    }

    // 4. Build Info
    output.push_str(&format_build_info(
        uuid.as_deref(),
        build_ver.as_ref(),
        version_min,
        source_ver,
    ));

    // 5. Memory Layout
    output.push_str(&render_memory_map(&segments, entry.as_ref(), is_64bit));

    // 6. Segments & Sections Table
    output.push_str(&format_segments_table(&segments, is_64bit, slice));

    // 7. Linked Libraries
    output.push_str(&format_linked_libs(&dylibs));

    // 8. RPaths
    output.push_str(&format_rpaths(&rpaths));

    // 9. Symbol Table
    output.push_str(&format_symbol_table(&symbols, is_64bit));

    // 10. Code Signature
    if let Some(ref cs) = code_sig {
        output.push_str(&format_code_signature(cs));
    }

    // 11. Entry Point
    if let Some(ref ep) = entry {
        output.push_str(&format_entry_point(ep, &segments));
    }

    Ok(output)
}
