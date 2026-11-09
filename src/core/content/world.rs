//! Reading `level.dat`.
//!
//! The file is gzip-compressed NBT, a binary tree written by the game rather than
//! by this launcher. Only six values are needed, so this is a streaming reader: it
//! walks the tree, keeps the keys it cares about and skips the rest. Every read is
//! bounded, because the input is whatever the player put there.

use std::io::Read;
use std::path::Path;

/// The fields the world list shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldInfo {
    pub level_name: String,
    pub version: Option<String>,
    pub last_played: Option<i64>,
    pub game_mode: Option<String>,
    pub hardcore: bool,
}

const END: u8 = 0;
const BYTE: u8 = 1;
const SHORT: u8 = 2;
const INT: u8 = 3;
const LONG: u8 = 4;
const FLOAT: u8 = 5;
const DOUBLE: u8 = 6;
const BYTE_ARRAY: u8 = 7;
const STRING: u8 = 8;
const LIST: u8 = 9;
const COMPOUND: u8 = 10;
const INT_ARRAY: u8 = 11;
const LONG_ARRAY: u8 = 12;

/// Guard rails. A corrupted or hostile file must not be able to allocate or
/// recurse without limit.
const MAX_DEPTH: usize = 32;
const MAX_STRING: usize = 64 * 1024;
const MAX_ARRAY: usize = 4 * 1024 * 1024;
const MAX_TOTAL: usize = 64 * 1024 * 1024;

pub fn read(path: &Path) -> Option<WorldInfo> {
    let raw = std::fs::read(path).ok()?;
    // Vanilla always gzips; some tools write the tree uncompressed, so a failed
    // gunzip falls back to the bytes as they are.
    let data = gunzip(&raw).unwrap_or(raw);
    if data.len() > MAX_TOTAL {
        return None;
    }
    parse(&data)
}

fn gunzip(raw: &[u8]) -> Option<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(raw);
    let mut out = Vec::new();
    decoder.take(MAX_TOTAL as u64).read_to_end(&mut out).ok()?;
    (!out.is_empty()).then_some(out)
}

fn parse(bytes: &[u8]) -> Option<WorldInfo> {
    let mut nbt = Nbt::new(bytes);
    if nbt.u8()? != COMPOUND {
        return None;
    }
    nbt.string()?;
    loop {
        let tag = nbt.u8()?;
        if tag == END {
            return None;
        }
        let name = nbt.string()?;
        if tag == COMPOUND && name == "Data" {
            return read_data(&mut nbt);
        }
        nbt.skip(tag, 0)?;
    }
}

fn read_data(nbt: &mut Nbt<'_>) -> Option<WorldInfo> {
    let mut info = WorldInfo::default();
    loop {
        let tag = nbt.u8()?;
        if tag == END {
            return Some(info);
        }
        let name = nbt.string()?;
        match (name.as_str(), tag) {
            ("LevelName", STRING) => info.level_name = nbt.string()?,
            ("LastPlayed", LONG) => info.last_played = Some(nbt.i64()?),
            ("hardcore", BYTE) => info.hardcore = nbt.u8()? != 0,
            ("GameType", INT) => info.game_mode = Some(game_mode(nbt.i32()?).to_owned()),
            ("Version", COMPOUND) => info.version = read_version(nbt),
            _ => nbt.skip(tag, 0)?,
        }
    }
}

fn read_version(nbt: &mut Nbt<'_>) -> Option<String> {
    let mut version = None;
    loop {
        let tag = nbt.u8()?;
        if tag == END {
            return version;
        }
        let name = nbt.string()?;
        if name == "Name" && tag == STRING {
            version = Some(nbt.string()?);
        } else {
            nbt.skip(tag, 0)?;
        }
    }
}

fn game_mode(id: i32) -> &'static str {
    match id {
        1 => "creative",
        2 => "adventure",
        3 => "spectator",
        _ => "survival",
    }
}

struct Nbt<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Nbt<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(length)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let length = usize::from(u16::try_from(self.i16()?).ok()?);
        if length > MAX_STRING {
            return None;
        }
        String::from_utf8(self.take(length)?.to_vec()).ok()
    }

    /// Step over a payload of `tag`, whatever it is.
    fn skip(&mut self, tag: u8, depth: usize) -> Option<()> {
        if depth > MAX_DEPTH {
            return None;
        }
        match tag {
            BYTE => self.take(1).map(|_| ()),
            SHORT => self.take(2).map(|_| ()),
            INT | FLOAT => self.take(4).map(|_| ()),
            LONG | DOUBLE => self.take(8).map(|_| ()),
            BYTE_ARRAY => {
                let length = self.i32()?;
                self.take(count(length)?).map(|_| ())
            }
            STRING => self.string().map(|_| ()),
            LIST => {
                let element = self.u8()?;
                let length = count(self.i32()?)?;
                if element == END {
                    // An empty list still carries a type byte and a zero length.
                    return Some(());
                }
                for _ in 0..length {
                    self.skip(element, depth + 1)?;
                }
                Some(())
            }
            COMPOUND => loop {
                let inner = self.u8()?;
                if inner == END {
                    return Some(());
                }
                self.string()?;
                self.skip(inner, depth + 1)?;
            },
            INT_ARRAY => {
                let length = count(self.i32()?)?;
                self.take(length.checked_mul(4)?).map(|_| ())
            }
            LONG_ARRAY => {
                let length = count(self.i32()?)?;
                self.take(length.checked_mul(8)?).map(|_| ())
            }
            _ => None,
        }
    }
}

fn count(raw: i32) -> Option<usize> {
    let length = usize::try_from(raw).ok()?;
    (length <= MAX_ARRAY).then_some(length)
}

#[cfg(test)]
mod tests {
    use super::{WorldInfo, read};
    use std::path::{Path, PathBuf};

    fn temp_file(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("tmcl-level-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create temp dir");
        root.join("level.dat")
    }

    fn name(text: &str) -> Vec<u8> {
        let mut out = (text.len() as u16).to_be_bytes().to_vec();
        out.extend_from_slice(text.as_bytes());
        out
    }

    fn long(value: i64) -> Vec<u8> {
        value.to_be_bytes().to_vec()
    }

    fn int(value: i32) -> Vec<u8> {
        value.to_be_bytes().to_vec()
    }

    fn write_gzip(path: &Path, payload: &[u8]) {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write;
        let file = std::fs::File::create(path).expect("create level.dat");
        let mut encoder = GzEncoder::new(file, Compression::fast());
        encoder.write_all(payload).expect("write gzip");
        encoder.finish().expect("finish gzip");
    }

    /// Root compound → `Data` compound with the fields the list shows.
    fn sample(extra: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(8); // TAG_String
        data.extend(name("LevelName"));
        data.extend(name("Birthday World"));
        data.push(4); // TAG_Long
        data.extend(name("LastPlayed"));
        data.extend(long(1_700_000_000_000));
        data.push(1); // TAG_Byte
        data.extend(name("hardcore"));
        data.push(1);
        data.push(3); // TAG_Int
        data.extend(name("GameType"));
        data.extend(int(1));
        data.push(10); // TAG_Compound
        data.extend(name("Version"));
        data.push(8);
        data.extend(name("Name"));
        data.extend(name("1.20.1"));
        data.push(0); // end of Version
        data.extend_from_slice(extra);
        data.push(0); // end of Data

        let mut root = vec![10];
        root.extend(name(""));
        root.push(10); // TAG_Compound
        root.extend(name("Data"));
        root.extend(data);
        root.push(0); // end of root
        root
    }

    #[test]
    fn level_dat_reads_name_version_and_last_played() {
        let path = temp_file("reads");
        write_gzip(&path, &sample(&[]));
        assert_eq!(
            read(&path),
            Some(WorldInfo {
                level_name: "Birthday World".into(),
                version: Some("1.20.1".into()),
                last_played: Some(1_700_000_000_000),
                game_mode: Some("creative".into()),
                hardcore: true,
            })
        );
        std::fs::remove_file(&path).expect("clean temp file");
    }

    #[test]
    fn level_dat_skips_unknown_payloads() {
        let path = temp_file("skip");
        // A list of compounds, an int array and a byte array all sit between the
        // fields we want; the reader must step over them and keep going.
        let mut extra = Vec::new();
        extra.push(9); // TAG_List
        extra.extend(name("Structures"));
        extra.push(10); // of compounds
        extra.extend(int(1));
        extra.push(10);
        extra.extend(name("start"));
        extra.push(3);
        extra.extend(name("n"));
        extra.extend(int(7));
        extra.push(0); // end compound
        extra.push(0); // end list
        extra.push(11); // TAG_IntArray
        extra.extend(name("Heights"));
        extra.extend(int(2));
        extra.extend(int(1));
        extra.extend(int(2));
        extra.push(7); // TAG_ByteArray
        extra.extend(name("Blob"));
        extra.extend(int(1));
        extra.push(0x7f);
        // A field *after* the skipped payloads proves the reader resynchronised.
        extra.push(8);
        extra.extend(name("LevelName2"));
        extra.extend(name("ignored"));
        write_gzip(&path, &sample(&extra));
        let info = read(&path).expect("read level.dat");
        assert_eq!(info.level_name, "Birthday World");
        assert_eq!(info.version.as_deref(), Some("1.20.1"));
        std::fs::remove_file(&path).expect("clean temp file");
    }

    #[test]
    fn level_dat_reads_uncompressed_files() {
        let path = temp_file("plain");
        std::fs::write(&path, sample(&[])).expect("write level.dat");
        assert_eq!(
            read(&path).map(|info| info.level_name),
            Some("Birthday World".to_owned())
        );
        std::fs::remove_file(&path).expect("clean temp file");
    }

    #[test]
    fn corrupt_level_dat_returns_none() {
        let path = temp_file("corrupt");
        std::fs::write(&path, b"\x1f\x8b\x08\x00not really gzip").expect("write level.dat");
        assert_eq!(read(&path), None);

        std::fs::write(&path, b"\x0a\x00\x00").expect("write truncated");
        assert_eq!(read(&path), None);
        std::fs::remove_file(&path).expect("clean temp file");
    }

    #[test]
    fn deep_nesting_is_refused() {
        let path = temp_file("deep");
        // 40 nested single-key compounds: beyond the depth guard.
        let mut data = vec![10];
        data.extend(name("Data"));
        for index in 0..40 {
            data.push(10);
            data.extend(name(&format!("n{index}")));
        }
        data.extend(std::iter::repeat_n(0u8, 40));
        data.push(0);
        let mut root = vec![10];
        root.extend(name(""));
        root.extend(data);
        root.push(0);
        write_gzip(&path, &root);
        assert_eq!(read(&path), None);
        std::fs::remove_file(&path).expect("clean temp file");
    }
}
