//! Opening modpack archives without rejecting otherwise readable packs.
//!
//! `zip` refuses a whole archive when the optional Info-ZIP Unicode Path extra
//! field (0x7075) carries a CRC that does not match the stored name, and reports
//! the failure as `Could not find EOCD`, indistinguishable from a truncated
//! download. Packers do get that CRC wrong: they compute it over a different
//! encoding of the name than the one they write, so packs made on Windows carry
//! dozens of stale checksums. Explorer, 7-Zip, PCL2 and HMCL ignore the field,
//! so those packs open everywhere but here.
//!
//! The field is redundant whenever the name it describes is already stored in the
//! record, so recovery is rewriting the four CRC bytes that make it consistent. Nothing else in the archive moves: local headers, data offsets
//! and the central directory length stay exactly as they were.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::core::error::{Error, Result};

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const UNICODE_PATH_FIELD: u16 = 0x7075;
const EOCD_LEN: usize = 22;
const CENTRAL_HEADER_LEN: usize = 46;
/// An archive comment may be 64 KiB long, so the EOCD can sit that far back.
const EOCD_SEARCH_LIMIT: usize = u16::MAX as usize + EOCD_LEN;
/// Version byte + CRC32 before the UTF-8 name inside a 0x7075 field.
const UNICODE_FIELD_HEADER_LEN: usize = 5;

/// Open an archive, repairing stale Unicode path checksums once if `zip` chokes.
pub fn open(path: &Path) -> Result<zip::ZipArchive<File>> {
    match open_once(path) {
        Ok(archive) => Ok(archive),
        Err(error) => match repair_unicode_path_crc(path) {
            // A repair that cannot even run (read-only file, no EOCD) leaves the
            // original complaint in place: it is the one that describes the file.
            Ok(0) | Err(_) => Err(error),
            Ok(_) => open_once(path),
        },
    }
}

fn open_once(path: &Path) -> Result<zip::ZipArchive<File>> {
    let file = File::open(path)?;
    zip::ZipArchive::new(file).map_err(|error| {
        let text = error.to_string();
        // `zip` reports every central-directory problem as a missing end record,
        // so a packer's broken extra field reads like a half-downloaded file. Say
        // what the reader can verify.
        if text.contains("Could not find EOCD") {
            Error::Download(format!(
                "read modpack archive: no end record — the file is truncated, still downloading, or not a ZIP ({text})"
            ))
        } else {
            Error::Download(format!("read modpack archive: {text}"))
        }
    })
}

/// Rewrite every 0x7075 checksum that disagrees with its entry name.
///
/// Returns how many fields were corrected; zero means the archive was left
/// untouched, which is also what happens for anything that is not a plain
/// (non-ZIP64) archive.
fn repair_unicode_path_crc(path: &Path) -> Result<usize> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let file_len = file.seek(SeekFrom::End(0))?;
    if file_len < EOCD_LEN as u64 {
        return Ok(0);
    }
    let tail_len = (file_len as usize).min(EOCD_SEARCH_LIMIT);
    let mut tail = vec![0u8; tail_len];
    file.seek(SeekFrom::Start(file_len - tail_len as u64))?;
    file.read_exact(&mut tail)?;
    let Some(eocd) = find_eocd(&tail) else {
        return Ok(0);
    };
    let central_offset = read_u32(&tail[eocd + 16..]) as u64;
    let central_size = read_u32(&tail[eocd + 12..]) as u64;
    // ZIP64 stores sentinels here and keeps the real values in the ZIP64
    // records; those archives are rare and never the ones that fail.
    if central_offset == u32::MAX as u64 || central_size == u32::MAX as u64 {
        return Ok(0);
    }
    let Some(central_end) = central_offset.checked_add(central_size) else {
        return Ok(0);
    };
    if central_end > file_len {
        return Ok(0);
    }
    let mut central = vec![0u8; central_size as usize];
    file.seek(SeekFrom::Start(central_offset))?;
    file.read_exact(&mut central)?;
    let repaired = repair_central_directory(&mut central);
    if repaired > 0 {
        file.seek(SeekFrom::Start(central_offset))?;
        file.write_all(&central)?;
        file.flush()?;
    }
    Ok(repaired)
}

fn find_eocd(tail: &[u8]) -> Option<usize> {
    let end = tail.len();
    // The record is 22 bytes and must end exactly at the end of the file.
    (0..=end.checked_sub(EOCD_LEN)?).rev().find(|&offset| {
        read_u32(&tail[offset..]) == EOCD_SIGNATURE
            && offset + EOCD_LEN + read_u16(&tail[offset + 20..]) as usize == end
    })
}

/// Fix the stale checksums in place; the record lengths never change.
fn repair_central_directory(central: &mut [u8]) -> usize {
    let mut repaired = 0;
    let mut cursor = 0;
    while cursor + CENTRAL_HEADER_LEN <= central.len() {
        if read_u32(&central[cursor..]) != CENTRAL_HEADER_SIGNATURE {
            break;
        }
        let name_len = read_u16(&central[cursor + 28..]) as usize;
        let extra_len = read_u16(&central[cursor + 30..]) as usize;
        let comment_len = read_u16(&central[cursor + 32..]) as usize;
        let name_start = cursor + CENTRAL_HEADER_LEN;
        let Some(name_end) = name_start.checked_add(name_len) else {
            break;
        };
        let Some(extra_end) = name_end.checked_add(extra_len) else {
            break;
        };
        if extra_end + comment_len > central.len() {
            break;
        }
        let name_crc = crc32fast::hash(&central[name_start..name_end]);
        let mut field = name_end;
        while field + 4 <= extra_end {
            let id = read_u16(&central[field..]);
            let len = read_u16(&central[field + 2..]) as usize;
            let body = field + 4;
            let Some(next) = body.checked_add(len) else {
                break;
            };
            if next > extra_end {
                break;
            }
            // Only the path field: the crate checks it against the raw name
            // bytes, while the comment field is checked against an already
            // decoded string whose bytes we cannot reconstruct here.
            if id == UNICODE_PATH_FIELD && len >= UNICODE_FIELD_HEADER_LEN {
                let crc = body + 1;
                if read_u32(&central[crc..]) != name_crc {
                    central[crc..crc + 4].copy_from_slice(&name_crc.to_le_bytes());
                    repaired += 1;
                }
            }
            field = next;
        }
        cursor = extra_end + comment_len;
    }
    repaired
}

fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use super::{open, repair_unicode_path_crc};
    use std::path::PathBuf;

    struct Entry<'a> {
        name: &'a [u8],
        extra: &'a [u8],
        data: &'a [u8],
    }

    /// A minimal ZIP with stored entries, so the test controls every byte.
    fn stored_zip(entries: &[Entry<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut offsets = Vec::new();
        for entry in entries {
            offsets.push(out.len() as u32);
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc32fast::hash(entry.data).to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(entry.name);
            out.extend_from_slice(entry.data);
        }
        let central_offset = out.len() as u32;
        for (entry, offset) in entries.iter().zip(&offsets) {
            out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            out.extend_from_slice(&0x0314u16.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc32fast::hash(entry.data).to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(entry.extra.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(entry.name);
            out.extend_from_slice(entry.extra);
        }
        let central_size = out.len() as u32 - central_offset;
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// A 0x7075 field whose checksum does not match `name`: what packers that hash
    /// the pre-conversion name leave behind.
    fn stale_unicode_path(name: &[u8], stored_crc: u32) -> Vec<u8> {
        let mut field = Vec::new();
        field.extend_from_slice(&0x7075u16.to_le_bytes());
        field.extend_from_slice(&((5 + name.len()) as u16).to_le_bytes());
        field.push(1);
        field.extend_from_slice(&stored_crc.to_le_bytes());
        field.extend_from_slice(name);
        field
    }

    fn write_temp(bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("tmcl-zip-{}.zip", uuid::Uuid::new_v4()));
        std::fs::write(&path, bytes).expect("write archive");
        path
    }

    const INDEX: &[u8] = br#"{"dependencies":{"minecraft":"1.20.1","forge":"47.2.0"}}"#;

    #[test]
    fn a_stale_unicode_checksum_is_repaired_before_the_archive_is_rejected() {
        let name = "overrides/config/新的世界/settings.txt".as_bytes();
        let extra = stale_unicode_path(name, 0x1234_5678);
        let archive = write_temp(&stored_zip(&[
            Entry {
                name: b"modrinth.index.json",
                extra: &[],
                data: INDEX,
            },
            Entry {
                name,
                extra: &extra,
                data: b"ok",
            },
        ]));

        // The stock reader refuses the whole pack, with an error that reads
        // like a truncated file rather than a broken optional field.
        let rejected = zip::ZipArchive::new(std::fs::File::open(&archive).expect("open"));
        assert!(rejected.is_err(), "zip should reject the stale checksum");

        let opened = open(&archive).expect("repair then open");
        assert_eq!(opened.len(), 2);
        assert_eq!(
            crate::core::resource::modpack::read_modpack_metadata(&archive)
                .expect("read metadata")
                .loader,
            "forge"
        );
        std::fs::remove_file(archive).expect("clean archive");
    }

    #[test]
    fn a_consistent_archive_is_left_byte_for_byte_alone() {
        let archive = write_temp(&stored_zip(&[Entry {
            name: b"modrinth.index.json",
            extra: &[],
            data: INDEX,
        }]));
        let before = std::fs::read(&archive).expect("read archive");
        assert_eq!(repair_unicode_path_crc(&archive).expect("repair"), 0);
        assert_eq!(std::fs::read(&archive).expect("read archive"), before);
        std::fs::remove_file(archive).expect("clean archive");
    }

    #[test]
    fn repairing_twice_changes_nothing_the_second_time() {
        let name = "overrides/mods/视频播放器.jar".as_bytes();
        let extra = stale_unicode_path(name, 0);
        let archive = write_temp(&stored_zip(&[Entry {
            name,
            extra: &extra,
            data: b"jar",
        }]));
        assert_eq!(repair_unicode_path_crc(&archive).expect("repair"), 1);
        assert_eq!(repair_unicode_path_crc(&archive).expect("repair"), 0);
        // The name itself is untouched, only its checksum was rewritten.
        assert!(
            std::fs::read(&archive)
                .expect("read")
                .windows(name.len())
                .any(|window| window == name)
        );
        std::fs::remove_file(archive).expect("clean archive");
    }

    #[test]
    fn a_truncated_file_still_reports_an_error() {
        let archive = write_temp(b"PK\x03\x04 not really a zip");
        let error = open(&archive).expect_err("garbage must fail").to_string();
        assert!(error.contains("read modpack archive"), "{error}");
        assert!(error.contains("truncated"), "{error}");
        std::fs::remove_file(archive).expect("clean archive");
    }
}
