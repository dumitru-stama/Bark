//! ELF file parsing module

use std::fs::File;
use std::io::Read;

/// ELF magic bytes
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

/// Check if the given bytes are ELF magic
pub fn is_elf(magic: &[u8]) -> bool {
    magic.len() >= 4 && magic[..4] == ELF_MAGIC
}

/// Parse an ELF file and return formatted output
pub fn parse_elf(path: &str) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|e| e.to_string())?;

    parse_elf_bytes(&data)
}

/// Parse ELF data from bytes
pub fn parse_elf_bytes(data: &[u8]) -> Result<String, String> {
    if data.len() < 64 {
        return Err("File too small for ELF header".to_string());
    }

    // Check magic
    if !is_elf(data) {
        return Err("Not an ELF file".to_string());
    }

    let mut output = String::new();
    output.push_str("═══════════════════════════════════════════════════════════════\n");
    output.push_str("                         ELF HEADER\n");
    output.push_str("═══════════════════════════════════════════════════════════════\n\n");

    // ELF class (32-bit or 64-bit)
    let class = data[4];
    let is_64bit = class == 2;
    output.push_str(&format!(
        "  Class:                             {}\n",
        match class {
            1 => "ELF32 (32-bit)",
            2 => "ELF64 (64-bit)",
            _ => "Unknown",
        }
    ));

    // Data encoding (endianness)
    let encoding = data[5];
    let is_le = encoding == 1;
    output.push_str(&format!(
        "  Data:                              {}\n",
        match encoding {
            1 => "2's complement, little endian",
            2 => "2's complement, big endian",
            _ => "Unknown",
        }
    ));

    // ELF version
    output.push_str(&format!(
        "  Version:                           {} (current)\n",
        data[6]
    ));

    // OS/ABI
    output.push_str(&format!(
        "  OS/ABI:                            {}\n",
        match data[7] {
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

    // ABI version
    output.push_str(&format!("  ABI Version:                       {}\n", data[8]));

    output.push('\n');

    // Read 16-bit and 32/64-bit values based on endianness
    let read_u16 = |offset: usize| -> u16 {
        if is_le {
            u16::from_le_bytes([data[offset], data[offset + 1]])
        } else {
            u16::from_be_bytes([data[offset], data[offset + 1]])
        }
    };

    let read_u32 = |offset: usize| -> u32 {
        if is_le {
            u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ])
        } else {
            u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ])
        }
    };

    let read_u64 = |offset: usize| -> u64 {
        if is_le {
            u64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ])
        } else {
            u64::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ])
        }
    };

    // Type
    let e_type = read_u16(16);
    output.push_str(&format!(
        "  Type:                              {}\n",
        match e_type {
            0 => "NONE (No file type)",
            1 => "REL (Relocatable file)",
            2 => "EXEC (Executable file)",
            3 => "DYN (Shared object file)",
            4 => "CORE (Core file)",
            _ => "Unknown",
        }
    ));

    // Machine
    let e_machine = read_u16(18);
    output.push_str(&format!(
        "  Machine:                           {}\n",
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

    // Version
    let e_version = read_u32(20);
    output.push_str(&format!(
        "  Version:                           0x{:x}\n",
        e_version
    ));

    // Entry point, program header offset, section header offset
    if is_64bit {
        let e_entry = read_u64(24);
        let e_phoff = read_u64(32);
        let e_shoff = read_u64(40);
        let e_flags = read_u32(48);
        let e_ehsize = read_u16(52);
        let e_phentsize = read_u16(54);
        let e_phnum = read_u16(56);
        let e_shentsize = read_u16(58);
        let e_shnum = read_u16(60);
        let e_shstrndx = read_u16(62);

        output.push_str(&format!(
            "  Entry point address:               0x{:x}\n",
            e_entry
        ));
        output.push_str(&format!(
            "  Start of program headers:          {} (bytes into file)\n",
            e_phoff
        ));
        output.push_str(&format!(
            "  Start of section headers:          {} (bytes into file)\n",
            e_shoff
        ));
        output.push_str(&format!("  Flags:                             0x{:x}\n", e_flags));
        output.push_str(&format!(
            "  Size of this header:               {} (bytes)\n",
            e_ehsize
        ));
        output.push_str(&format!(
            "  Size of program headers:           {} (bytes)\n",
            e_phentsize
        ));
        output.push_str(&format!(
            "  Number of program headers:         {}\n",
            e_phnum
        ));
        output.push_str(&format!(
            "  Size of section headers:           {} (bytes)\n",
            e_shentsize
        ));
        output.push_str(&format!(
            "  Number of section headers:         {}\n",
            e_shnum
        ));
        output.push_str(&format!(
            "  Section header string table index: {}\n",
            e_shstrndx
        ));

        // Program headers
        if e_phnum > 0 && e_phoff > 0 {
            output.push_str(
                "\n═══════════════════════════════════════════════════════════════\n",
            );
            output.push_str("                       PROGRAM HEADERS\n");
            output.push_str(
                "═══════════════════════════════════════════════════════════════\n\n",
            );
            output.push_str(
                "  Type           Offset             VirtAddr           PhysAddr\n",
            );
            output.push_str(
                "                 FileSiz            MemSiz              Flags  Align\n",
            );

            for i in 0..e_phnum.min(20) as usize {
                let offset = e_phoff as usize + i * e_phentsize as usize;
                if offset + 56 > data.len() {
                    break;
                }

                let p_type = read_u32(offset);
                let p_flags = read_u32(offset + 4);
                let p_offset = read_u64(offset + 8);
                let p_vaddr = read_u64(offset + 16);
                let p_paddr = read_u64(offset + 24);
                let p_filesz = read_u64(offset + 32);
                let p_memsz = read_u64(offset + 40);
                let p_align = read_u64(offset + 48);

                let type_str = match p_type {
                    0 => "NULL",
                    1 => "LOAD",
                    2 => "DYNAMIC",
                    3 => "INTERP",
                    4 => "NOTE",
                    6 => "PHDR",
                    7 => "TLS",
                    0x6474e550 => "GNU_EH_FRAME",
                    0x6474e551 => "GNU_STACK",
                    0x6474e552 => "GNU_RELRO",
                    0x6474e553 => "GNU_PROPERTY",
                    _ => "UNKNOWN",
                };

                let flags_str = format!(
                    "{}{}{}",
                    if p_flags & 4 != 0 { "R" } else { " " },
                    if p_flags & 2 != 0 { "W" } else { " " },
                    if p_flags & 1 != 0 { "E" } else { " " },
                );

                output.push_str(&format!(
                    "  {:14} 0x{:016x} 0x{:016x} 0x{:016x}\n",
                    type_str, p_offset, p_vaddr, p_paddr
                ));
                output.push_str(&format!(
                    "                 0x{:016x} 0x{:016x}  {}    0x{:x}\n",
                    p_filesz, p_memsz, flags_str, p_align
                ));
            }
        }
    } else {
        // 32-bit ELF
        let e_entry = read_u32(24);
        let e_phoff = read_u32(28);
        let e_shoff = read_u32(32);
        let e_flags = read_u32(36);
        let e_ehsize = read_u16(40);
        let e_phentsize = read_u16(42);
        let e_phnum = read_u16(44);
        let e_shentsize = read_u16(46);
        let e_shnum = read_u16(48);
        let e_shstrndx = read_u16(50);

        output.push_str(&format!(
            "  Entry point address:               0x{:x}\n",
            e_entry
        ));
        output.push_str(&format!(
            "  Start of program headers:          {} (bytes into file)\n",
            e_phoff
        ));
        output.push_str(&format!(
            "  Start of section headers:          {} (bytes into file)\n",
            e_shoff
        ));
        output.push_str(&format!("  Flags:                             0x{:x}\n", e_flags));
        output.push_str(&format!(
            "  Size of this header:               {} (bytes)\n",
            e_ehsize
        ));
        output.push_str(&format!(
            "  Size of program headers:           {} (bytes)\n",
            e_phentsize
        ));
        output.push_str(&format!(
            "  Number of program headers:         {}\n",
            e_phnum
        ));
        output.push_str(&format!(
            "  Size of section headers:           {} (bytes)\n",
            e_shentsize
        ));
        output.push_str(&format!(
            "  Number of section headers:         {}\n",
            e_shnum
        ));
        output.push_str(&format!(
            "  Section header string table index: {}\n",
            e_shstrndx
        ));
    }

    output.push_str("\n═══════════════════════════════════════════════════════════════\n");

    Ok(output)
}
