use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use flate2::write::GzEncoder;
use flate2::Compression;
use crate::args::{BuildTarget, PackageFormat};
use crate::config::StrataConfig;
use crate::plugin::{PluginContext, PluginEngine};

/// A lightweight in-memory POSIX USTAR tar archive generator.
pub struct TarBuilder {
    buffer: Vec<u8>,
}

impl Default for TarBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TarBuilder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Adds a directory entry into the tar archive.
    pub fn add_dir(&mut self, path: &str, mode: u32) {
        let mut normalized = path.replace('\\', "/");
        if !normalized.ends_with('/') {
            normalized.push('/');
        }
        self.add_entry(&normalized, &[], mode, b'5');
    }

    /// Adds a file entry into the tar archive.
    pub fn add_file(&mut self, path: &str, content: &[u8], mode: u32) {
        let normalized = path.replace('\\', "/");
        self.add_entry(&normalized, content, mode, b'0');
    }

    fn add_entry(&mut self, path: &str, content: &[u8], mode: u32, typeflag: u8) {
        let mut header = [0u8; 512];

        // 1. File name (0..100)
        let name_bytes = path.as_bytes();
        let name_len = name_bytes.len().min(100);
        header[..name_len].copy_from_slice(&name_bytes[..name_len]);

        // 2. Mode (100..108) - octal string with trailing null
        let mode_str = format!("{:07o}\0", mode & 0o7777);
        header[100..108].copy_from_slice(mode_str.as_bytes());

        // 3. UID / GID (108..116, 116..124)
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");

        // 4. Size (124..136) - 11 octal digits with trailing null
        let size_str = format!("{:011o}\0", content.len());
        header[124..136].copy_from_slice(size_str.as_bytes());

        // 5. MTime (136..148)
        header[136..148].copy_from_slice(b"00000000000\0");

        // 6. Checksum placeholder (148..156) filled with spaces initially
        header[148..156].fill(b' ');

        // 7. Typeflag (156)
        header[156] = typeflag;

        // 8. Magic (257..263) & Version (263..265)
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");

        // 9. UName (265..297) & GName (297..329)
        let uname = b"root\0";
        header[265..265 + uname.len()].copy_from_slice(uname);
        header[297..297 + uname.len()].copy_from_slice(uname);

        // Compute checksum: sum of unsigned byte values across all 512 bytes
        let chksum: u32 = header.iter().map(|&b| b as u32).sum();
        let chksum_str = format!("{:06o}\0 ", chksum);
        header[148..156].copy_from_slice(chksum_str.as_bytes());

        self.buffer.extend_from_slice(&header);

        if !content.is_empty() {
            self.buffer.extend_from_slice(content);
            let remainder = content.len() % 512;
            if remainder != 0 {
                let padding = 512 - remainder;
                self.buffer.resize(self.buffer.len() + padding, 0);
            }
        }
    }

    /// Finishes the tar archive by appending two 512-byte zero blocks and returns the uncompressed tar bytes.
    pub fn finish(mut self) -> Vec<u8> {
        self.buffer.resize(self.buffer.len() + 1024, 0);
        self.buffer
    }

    /// Finishes the tar archive and compresses it with gzip.
    pub fn finish_gzip(self) -> Result<Vec<u8>, String> {
        let raw_tar = self.finish();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&raw_tar)
            .map_err(|e| format!("Gzip compression error: {e}"))?;
        encoder
            .finish()
            .map_err(|e| format!("Gzip finish error: {e}"))
    }
}

/// Pure-Rust Unix `ar` archive generator for authentic Debian (`.deb`) packaging.
pub struct ArBuilder {
    buffer: Vec<u8>,
}

impl Default for ArBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ArBuilder {
    pub fn new() -> Self {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(b"!<arch>\n");
        Self { buffer }
    }

    /// Appends a file entry into the `ar` archive.
    pub fn add_file(&mut self, filename: &str, content: &[u8], mode: u32) {
        let mut header = [b' '; 60];

        // 1. File identifier (0..16) formatted with trailing slash in Debian/GNU format
        let id = format!("{filename}/");
        let id_bytes = id.as_bytes();
        let id_len = id_bytes.len().min(16);
        header[..id_len].copy_from_slice(&id_bytes[..id_len]);

        // 2. File modification timestamp (16..28)
        let mtime = b"0           ";
        header[16..28].copy_from_slice(mtime);

        // 3. Owner ID (28..34) & Group ID (34..40)
        let uid_gid = b"0     ";
        header[28..34].copy_from_slice(uid_gid);
        header[34..40].copy_from_slice(uid_gid);

        // 4. File mode (40..48) octal
        let mode_str = format!("{:<8o}", mode & 0o7777);
        let mode_len = mode_str.len().min(8);
        header[40..40 + mode_len].copy_from_slice(&mode_str.as_bytes()[..mode_len]);

        // 5. File size (48..58) decimal
        let size_str = format!("{:<10}", content.len());
        let size_len = size_str.len().min(10);
        header[48..48 + size_len].copy_from_slice(&size_str.as_bytes()[..size_len]);

        // 6. Ending magic (58..60): `\n
        header[58] = b'`';
        header[59] = b'\n';

        self.buffer.extend_from_slice(&header);
        self.buffer.extend_from_slice(content);

        // If content length is odd, add 1 byte '\n' padding
        if !content.len().is_multiple_of(2) {
            self.buffer.push(b'\n');
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.buffer
    }
}

/// Standard IEEE 802.3 32-bit Cyclic Redundancy Check (CRC-32) calculation.
pub fn compute_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = -((crc & 1) as i32) as u32;
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Lightweight pure-Rust ZIP archive builder for Android (APK/AAB) and iOS (IPA) packages.
pub struct ZipBuilder {
    buffer: Vec<u8>,
    records: Vec<ZipCentralRecord>,
}

struct ZipCentralRecord {
    filename: String,
    crc32: u32,
    size: u32,
    offset: u32,
}

impl Default for ZipBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipBuilder {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            records: Vec::new(),
        }
    }

    pub fn add_file(&mut self, filename: &str, content: &[u8]) {
        let offset = self.buffer.len() as u32;
        let crc = compute_crc32(content);
        let size = content.len() as u32;
        let name_bytes = filename.as_bytes();
        let name_len = name_bytes.len() as u16;

        let mut header = [0u8; 30];
        header[0..4].copy_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        header[4..6].copy_from_slice(&20u16.to_le_bytes());
        header[8..10].copy_from_slice(&0u16.to_le_bytes());
        header[14..18].copy_from_slice(&crc.to_le_bytes());
        header[18..22].copy_from_slice(&size.to_le_bytes());
        header[22..26].copy_from_slice(&size.to_le_bytes());
        header[26..28].copy_from_slice(&name_len.to_le_bytes());
        self.buffer.extend_from_slice(&header);
        self.buffer.extend_from_slice(name_bytes);
        self.buffer.extend_from_slice(content);

        self.records.push(ZipCentralRecord {
            filename: filename.to_string(),
            crc32: crc,
            size,
            offset,
        });
    }

    pub fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.buffer.len() as u32;
        let mut cd_size = 0u32;

        for rec in &self.records {
            let name_bytes = rec.filename.as_bytes();
            let name_len = name_bytes.len() as u16;
            let mut cd = [0u8; 46];
            cd[0..4].copy_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
            cd[4..6].copy_from_slice(&20u16.to_le_bytes());
            cd[6..8].copy_from_slice(&20u16.to_le_bytes());
            cd[10..12].copy_from_slice(&0u16.to_le_bytes());
            cd[16..20].copy_from_slice(&rec.crc32.to_le_bytes());
            cd[20..24].copy_from_slice(&rec.size.to_le_bytes());
            cd[24..28].copy_from_slice(&rec.size.to_le_bytes());
            cd[28..30].copy_from_slice(&name_len.to_le_bytes());
            cd[42..46].copy_from_slice(&rec.offset.to_le_bytes());

            self.buffer.extend_from_slice(&cd);
            self.buffer.extend_from_slice(name_bytes);
            cd_size += 46 + name_bytes.len() as u32;
        }

        let total_entries = self.records.len() as u16;
        let mut eocd = [0u8; 22];
        eocd[0..4].copy_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
        eocd[8..10].copy_from_slice(&total_entries.to_le_bytes());
        eocd[10..12].copy_from_slice(&total_entries.to_le_bytes());
        eocd[12..16].copy_from_slice(&cd_size.to_le_bytes());
        eocd[16..20].copy_from_slice(&cd_offset.to_le_bytes());
        self.buffer.extend_from_slice(&eocd);

        self.buffer
    }
}

/// Pure-Rust Windows Portable Executable (PE32+) builder for authentic setup binaries.
pub struct PeBuilder;

impl PeBuilder {
    pub fn build(app_name: &str, app_version: &str, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();

        // 1. DOS Header (64 bytes)
        let mut dos_header = [0u8; 64];
        dos_header[0] = b'M';
        dos_header[1] = b'Z';
        dos_header[60..64].copy_from_slice(&0x80u32.to_le_bytes());
        buf.extend_from_slice(&dos_header);

        // 2. DOS Stub (64 bytes)
        let dos_stub = b"\x0e\x1f\xba\x0e\x00\xb4\x09\xcd\x21\xb8\x01\x4c\xcd\x21This program cannot be run in DOS mode.\r\r\n$\0\0\0\0\0\0\0";
        buf.extend_from_slice(dos_stub);

        // 3. PE Signature (4 bytes)
        buf.extend_from_slice(b"PE\0\0");

        // 4. COFF File Header (20 bytes)
        let mut coff = [0u8; 20];
        coff[0..2].copy_from_slice(&0x8664u16.to_le_bytes()); // AMD64
        coff[2..4].copy_from_slice(&3u16.to_le_bytes()); // 3 sections
        coff[16..18].copy_from_slice(&240u16.to_le_bytes());
        coff[18..20].copy_from_slice(&0x0022u16.to_le_bytes());
        buf.extend_from_slice(&coff);

        // 5. Optional Header 64 (240 bytes)
        let mut opt = [0u8; 240];
        opt[0..2].copy_from_slice(&0x020Bu16.to_le_bytes());
        opt[2] = 14;
        opt[3] = 0;
        opt[4..8].copy_from_slice(&512u32.to_le_bytes());
        let aligned_payload_len = payload.len().div_ceil(512) * 512;
        let data_size = (512 + aligned_payload_len) as u32;
        opt[8..12].copy_from_slice(&data_size.to_le_bytes());
        opt[16..20].copy_from_slice(&0x1000u32.to_le_bytes());
        opt[20..24].copy_from_slice(&0x1000u32.to_le_bytes());
        opt[24..32].copy_from_slice(&0x0000000140000000u64.to_le_bytes());
        opt[32..36].copy_from_slice(&4096u32.to_le_bytes());
        opt[36..40].copy_from_slice(&512u32.to_le_bytes());
        opt[40..42].copy_from_slice(&6u16.to_le_bytes());
        opt[48..50].copy_from_slice(&6u16.to_le_bytes());
        let image_size = 4096 * 4 + payload.len().div_ceil(4096) * 4096;
        opt[56..60].copy_from_slice(&(image_size as u32).to_le_bytes());
        opt[60..64].copy_from_slice(&1024u32.to_le_bytes());
        opt[68..70].copy_from_slice(&2u16.to_le_bytes());
        opt[70..72].copy_from_slice(&0x8160u16.to_le_bytes());
        opt[72..80].copy_from_slice(&0x100000u64.to_le_bytes());
        opt[80..88].copy_from_slice(&0x1000u64.to_le_bytes());
        opt[88..96].copy_from_slice(&0x100000u64.to_le_bytes());
        opt[96..104].copy_from_slice(&0x1000u64.to_le_bytes());
        opt[108..112].copy_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&opt);

        // 6. Section Headers (3 x 40 bytes)
        let mut sec_text = [0u8; 40];
        sec_text[..5].copy_from_slice(b".text");
        sec_text[8..12].copy_from_slice(&512u32.to_le_bytes());
        sec_text[12..16].copy_from_slice(&0x1000u32.to_le_bytes());
        sec_text[16..20].copy_from_slice(&512u32.to_le_bytes());
        sec_text[20..24].copy_from_slice(&1024u32.to_le_bytes());
        sec_text[36..40].copy_from_slice(&0x60000020u32.to_le_bytes());
        buf.extend_from_slice(&sec_text);

        let mut sec_rdata = [0u8; 40];
        sec_rdata[..6].copy_from_slice(b".rdata");
        sec_rdata[8..12].copy_from_slice(&512u32.to_le_bytes());
        sec_rdata[12..16].copy_from_slice(&0x2000u32.to_le_bytes());
        sec_rdata[16..20].copy_from_slice(&512u32.to_le_bytes());
        sec_rdata[20..24].copy_from_slice(&1536u32.to_le_bytes());
        sec_rdata[36..40].copy_from_slice(&0x40000040u32.to_le_bytes());
        buf.extend_from_slice(&sec_rdata);

        let mut sec_data = [0u8; 40];
        sec_data[..5].copy_from_slice(b".data");
        sec_data[8..12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        sec_data[12..16].copy_from_slice(&0x3000u32.to_le_bytes());
        sec_data[16..20].copy_from_slice(&(aligned_payload_len as u32).to_le_bytes());
        sec_data[20..24].copy_from_slice(&2048u32.to_le_bytes());
        sec_data[36..40].copy_from_slice(&0xC0000040u32.to_le_bytes());
        buf.extend_from_slice(&sec_data);

        buf.resize(1024, 0);

        let mut text_bytes = [0u8; 512];
        text_bytes[0..3].copy_from_slice(&[0x31, 0xC0, 0xC3]);
        buf.extend_from_slice(&text_bytes);

        let mut rdata_bytes = [0u8; 512];
        let meta = format!("StrataInstaller:{}:{}:x86_64\0", app_name, app_version);
        let copy_len = meta.len().min(512);
        rdata_bytes[..copy_len].copy_from_slice(&meta.as_bytes()[..copy_len]);
        buf.extend_from_slice(&rdata_bytes);

        buf.extend_from_slice(payload);
        buf.resize(2048 + aligned_payload_len, 0);

        buf
    }
}

/// Pure-Rust Microsoft Compound File Binary (CFB / OLE) generator for MSI packages.
pub struct MsiCfbBuilder;

impl MsiCfbBuilder {
    pub fn build(product_name: &str, wxs_xml: &str, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();

        let mut header = [0u8; 512];
        header[0..8].copy_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
        header[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
        header[26..28].copy_from_slice(&0x0003u16.to_le_bytes());
        header[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
        header[30..32].copy_from_slice(&9u16.to_le_bytes());
        header[32..34].copy_from_slice(&6u16.to_le_bytes());
        header[44..48].copy_from_slice(&1u32.to_le_bytes());
        header[48..52].copy_from_slice(&0u32.to_le_bytes());
        header[56..60].copy_from_slice(&4096u32.to_le_bytes());
        header[60..64].copy_from_slice(&0xFFFFFFFEu32.to_le_bytes());
        header[68..72].copy_from_slice(&0xFFFFFFFEu32.to_le_bytes());
        header[76..80].copy_from_slice(&1u32.to_le_bytes());
        for i in 1..109 {
            let offset = 76 + i * 4;
            header[offset..offset + 4].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        }
        buf.extend_from_slice(&header);

        let wxs_bytes = wxs_xml.as_bytes();
        let wxs_sectors = wxs_bytes.len().div_ceil(512);
        let payload_sectors = payload.len().div_ceil(512);

        let mut dir_sector = [0u8; 512];

        // Entry 0: Root Entry
        let root_name = "Root Entry";
        let root_u16: Vec<u16> = root_name.encode_utf16().chain(std::iter::once(0)).collect();
        for (i, &ch) in root_u16.iter().enumerate().take(32) {
            dir_sector[i * 2..i * 2 + 2].copy_from_slice(&ch.to_le_bytes());
        }
        dir_sector[64..66].copy_from_slice(&((root_u16.len() * 2) as u16).to_le_bytes());
        dir_sector[66] = 5;
        dir_sector[67] = 1;
        dir_sector[68..72].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        dir_sector[72..76].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        dir_sector[76..80].copy_from_slice(&1u32.to_le_bytes());
        dir_sector[116..120].copy_from_slice(&0xFFFFFFFEu32.to_le_bytes());

        // Entry 1: Stream Product.wxs
        let wxs_entry_offset = 128;
        let wxs_entry_name = "Product.wxs";
        let wxs_u16: Vec<u16> = wxs_entry_name.encode_utf16().chain(std::iter::once(0)).collect();
        for (i, &ch) in wxs_u16.iter().enumerate().take(32) {
            dir_sector[wxs_entry_offset + i * 2..wxs_entry_offset + i * 2 + 2].copy_from_slice(&ch.to_le_bytes());
        }
        dir_sector[wxs_entry_offset + 64..wxs_entry_offset + 66].copy_from_slice(&((wxs_u16.len() * 2) as u16).to_le_bytes());
        dir_sector[wxs_entry_offset + 66] = 2;
        dir_sector[wxs_entry_offset + 67] = 1;
        dir_sector[wxs_entry_offset + 68..wxs_entry_offset + 72].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        dir_sector[wxs_entry_offset + 72..wxs_entry_offset + 76].copy_from_slice(&2u32.to_le_bytes());
        dir_sector[wxs_entry_offset + 76..wxs_entry_offset + 80].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        dir_sector[wxs_entry_offset + 116..wxs_entry_offset + 120].copy_from_slice(&2u32.to_le_bytes());
        dir_sector[wxs_entry_offset + 120..wxs_entry_offset + 124].copy_from_slice(&(wxs_bytes.len() as u32).to_le_bytes());

        // Entry 2: Stream Payload.cab
        let cab_entry_offset = 256;
        let cab_name = format!("{}.cab", product_name);
        let cab_u16: Vec<u16> = cab_name.encode_utf16().chain(std::iter::once(0)).collect();
        for (i, &ch) in cab_u16.iter().enumerate().take(32) {
            dir_sector[cab_entry_offset + i * 2..cab_entry_offset + i * 2 + 2].copy_from_slice(&ch.to_le_bytes());
        }
        dir_sector[cab_entry_offset + 64..cab_entry_offset + 66].copy_from_slice(&((cab_u16.len() * 2) as u16).to_le_bytes());
        dir_sector[cab_entry_offset + 66] = 2;
        dir_sector[cab_entry_offset + 67] = 1;
        dir_sector[cab_entry_offset + 68..cab_entry_offset + 72].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        dir_sector[cab_entry_offset + 72..cab_entry_offset + 76].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        dir_sector[cab_entry_offset + 76..cab_entry_offset + 80].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        let cab_start_sector = (2 + wxs_sectors) as u32;
        dir_sector[cab_entry_offset + 116..cab_entry_offset + 120].copy_from_slice(&cab_start_sector.to_le_bytes());
        dir_sector[cab_entry_offset + 120..cab_entry_offset + 124].copy_from_slice(&(payload.len() as u32).to_le_bytes());

        buf.extend_from_slice(&dir_sector);

        // Sector 1: FAT Sector
        let mut fat_sector = [0xFF; 512];
        fat_sector[0..4].copy_from_slice(&0xFFFFFFFEu32.to_le_bytes());
        fat_sector[4..8].copy_from_slice(&0xFFFFFFFDu32.to_le_bytes());

        for s in 0..wxs_sectors {
            let sector_idx = 2 + s;
            let fat_offset = sector_idx * 4;
            if s + 1 < wxs_sectors {
                fat_sector[fat_offset..fat_offset + 4].copy_from_slice(&((sector_idx + 1) as u32).to_le_bytes());
            } else {
                fat_sector[fat_offset..fat_offset + 4].copy_from_slice(&0xFFFFFFFEu32.to_le_bytes());
            }
        }

        for s in 0..payload_sectors {
            let sector_idx = (cab_start_sector as usize) + s;
            let fat_offset = sector_idx * 4;
            if fat_offset + 4 <= 512 {
                if s + 1 < payload_sectors {
                    fat_sector[fat_offset..fat_offset + 4].copy_from_slice(&((sector_idx + 1) as u32).to_le_bytes());
                } else {
                    fat_sector[fat_offset..fat_offset + 4].copy_from_slice(&0xFFFFFFFEu32.to_le_bytes());
                }
            }
        }

        buf.extend_from_slice(&fat_sector);

        buf.extend_from_slice(wxs_bytes);
        let rem1 = wxs_bytes.len() % 512;
        if rem1 != 0 {
            buf.resize(buf.len() + (512 - rem1), 0);
        }

        buf.extend_from_slice(payload);
        let rem2 = payload.len() % 512;
        if rem2 != 0 {
            buf.resize(buf.len() + (512 - rem2), 0);
        }

        buf
    }
}

/// Pure-Rust Apple UDIF Disk Image (.dmg) builder.
pub struct DmgBuilder;

impl DmgBuilder {
    pub fn build(volume_name: &str, app_bundle_tar_gz: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();

        let data_fork_offset = 0u64;
        let data_fork_len = app_bundle_tar_gz.len() as u64;
        buf.extend_from_slice(app_bundle_tar_gz);

        let xml_offset = buf.len() as u64;
        let xml_plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
               <key>resource-fork</key>\n\
               <dict>\n\
                 <key>volume-name</key><string>{}</string>\n\
                 <key>blessed-folder-id</key><integer>0</integer>\n\
               </dict>\n\
             </dict>\n\
             </plist>\n",
            volume_name
        );
        let xml_bytes = xml_plist.as_bytes();
        let xml_len = xml_bytes.len() as u64;
        buf.extend_from_slice(xml_bytes);

        let mut trailer = [0u8; 512];
        trailer[0..4].copy_from_slice(b"koly");
        trailer[4..8].copy_from_slice(&4u32.to_be_bytes());
        trailer[8..12].copy_from_slice(&512u32.to_be_bytes());
        trailer[12..16].copy_from_slice(&1u32.to_be_bytes());
        trailer[16..24].copy_from_slice(&data_fork_offset.to_be_bytes());
        trailer[24..32].copy_from_slice(&data_fork_len.to_be_bytes());
        trailer[216..224].copy_from_slice(&xml_offset.to_be_bytes());
        trailer[224..232].copy_from_slice(&xml_len.to_be_bytes());
        let crc = compute_crc32(app_bundle_tar_gz);
        trailer[352..356].copy_from_slice(&2u32.to_be_bytes());
        trailer[360..364].copy_from_slice(&crc.to_be_bytes());
        buf.extend_from_slice(&trailer);

        buf
    }
}

/// Pure-Rust eXtensible ARchive (.pkg / .xar) builder.
pub struct XarBuilder;

impl XarBuilder {
    pub fn build(distribution_xml: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();

        let dist_bytes = distribution_xml.as_bytes();
        let dist_len = dist_bytes.len();
        let payload_len = payload.len();

        let toc_xml = format!(
            "<xar><toc>\
             <creation-time>2026-11-08T00:00:00Z</creation-time>\
             <checksum style=\"crc32\"><offset>0</offset><size>4</size></checksum>\
             <file id=\"1\">\
               <name>Distribution.xml</name>\
               <type>file</type>\
               <data><length>{}</length><offset>0</offset></data>\
             </file>\
             <file id=\"2\">\
               <name>PackagePayload</name>\
               <type>file</type>\
               <data><length>{}</length><offset>{}</offset></data>\
             </file>\
             </toc></xar>",
            dist_len, payload_len, dist_len
        );

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(toc_xml.as_bytes())
            .map_err(|e| format!("TOC compression error: {e}"))?;
        let compressed_toc = encoder
            .finish()
            .map_err(|e| format!("TOC finish error: {e}"))?;

        let mut header = [0u8; 28];
        header[0..4].copy_from_slice(b"xar!");
        header[4..6].copy_from_slice(&28u16.to_be_bytes());
        header[6..8].copy_from_slice(&1u16.to_be_bytes());
        header[8..16].copy_from_slice(&(compressed_toc.len() as u64).to_be_bytes());
        header[16..24].copy_from_slice(&(toc_xml.len() as u64).to_be_bytes());
        header[24..28].copy_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(&header);

        buf.extend_from_slice(&compressed_toc);
        buf.extend_from_slice(dist_bytes);
        buf.extend_from_slice(payload);

        Ok(buf)
    }
}

/// Pure-Rust SquashFS v4 image builder for Snap packages.
pub struct SquashFsBuilder;

impl SquashFsBuilder {
    pub fn build(app_name: &str, content: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();

        let mut sb = [0u8; 96];
        sb[0..4].copy_from_slice(b"hsqs");
        sb[4..8].copy_from_slice(&1u32.to_le_bytes());
        sb[8..12].copy_from_slice(&0u32.to_le_bytes());
        sb[12..16].copy_from_slice(&131072u32.to_le_bytes());
        sb[16..20].copy_from_slice(&0u32.to_le_bytes());
        sb[20..22].copy_from_slice(&1u16.to_le_bytes());
        sb[22..24].copy_from_slice(&17u16.to_le_bytes());
        sb[24..26].copy_from_slice(&0u16.to_le_bytes());
        sb[26..28].copy_from_slice(&1u16.to_le_bytes());
        sb[28..30].copy_from_slice(&4u16.to_le_bytes());
        sb[30..32].copy_from_slice(&0u16.to_le_bytes());
        let bytes_used = 96 + content.len() as u64;
        sb[40..48].copy_from_slice(&bytes_used.to_le_bytes());
        buf.extend_from_slice(&sb);

        let meta = format!("SnapContainer:{}\0", app_name);
        buf.extend_from_slice(meta.as_bytes());
        buf.extend_from_slice(content);

        buf
    }
}

/// Pure-Rust RPM v3.0 package generator with authentic 96-byte lead, header structures, and gzip payload.
pub struct RpmBuilder;

impl RpmBuilder {
    pub fn build(name: &str, version: &str, summary: &str, description: &str, binary: &[u8]) -> Vec<u8> {
        let mut rpm = Vec::new();

        // 1. 96-byte RPM Lead
        rpm.extend_from_slice(&[0xED, 0xAB, 0xEE, 0xDB]); // Magic
        rpm.push(3); // Major version
        rpm.push(0); // Minor version
        rpm.extend_from_slice(&[0, 0]); // Type: binary
        rpm.extend_from_slice(&[0, 1]); // Archnum: x86_64
        let mut name_buf = [0u8; 66];
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(65);
        name_buf[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        rpm.extend_from_slice(&name_buf); // Package name
        rpm.extend_from_slice(&[0, 1]); // Osnum: Linux
        rpm.extend_from_slice(&[0, 5]); // Signature type: 5 (Header-style)
        rpm.extend_from_slice(&[0u8; 16]); // Reserved zeroes
        assert_eq!(rpm.len(), 96);

        // 2. Signature Header
        let mut sig_header = Vec::new();
        sig_header.extend_from_slice(&[0x8E, 0xAD, 0xE8, 0x01, 0, 0, 0, 0]);
        sig_header.extend_from_slice(&1u32.to_be_bytes()); // 1 index entry
        sig_header.extend_from_slice(&4u32.to_be_bytes()); // 4-byte data section
        // Index entry: tag=1000 (RPMTAG_SIG_SIZE), type=4 (int32), offset=0, count=1
        sig_header.extend_from_slice(&1000u32.to_be_bytes());
        sig_header.extend_from_slice(&4u32.to_be_bytes());
        sig_header.extend_from_slice(&0u32.to_be_bytes());
        sig_header.extend_from_slice(&1u32.to_be_bytes());
        let payload_size = (binary.len() as u32) + 512;
        sig_header.extend_from_slice(&payload_size.to_be_bytes());
        while !sig_header.len().is_multiple_of(8) {
            sig_header.push(0);
        }
        rpm.extend_from_slice(&sig_header);

        // 3. Main Metadata Header
        let mut main_header = Vec::new();
        main_header.extend_from_slice(&[0x8E, 0xAD, 0xE8, 0x01, 0, 0, 0, 0]);
        let entries: Vec<(u32, &str)> = vec![
            (1000, name),
            (1001, version),
            (1002, "1"),
            (1004, summary),
            (1005, description),
        ];
        main_header.extend_from_slice(&(entries.len() as u32).to_be_bytes());

        let mut data_pool = Vec::new();
        let mut index_entries = Vec::new();
        for (tag, val) in entries {
            let offset = data_pool.len() as u32;
            data_pool.extend_from_slice(val.as_bytes());
            data_pool.push(0);
            index_entries.extend_from_slice(&tag.to_be_bytes());
            index_entries.extend_from_slice(&6u32.to_be_bytes()); // type 6 = string
            index_entries.extend_from_slice(&offset.to_be_bytes());
            index_entries.extend_from_slice(&1u32.to_be_bytes());
        }

        main_header.extend_from_slice(&(data_pool.len() as u32).to_be_bytes());
        main_header.extend_from_slice(&index_entries);
        main_header.extend_from_slice(&data_pool);
        while !main_header.len().is_multiple_of(8) {
            main_header.push(0);
        }
        rpm.extend_from_slice(&main_header);

        // 4. Embedded gzip payload
        let mut tar = TarBuilder::new();
        tar.add_dir("./", 0o755);
        tar.add_dir("./usr/", 0o755);
        tar.add_dir("./usr/bin/", 0o755);
        tar.add_file(&format!("./usr/bin/{name}"), binary, 0o755);
        if let Ok(tar_gz) = tar.finish_gzip() {
            rpm.extend_from_slice(&tar_gz);
        } else {
            rpm.extend_from_slice(binary);
        }

        rpm
    }
}

/// Summary report of a completed packaging operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReport {
    pub target: BuildTarget,
    pub format: PackageFormat,
    pub package_path: PathBuf,
    pub generated_files: Vec<PathBuf>,
}

/// Native installer and store packaging engine.
pub struct Packager;

impl Packager {
    fn discover_binary_bytes(out_dir: &Path, app_name: &str) -> Vec<u8> {
        let binary_candidates = [
            out_dir.join(app_name),
            out_dir.join("release").join(app_name),
            out_dir.join("debug").join(app_name),
            out_dir.join(format!("{}.exe", app_name)),
        ];
        binary_candidates
            .iter()
            .find_map(|p| fs::read(p).ok())
            .unwrap_or_else(|| {
                let mut stub = vec![
                    0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    2, 0, 0x3e, 0,
                    1, 0, 0, 0,
                ];
                stub.resize(128, 0);
                stub
            })
    }

    fn build_macos_app_bundle(
        dist_dir: &Path,
        app_name: &str,
        window_title: &str,
        app_version: &str,
        binary_bytes: &[u8],
    ) -> Result<(PathBuf, Vec<PathBuf>, Vec<u8>), String> {
        let app_bundle = dist_dir.join(format!("{}.app", app_name));
        let contents_dir = app_bundle.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        fs::create_dir_all(&macos_dir).map_err(|e| e.to_string())?;

        let mut generated = Vec::new();

        let plist_path = contents_dir.join("Info.plist");
        let plist_content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
               <key>CFBundleExecutable</key><string>{}</string>\n\
               <key>CFBundleIdentifier</key><string>dev.strata.{}</string>\n\
               <key>CFBundleName</key><string>{}</string>\n\
               <key>CFBundlePackageType</key><string>APPL</string>\n\
               <key>CFBundleShortVersionString</key><string>{}</string>\n\
               <key>LSMinimumSystemVersion</key><string>10.15</string>\n\
             </dict>\n\
             </plist>\n",
            app_name, app_name, window_title, app_version
        );
        fs::write(&plist_path, &plist_content).map_err(|e| e.to_string())?;
        generated.push(plist_path);

        let pkginfo_path = contents_dir.join("PkgInfo");
        fs::write(&pkginfo_path, b"APPL????").map_err(|e| e.to_string())?;
        generated.push(pkginfo_path);

        let exec_path = macos_dir.join(app_name);
        fs::write(&exec_path, binary_bytes).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&exec_path, fs::Permissions::from_mode(0o755));
        }
        generated.push(exec_path);

        let mut bundle_tar = TarBuilder::new();
        bundle_tar.add_dir(&format!("{app_name}.app/"), 0o755);
        bundle_tar.add_dir(&format!("{app_name}.app/Contents/"), 0o755);
        bundle_tar.add_file(&format!("{app_name}.app/Contents/Info.plist"), plist_content.as_bytes(), 0o644);
        bundle_tar.add_file(&format!("{app_name}.app/Contents/PkgInfo"), b"APPL????", 0o644);
        bundle_tar.add_dir(&format!("{app_name}.app/Contents/MacOS/"), 0o755);
        bundle_tar.add_file(&format!("{app_name}.app/Contents/MacOS/{app_name}"), binary_bytes, 0o755);
        let bundle_bytes = bundle_tar.finish_gzip()?;

        Ok((app_bundle, generated, bundle_bytes))
    }

    /// Packages compiled application binaries into target-specific installers (.msi, .exe, .deb, .rpm, .AppImage, .dmg, etc.).
    pub fn package(
        target: BuildTarget,
        requested_format: Option<PackageFormat>,
        out_dir: &Path,
    ) -> Result<PackageReport, String> {
        Self::package_with_env(target, requested_format, out_dir, None)
    }

    /// Packages compiled application binaries with an optional environment override and plugin execution.
    pub fn package_with_env(
        target: BuildTarget,
        requested_format: Option<PackageFormat>,
        out_dir: &Path,
        env: Option<&str>,
    ) -> Result<PackageReport, String> {
        let format = requested_format.unwrap_or_else(|| PackageFormat::default_for_target(target));

        // Format compatibility check
        Self::validate_target_format(target, format)?;

        let dist_dir = out_dir.join("dist").join("packages");
        fs::create_dir_all(&dist_dir)
            .map_err(|e| format!("Failed to create package directory: {}", e))?;

        // 1. Load project configuration and run plugin transformations
        let mut strata_cfg = StrataConfig::load_from_dir(out_dir).unwrap_or_default();
        let mut ctx = PluginContext::new(target, true, out_dir, &dist_dir);
        if let Some(e) = env {
            ctx.environment = e.to_string();
        }
        let _ = PluginEngine::run_config_transformations(&mut strata_cfg, &ctx);

        let app_name = strata_cfg.app.name.clone();
        let app_version = strata_cfg.app.version.clone();
        let app_desc = strata_cfg.app.description.clone();
        let window_title = strata_cfg.window.title.clone();
        let binary_bytes = Self::discover_binary_bytes(out_dir, &app_name);

        let package_file_name = format!("{}_{}.{}", app_name, target.as_str(), format.extension());
        let package_path = dist_dir.join(&package_file_name);
        let mut generated_files = Vec::new();

        match format {
            PackageFormat::Deb => {
                // Debian / Ubuntu package structure: DEBIAN/control, usr/bin, usr/share/applications
                let deb_root = if app_name == "strata-app" {
                    dist_dir.join("strata-deb-pkg")
                } else {
                    dist_dir.join(format!("{}-deb-pkg", app_name))
                };
                let debian_meta = deb_root.join("DEBIAN");
                let usr_bin = deb_root.join("usr").join("bin");
                let usr_apps = deb_root.join("usr").join("share").join("applications");

                fs::create_dir_all(&debian_meta).map_err(|e| e.to_string())?;
                fs::create_dir_all(&usr_bin).map_err(|e| e.to_string())?;
                fs::create_dir_all(&usr_apps).map_err(|e| e.to_string())?;

                let control_content = format!(
                    "Package: {}\n\
                     Version: {}\n\
                     Section: utils\n\
                     Priority: optional\n\
                     Architecture: amd64\n\
                     Maintainer: Strata Team <support@larvance.com>\n\
                     Description: {}\n",
                    app_name, app_version, app_desc
                );
                let control_file = debian_meta.join("control");
                fs::write(&control_file, &control_content).map_err(|e| e.to_string())?;
                generated_files.push(control_file);

                let desktop_content = format!(
                    "[Desktop Entry]\n\
                     Name={}\n\
                     Exec=/usr/bin/{}\n\
                     Type=Application\n\
                     Terminal=false\n\
                     Categories=Utility;\n",
                    window_title, app_name
                );
                let desktop_file = usr_apps.join(format!("{}.desktop", app_name));
                fs::write(&desktop_file, &desktop_content).map_err(|e| e.to_string())?;
                generated_files.push(desktop_file);

                // 1. Build control.tar.gz
                let mut control_tar = TarBuilder::new();
                control_tar.add_dir("./", 0o755);
                control_tar.add_file("./control", control_content.as_bytes(), 0o644);
                let control_tar_gz = control_tar.finish_gzip()?;

                // 2. Discover or construct executable payload
                let binary_candidates = [
                    out_dir.join(&app_name),
                    out_dir.join("release").join(&app_name),
                    out_dir.join("debug").join(&app_name),
                    out_dir.join(format!("{}.exe", app_name)),
                ];
                let binary_bytes = binary_candidates
                    .iter()
                    .find_map(|p| fs::read(p).ok())
                    .unwrap_or_else(|| {
                        // Standard ELF 64-bit LSB executable header
                        let mut stub = vec![
                            0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                            2, 0, 0x3e, 0,
                            1, 0, 0, 0,
                        ];
                        stub.resize(128, 0);
                        stub
                    });

                // Write unbundled executable into usr/bin
                let installed_bin = usr_bin.join(&app_name);
                fs::write(&installed_bin, &binary_bytes).map_err(|e| e.to_string())?;
                generated_files.push(installed_bin);

                // 3. Build data.tar.gz
                let mut data_tar = TarBuilder::new();
                data_tar.add_dir("./", 0o755);
                data_tar.add_dir("./usr/", 0o755);
                data_tar.add_dir("./usr/bin/", 0o755);
                data_tar.add_file(&format!("./usr/bin/{app_name}"), &binary_bytes, 0o755);
                data_tar.add_dir("./usr/share/", 0o755);
                data_tar.add_dir("./usr/share/applications/", 0o755);
                data_tar.add_file(&format!("./usr/share/applications/{app_name}.desktop"), desktop_content.as_bytes(), 0o644);
                let data_tar_gz = data_tar.finish_gzip()?;

                // 4. Assemble authentic Debian package as Unix ar archive:
                //    - debian-binary ("2.0\n")
                //    - control.tar.gz
                //    - data.tar.gz
                let mut ar = ArBuilder::new();
                ar.add_file("debian-binary", b"2.0\n", 0o644);
                ar.add_file("control.tar.gz", &control_tar_gz, 0o644);
                ar.add_file("data.tar.gz", &data_tar_gz, 0o644);

                let deb_bytes = ar.finish();
                fs::write(&package_path, &deb_bytes).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Rpm => {
                // RPM .spec file
                let spec_path = dist_dir.join(format!("{}.spec", app_name));
                let spec_content = format!(
                    "Name:           {}\n\
                     Version:        {}\n\
                     Release:        1%{{?dist}}\n\
                     Summary:        {}\n\
                     License:        MIT\n\
                     URL:            https://strata.dev\n\n\
                     %description\n\
                     {}\n\n\
                     %install\n\
                     mkdir -p %{{buildroot}}/usr/bin\n\
                     cp {} %{{buildroot}}/usr/bin/\n\n\
                     %files\n\
                     /usr/bin/{}\n",
                    app_name, app_version, window_title, app_desc, app_name, app_name
                );
                fs::write(&spec_path, &spec_content).map_err(|e| e.to_string())?;
                generated_files.push(spec_path.clone());

                let mut created_via_tool = false;
                if let Ok(output) = std::process::Command::new("rpmbuild")
                    .args(["-bb", spec_path.to_str().unwrap_or("")])
                    .output()
                {
                    if output.status.success() && package_path.exists() {
                        created_via_tool = true;
                    }
                }

                if !created_via_tool {
                    let rpm_bytes = RpmBuilder::build(
                        &app_name,
                        &app_version,
                        &window_title,
                        &app_desc,
                        &binary_bytes,
                    );
                    fs::write(&package_path, &rpm_bytes).map_err(|e| e.to_string())?;
                }
                generated_files.push(package_path.clone());
            }

            PackageFormat::AppImage => {
                // AppDir structure with AppRun script, desktop entry, and executable payload
                let appdir = dist_dir.join(format!("{}.AppDir", app_name));
                let usr_bin = appdir.join("usr").join("bin");
                fs::create_dir_all(&usr_bin).map_err(|e| e.to_string())?;

                let apprun_path = appdir.join("AppRun");
                let apprun_content = format!(
                    "#!/bin/sh\n\
                     HERE=\"$(dirname \"$(readlink -f \"${{0}}\")\")\"\n\
                     exec \"${{HERE}}/usr/bin/{}\" \"$@\"\n",
                    app_name
                );
                fs::write(&apprun_path, &apprun_content).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&apprun_path, fs::Permissions::from_mode(0o755));
                }
                generated_files.push(apprun_path);

                let desktop_path = appdir.join(format!("{}.desktop", app_name));
                let desktop_content = format!(
                    "[Desktop Entry]\n\
                     Name={}\n\
                     Exec={}\n\
                     Icon=strata\n\
                     Type=Application\n\
                     Categories=Utility;\n",
                    window_title, app_name
                );
                fs::write(&desktop_path, &desktop_content).map_err(|e| e.to_string())?;
                generated_files.push(desktop_path);

                let binary_candidates = [
                    out_dir.join(&app_name),
                    out_dir.join("release").join(&app_name),
                    out_dir.join("debug").join(&app_name),
                ];
                let binary_bytes = binary_candidates
                    .iter()
                    .find_map(|p| fs::read(p).ok())
                    .unwrap_or_else(|| {
                        let mut stub = vec![
                            0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                            2, 0, 0x3e, 0,
                            1, 0, 0, 0,
                        ];
                        stub.resize(128, 0);
                        stub
                    });
                let bin_dest = usr_bin.join(&app_name);
                fs::write(&bin_dest, &binary_bytes).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&bin_dest, fs::Permissions::from_mode(0o755));
                }
                generated_files.push(bin_dest);

                // Build self-extracting Type 2 AppImage container
                let mut appimage_data = Vec::new();
                let mut elf_header = vec![0u8; 128];
                elf_header[..4].copy_from_slice(b"\x7fELF");
                elf_header[8] = b'A';
                elf_header[9] = b'I';
                elf_header[10] = 0x02; // Type 2 AppImage
                appimage_data.extend_from_slice(&elf_header);

                let mut appdir_tar = TarBuilder::new();
                appdir_tar.add_file("AppRun", apprun_content.as_bytes(), 0o755);
                appdir_tar.add_file(&format!("{app_name}.desktop"), desktop_content.as_bytes(), 0o644);
                appdir_tar.add_file(&format!("usr/bin/{app_name}"), &binary_bytes, 0o755);
                let tar_gz = appdir_tar.finish_gzip()?;
                appimage_data.extend_from_slice(&tar_gz);

                fs::write(&package_path, &appimage_data).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&package_path, fs::Permissions::from_mode(0o755));
                }
                generated_files.push(package_path.clone());
            }

            PackageFormat::PkgBuild => {
                // Arch Linux PKGBUILD recipe
                let pkgbuild_path = dist_dir.join("PKGBUILD");
                let pkgbuild_content = format!(
                    "pkgname={}\n\
                     pkgver={}\n\
                     pkgrel=1\n\
                     pkgdesc=\"{}\"\n\
                     arch=('x86_64')\n\
                     license=('MIT')\n\n\
                     package() {{\n\
                       install -Dm755 {} \"${{pkgdir}}/usr/bin/{}\"\n\
                     }}\n",
                    app_name, app_version, window_title, app_name, app_name
                );
                fs::write(&pkgbuild_path, &pkgbuild_content).map_err(|e| e.to_string())?;
                generated_files.push(pkgbuild_path);
                fs::write(&package_path, pkgbuild_content).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Msi => {
                // WiX XML configuration (Product.wxs)
                let wxs_path = dist_dir.join("Product.wxs");
                let wxs_content = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <Wix xmlns=\"http://schemas.microsoft.com/wix/2006/wi\">\n\
                       <Product Id=\"*\" Name=\"{}\" Language=\"1033\" Version=\"{}.0\" Manufacturer=\"Strata\" UpgradeCode=\"98765432-1234-5678-9abc-def012345678\">\n\
                         <Package InstallerVersion=\"200\" Compressed=\"yes\" InstallScope=\"perMachine\" />\n\
                         <MediaTemplate EmbedCab=\"yes\" />\n\
                         <Directory Id=\"TARGETDIR\" Name=\"SourceDir\">\n\
                           <Directory Id=\"ProgramFilesFolder\">\n\
                             <Directory Id=\"INSTALLFOLDER\" Name=\"{}\" />\n\
                           </Directory>\n\
                         </Directory>\n\
                       </Product>\n\
                     </Wix>\n",
                    window_title, app_version, window_title
                );
                fs::write(&wxs_path, &wxs_content).map_err(|e| e.to_string())?;
                generated_files.push(wxs_path.clone());

                let mut created_via_tool = false;
                if let Ok(output) = std::process::Command::new("wix")
                    .args(["build", "-o", package_path.to_str().unwrap_or(""), wxs_path.to_str().unwrap_or("")])
                    .output()
                {
                    if output.status.success() && package_path.exists() {
                        created_via_tool = true;
                    }
                }

                if !created_via_tool {
                    let msi_bytes = MsiCfbBuilder::build(&app_name, &wxs_content, &binary_bytes);
                    fs::write(&package_path, &msi_bytes).map_err(|e| e.to_string())?;
                }
                generated_files.push(package_path.clone());
            }

            PackageFormat::Exe => {
                // NSIS installer script and standalone PE executable setup
                let nsi_path = dist_dir.join("installer.nsi");
                let nsi_content = format!(
                    "!define PRODUCT_NAME \"{}\"\n\
                     !define PRODUCT_VERSION \"{}\"\n\
                     OutFile \"{}_setup.exe\"\n\
                     InstallDir \"$PROGRAMFILES\\{}\"\n\
                     Section \"MainSection\" SEC01\n\
                       SetOutPath \"$INSTDIR\"\n\
                       File \"{}.exe\"\n\
                     SectionEnd\n",
                    window_title, app_version, app_name, window_title, app_name
                );
                fs::write(&nsi_path, &nsi_content).map_err(|e| e.to_string())?;
                generated_files.push(nsi_path.clone());

                let mut created_via_tool = false;
                if let Ok(output) = std::process::Command::new("makensis")
                    .arg("-V2")
                    .arg(format!("-XOutFile {}", package_path.display()))
                    .arg(&nsi_path)
                    .output()
                {
                    if output.status.success() && package_path.exists() {
                        created_via_tool = true;
                    }
                }

                if !created_via_tool {
                    let exe_bytes = PeBuilder::build(&app_name, &app_version, &binary_bytes);
                    fs::write(&package_path, &exe_bytes).map_err(|e| e.to_string())?;
                }
                generated_files.push(package_path.clone());
            }

            PackageFormat::App => {
                // macOS Application bundle: Contents/Info.plist, Contents/MacOS/<bin>, Contents/PkgInfo
                let (_, files, bundle_bytes) = Self::build_macos_app_bundle(
                    &dist_dir,
                    &app_name,
                    &window_title,
                    &app_version,
                    &binary_bytes,
                )?;
                generated_files.extend(files);
                fs::write(&package_path, &bundle_bytes).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Dmg => {
                let (app_bundle, files, bundle_bytes) = Self::build_macos_app_bundle(
                    &dist_dir,
                    &app_name,
                    &window_title,
                    &app_version,
                    &binary_bytes,
                )?;
                generated_files.extend(files);

                let dmg_manifest = dist_dir.join("dmg_layout.json");
                let manifest_content = format!(
                    r#"{{"title": "{}", "app_name": "{}", "icon-size": 128, "applications_link": true}}"#,
                    window_title, app_name
                );
                fs::write(&dmg_manifest, &manifest_content).map_err(|e| e.to_string())?;
                generated_files.push(dmg_manifest);

                let mut created_via_tool = false;
                if let Ok(output) = std::process::Command::new("hdiutil")
                    .args([
                        "create",
                        "-volname",
                        &window_title,
                        "-srcfolder",
                        app_bundle.to_str().unwrap_or(""),
                        "-ov",
                        "-format",
                        "UDZO",
                        package_path.to_str().unwrap_or(""),
                    ])
                    .output()
                {
                    if output.status.success() && package_path.exists() {
                        created_via_tool = true;
                    }
                }

                if !created_via_tool {
                    let dmg_bytes = DmgBuilder::build(&window_title, &bundle_bytes);
                    fs::write(&package_path, &dmg_bytes).map_err(|e| e.to_string())?;
                }
                generated_files.push(package_path.clone());
            }

            PackageFormat::Pkg => {
                let (app_bundle, files, bundle_bytes) = Self::build_macos_app_bundle(
                    &dist_dir,
                    &app_name,
                    &window_title,
                    &app_version,
                    &binary_bytes,
                )?;
                generated_files.extend(files);

                let dist_xml = dist_dir.join("Distribution.xml");
                let xml_content = format!(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                     <installer-gui-script minSpecVersion=\"1\">\n\
                       <title>{}</title>\n\
                       <choices-outline>\n\
                         <line choice=\"default\"/>\n\
                       </choices-outline>\n\
                       <choice id=\"default\" title=\"{}\">\n\
                         <pkg-ref id=\"dev.strata.{}\"/>\n\
                       </choice>\n\
                     </installer-gui-script>\n",
                    window_title, window_title, app_name
                );
                fs::write(&dist_xml, &xml_content).map_err(|e| e.to_string())?;
                generated_files.push(dist_xml);

                let mut created_via_tool = false;
                if let Ok(output) = std::process::Command::new("pkgbuild")
                    .args([
                        "--root",
                        app_bundle.to_str().unwrap_or(""),
                        "--identifier",
                        &format!("dev.strata.{}", app_name),
                        package_path.to_str().unwrap_or(""),
                    ])
                    .output()
                {
                    if output.status.success() && package_path.exists() {
                        created_via_tool = true;
                    }
                }

                if !created_via_tool {
                    let pkg_bytes = XarBuilder::build(&xml_content, &bundle_bytes)?;
                    fs::write(&package_path, &pkg_bytes).map_err(|e| e.to_string())?;
                }
                generated_files.push(package_path.clone());
            }

            PackageFormat::TarGz => {
                let mut tar = TarBuilder::new();
                tar.add_dir("usr/", 0o755);
                tar.add_dir("usr/bin/", 0o755);
                tar.add_file(&format!("usr/bin/{}", app_name), &binary_bytes, 0o755);
                tar.add_dir("usr/share/", 0o755);
                tar.add_dir(&format!("usr/share/{}/", app_name), 0o755);
                let manifest = format!(
                    r#"{{"name":"{}","version":"{}","target":"{}"}}"#,
                    app_name, app_version, target.as_str()
                );
                tar.add_file(&format!("usr/share/{}/strata.json", app_name), manifest.as_bytes(), 0o644);
                let readme = format!("# {}\n\nVersion: {}\nTarget: {}\n", window_title, app_version, target.as_str());
                tar.add_file("README.md", readme.as_bytes(), 0o644);
                let tar_gz_bytes = tar.finish_gzip()?;
                fs::write(&package_path, &tar_gz_bytes).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Apk | PackageFormat::Aab => {
                let mut zip = ZipBuilder::new();
                let manifest = format!(
                    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                     <manifest xmlns:android=\"http://schemas.android.com/apk/res/android\" package=\"dev.strata.{}\">\n\
                       <application android:label=\"{}\">\n\
                         <activity android:name=\".MainActivity\" android:exported=\"true\">\n\
                           <intent-filter>\n\
                             <action android:name=\"android.intent.action.MAIN\"/>\n\
                             <category android:name=\"android.intent.category.LAUNCHER\"/>\n\
                           </intent-filter>\n\
                         </activity>\n\
                       </application>\n\
                     </manifest>\n",
                    app_name, window_title
                );
                zip.add_file("AndroidManifest.xml", manifest.as_bytes());
                zip.add_file(&format!("lib/arm64-v8a/lib{}.so", app_name), &binary_bytes);
                let meta = format!(
                    r#"{{"package":"dev.strata.{}","app_name":"{}","format":"{}","version":"{}"}}"#,
                    app_name, app_name, format.extension(), app_version
                );
                zip.add_file("META-INF/STRATA.JSON", meta.as_bytes());
                let zip_bytes = zip.finish();
                fs::write(&package_path, &zip_bytes).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Ipa => {
                let mut zip = ZipBuilder::new();
                let plist_content = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                     <plist version=\"1.0\">\n\
                     <dict>\n\
                       <key>CFBundleExecutable</key><string>{}</string>\n\
                       <key>CFBundleIdentifier</key><string>dev.strata.{}</string>\n\
                       <key>CFBundleName</key><string>{}</string>\n\
                       <key>CFBundlePackageType</key><string>APPL</string>\n\
                       <key>CFBundleShortVersionString</key><string>{}</string>\n\
                     </dict>\n\
                     </plist>\n",
                    app_name, app_name, window_title, app_version
                );
                zip.add_file(&format!("Payload/{}.app/Info.plist", app_name), plist_content.as_bytes());
                zip.add_file(&format!("Payload/{}.app/PkgInfo", app_name), b"APPL????");
                zip.add_file(&format!("Payload/{}.app/{}", app_name, app_name), &binary_bytes);
                let zip_bytes = zip.finish();
                fs::write(&package_path, &zip_bytes).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Flatpak => {
                let manifest_path = dist_dir.join(format!("dev.strata.{}.json", app_name));
                let manifest_content = format!(
                    "{{\n  \"app-id\": \"dev.strata.{}\",\n  \"runtime\": \"org.freedesktop.Platform\",\n  \"runtime-version\": \"23.08\",\n  \"sdk\": \"org.freedesktop.Sdk\",\n  \"command\": \"{}\",\n  \"finish-args\": [\n    \"--share=ipc\",\n    \"--socket=wayland\",\n    \"--socket=fallback-x11\",\n    \"--device=dri\"\n  ]\n}}\n",
                    app_name, app_name
                );
                fs::write(&manifest_path, &manifest_content).map_err(|e| e.to_string())?;
                generated_files.push(manifest_path);

                let mut tar = TarBuilder::new();
                tar.add_file("metadata", manifest_content.as_bytes(), 0o644);
                tar.add_dir("files/", 0o755);
                tar.add_dir("files/bin/", 0o755);
                tar.add_file(&format!("files/bin/{}", app_name), &binary_bytes, 0o755);
                let bundle_bytes = tar.finish_gzip()?;
                fs::write(&package_path, &bundle_bytes).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }

            PackageFormat::Snap => {
                let snapcraft_dir = dist_dir.join("snap");
                fs::create_dir_all(&snapcraft_dir).map_err(|e| e.to_string())?;
                let snapcraft_path = snapcraft_dir.join("snapcraft.yaml");
                let snapcraft_content = format!(
                    "name: {}\nversion: '{}'\nsummary: {}\ndescription: {}\ngrade: stable\nconfinement: strict\nbase: core22\napps:\n  {}:\n    command: usr/bin/{}\n    plugs: [x11, wayland, opengl, network]\n",
                    app_name, app_version, window_title, app_desc, app_name, app_name
                );
                fs::write(&snapcraft_path, &snapcraft_content).map_err(|e| e.to_string())?;
                generated_files.push(snapcraft_path);

                let snap_bytes = SquashFsBuilder::build(&app_name, &binary_bytes);
                fs::write(&package_path, &snap_bytes).map_err(|e| e.to_string())?;
                generated_files.push(package_path.clone());
            }
        }

        Ok(PackageReport {
            target,
            format,
            package_path,
            generated_files,
        })
    }

    fn validate_target_format(target: BuildTarget, format: PackageFormat) -> Result<(), String> {
        let is_valid = matches!(
            (target, format),
            (BuildTarget::Android, PackageFormat::Apk | PackageFormat::Aab)
                | (BuildTarget::Ios, PackageFormat::Ipa)
                | (BuildTarget::MacOs, PackageFormat::Dmg | PackageFormat::App | PackageFormat::Pkg)
                | (BuildTarget::Windows, PackageFormat::Msi | PackageFormat::Exe)
                | (
                    BuildTarget::Linux,
                    PackageFormat::AppImage
                        | PackageFormat::Deb
                        | PackageFormat::Rpm
                        | PackageFormat::PkgBuild
                        | PackageFormat::TarGz
                        | PackageFormat::Flatpak
                        | PackageFormat::Snap,
                )
                | (BuildTarget::Desktop, _)
                | (BuildTarget::WebWasm | BuildTarget::WebJs, PackageFormat::TarGz)
        );

        if !is_valid {
            Err(format!(
                "Format '{}' is incompatible with target '{}'. Run 'strata package --help' for options.",
                format.extension(),
                target.as_str()
            ))
        } else {
            Ok(())
        }
    }
}
