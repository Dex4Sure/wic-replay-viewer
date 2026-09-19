//! Bounded reader for World in Conflict `.sdf` archives.
//!
//! This is a port of `scripts/wic_sdf.py` in the parent research workspace,
//! which remains the reference implementation and the place where format
//! findings are recorded. The viewer needs its own copy because map overview
//! art is read at runtime from the user's own game installation; no game asset
//! is ever redistributed with this application.
//!
//! Three codecs appear in shipped archives. Codec 0 stores bytes verbatim and
//! codec 1 is a single zlib stream. Codec 3 is the texture codec: a 128-byte
//! prefix held in the archive's version-10 auxiliary metadata, followed by three
//! independently inflated zlib streams. Codec 2 exists in the format but is not
//! used by any shipped entry and is not implemented.
//!
//! Every offset and length read from an archive is treated as untrusted and
//! bounds-checked before use.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;

const SIGNATURE: &[u8; 3] = b"RYS";
const HEADER_SIZE: u64 = 8;
const BLOCK_HEADER_SIZE: u64 = 8;
const CODEC_SHIFT: u32 = 30;
const SIZE_MASK: u32 = (1 << CODEC_SHIFT) - 1;
const AUXILIARY_HEADER_SIZE: usize = 28;
const SEGMENTED_PREFIX_SIZE: usize = 128;
const RECORD_SIZE: u32 = 20;

/// Refuse absurd declared sizes before allocating for them. Shipped directories
/// are a few megabytes and the largest shipped texture is well under this.
const MAX_BLOCK_SIZE: u32 = 128 * 1024 * 1024;
const MAX_COMPRESSED_BLOCK_SIZE: u32 = MAX_BLOCK_SIZE + 1024 * 1024;
const MAX_AUXILIARY_TOTAL_BYTES: u32 = MAX_BLOCK_SIZE;
const MAX_ARCHIVE_ENTRIES: u32 = 250_000;
const MAX_ENTRY_PATH_BYTES: usize = 4_096;
const MAX_ARCHIVE_PATH_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SdfEntry {
    pub path: String,
    pub content_id: u32,
    pub uncompressed_size: u32,
    pub compressed_size: u32,
    pub codec: u8,
    pub data_offset: u32,
    record_offset: u32,
}

#[derive(Debug, Clone)]
struct SegmentedBlock {
    prefix: Vec<u8>,
    first_compressed_size: u32,
    second_compressed_size: u32,
}

#[derive(Debug)]
pub struct SdfArchive {
    path: PathBuf,
    version: u8,
    entries: Vec<SdfEntry>,
    segmented_blocks: HashMap<u32, SegmentedBlock>,
}

fn read_at(file: &mut File, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| format!("Cannot seek to {offset}: {error}"))?;
    let mut buffer = vec![0_u8; length];
    file.read_exact(&mut buffer)
        .map_err(|error| format!("Cannot read {length} bytes at {offset}: {error}"))?;
    Ok(buffer)
}

fn u32_at(data: &[u8], offset: usize, label: &str) -> Result<u32, String> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| format!("{label} offset overflows"))?;
    let bytes = data
        .get(offset..end)
        .ok_or_else(|| format!("{label} is outside the directory"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn c_string_at<'a>(data: &'a [u8], offset: u32, label: &str) -> Result<&'a str, String> {
    let offset = offset as usize;
    let tail = data
        .get(offset..)
        .ok_or_else(|| format!("{label} is outside the directory"))?;
    let end = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| format!("{label} is not terminated"))?;
    std::str::from_utf8(&tail[..end]).map_err(|error| format!("{label} is not UTF-8: {error}"))
}

fn account_entry_path(
    total_path_bytes: &mut usize,
    prefix: &str,
    basename: &str,
    index: u32,
) -> Result<(), String> {
    let path_bytes = prefix
        .len()
        .checked_add(basename.len())
        .and_then(|length| length.checked_add(usize::from(!prefix.is_empty())))
        .ok_or_else(|| format!("Entry {index} path length overflows"))?;
    if path_bytes > MAX_ENTRY_PATH_BYTES {
        return Err(format!(
            "Entry {index} path exceeds the {MAX_ENTRY_PATH_BYTES}-byte limit"
        ));
    }
    *total_path_bytes = total_path_bytes
        .checked_add(path_bytes)
        .ok_or_else(|| "Archive path storage overflows".to_owned())?;
    if *total_path_bytes > MAX_ARCHIVE_PATH_BYTES {
        return Err(format!(
            "Archive paths exceed the {} MiB aggregate limit",
            MAX_ARCHIVE_PATH_BYTES / 1024 / 1024
        ));
    }
    Ok(())
}

fn validate_compressed_size(size: u32, label: &str) -> Result<(), String> {
    if size > MAX_COMPRESSED_BLOCK_SIZE {
        return Err(format!(
            "{label} declares an implausible compressed size {size}"
        ));
    }
    Ok(())
}

/// Inflate one complete zlib stream into an aggregate output budget, rejecting
/// truncated, trailing, or over-expanding data before it can grow without bound.
fn inflate_append_exact(
    payload: &[u8],
    output: &mut Vec<u8>,
    maximum_output: usize,
    label: &str,
) -> Result<(), String> {
    let remaining = maximum_output.saturating_sub(output.len());
    let mut limited = ZlibDecoder::new(payload).take((remaining + 1) as u64);
    limited
        .read_to_end(output)
        .map_err(|error| format!("Invalid {label} zlib stream: {error}"))?;
    let decoder = limited.into_inner();
    if output.len() > maximum_output {
        return Err(format!(
            "Decoded {label} exceeds its {maximum_output}-byte output limit"
        ));
    }
    if decoder.total_in() != payload.len() as u64 {
        return Err(format!("{label} zlib stream has trailing data"));
    }
    Ok(())
}

fn decode_block(
    payload: &[u8],
    size_word: u32,
    expected_size: u32,
    segmented: Option<&SegmentedBlock>,
) -> Result<Vec<u8>, String> {
    let codec = (size_word >> CODEC_SHIFT) as u8;
    let compressed_size = size_word & SIZE_MASK;
    validate_compressed_size(compressed_size, "Block")?;
    if payload.len() != compressed_size as usize {
        return Err("Block payload length does not match its size word".to_owned());
    }
    if expected_size > MAX_BLOCK_SIZE {
        return Err(format!(
            "Block declares an implausible size {expected_size}"
        ));
    }

    let expected_size = expected_size as usize;
    let output = match codec {
        0 => payload.to_vec(),
        1 => {
            let mut output = Vec::with_capacity(expected_size);
            inflate_append_exact(payload, &mut output, expected_size, "block")?;
            output
        }
        3 => {
            let segmented =
                segmented.ok_or_else(|| "Codec 3 metadata is unavailable".to_owned())?;
            let first_end = segmented.first_compressed_size as usize;
            let second_end = first_end + segmented.second_compressed_size as usize;
            if first_end == 0 || second_end <= first_end || second_end >= payload.len() {
                return Err("Codec 3 segment sizes do not partition the payload".to_owned());
            }
            if segmented.prefix.len() > expected_size {
                return Err("Codec 3 prefix exceeds the declared output size".to_owned());
            }
            let mut output = Vec::with_capacity(expected_size);
            output.extend_from_slice(&segmented.prefix);
            inflate_append_exact(
                &payload[..first_end],
                &mut output,
                expected_size,
                "codec 3 segment 1",
            )?;
            inflate_append_exact(
                &payload[first_end..second_end],
                &mut output,
                expected_size,
                "codec 3 segment 2",
            )?;
            inflate_append_exact(
                &payload[second_end..],
                &mut output,
                expected_size,
                "codec 3 segment 3",
            )?;
            output
        }
        other => return Err(format!("Codec {other} extraction is not implemented")),
    };

    if output.len() != expected_size {
        return Err(format!(
            "Decoded block is {} bytes, expected {expected_size}",
            output.len()
        ));
    }
    Ok(output)
}

impl SdfArchive {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file =
            File::open(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
        let file_size = file
            .metadata()
            .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?
            .len();
        if file_size < HEADER_SIZE + BLOCK_HEADER_SIZE {
            return Err("File is too small to be an SDF archive".to_owned());
        }

        let header = read_at(&mut file, 0, HEADER_SIZE as usize)?;
        if &header[..3] != SIGNATURE {
            return Err("Missing RYS signature".to_owned());
        }
        let version = header[3];
        if version != 9 && version != 10 {
            return Err(format!("Unsupported SDF version {version}"));
        }
        let directory_offset = u32_at(&header, 4, "directory offset")? as u64;
        if directory_offset < HEADER_SIZE || directory_offset > file_size - BLOCK_HEADER_SIZE {
            return Err("Directory offset is outside the archive".to_owned());
        }

        let block_header = read_at(&mut file, directory_offset, BLOCK_HEADER_SIZE as usize)?;
        let directory_size = u32_at(&block_header, 0, "directory size")?;
        let size_word = u32_at(&block_header, 4, "directory size word")?;
        let directory_compressed_size = size_word & SIZE_MASK;
        validate_compressed_size(directory_compressed_size, "Directory")?;
        if u64::from(directory_compressed_size) > file_size - directory_offset - BLOCK_HEADER_SIZE {
            return Err("Compressed directory extends past end of archive".to_owned());
        }
        let compressed = read_at(
            &mut file,
            directory_offset + BLOCK_HEADER_SIZE,
            directory_compressed_size as usize,
        )?;
        let directory = decode_block(&compressed, size_word, directory_size, None)?;

        let entry_count = u32_at(&directory, 0, "entry count")?;
        if entry_count > MAX_ARCHIVE_ENTRIES {
            return Err(format!(
                "Archive exceeds the maximum of {MAX_ARCHIVE_ENTRIES} entries"
            ));
        }
        let table_end = 4_u32
            .checked_add(
                entry_count
                    .checked_mul(8)
                    .ok_or_else(|| "Entry lookup table overflows".to_owned())?,
            )
            .ok_or_else(|| "Entry lookup table overflows".to_owned())?;
        if table_end as usize > directory.len() {
            return Err("Entry lookup table extends past the directory".to_owned());
        }

        let mut entries = Vec::with_capacity(entry_count as usize);
        let mut seen_records = HashMap::new();
        let mut total_path_bytes = 0usize;
        for index in 0..entry_count {
            let record_offset = u32_at(&directory, (4 + index * 8 + 4) as usize, "record offset")?;
            if seen_records.insert(record_offset, index).is_some() {
                return Err(format!("Duplicate record offset {record_offset:#x}"));
            }
            let record_end = record_offset
                .checked_add(RECORD_SIZE)
                .ok_or_else(|| format!("Entry {index} record overflows"))?;
            if record_offset < table_end || record_end as usize > directory.len() {
                return Err(format!("Entry {index} record is outside the directory"));
            }

            let base = record_offset as usize;
            let content_id = u32_at(&directory, base, "content id")?;
            let uncompressed_size = u32_at(&directory, base + 4, "uncompressed size")?;
            let entry_size_word = u32_at(&directory, base + 8, "entry size word")?;
            let data_offset = u32_at(&directory, base + 12, "data offset")?;
            let prefix_offset = u32_at(&directory, base + 16, "prefix offset")?;

            let compressed_size = entry_size_word & SIZE_MASK;
            let codec = (entry_size_word >> CODEC_SHIFT) as u8;
            if u64::from(data_offset) < HEADER_SIZE {
                return Err(format!("Entry {index} data offset precedes the payload"));
            }
            if u64::from(data_offset) > directory_offset
                || u64::from(compressed_size) > directory_offset - u64::from(data_offset)
            {
                return Err(format!("Entry {index} data extends into the directory"));
            }

            let prefix = c_string_at(&directory, prefix_offset, "entry prefix")?;
            let basename = c_string_at(&directory, record_offset + RECORD_SIZE, "entry name")?;
            if basename.is_empty() {
                return Err(format!("Entry {index} has an empty name"));
            }
            account_entry_path(&mut total_path_bytes, prefix, basename, index)?;
            let entry_path = if prefix.is_empty() {
                basename.to_owned()
            } else {
                format!("{prefix}/{basename}")
            };

            entries.push(SdfEntry {
                path: entry_path,
                content_id,
                uncompressed_size,
                compressed_size,
                codec,
                data_offset,
                record_offset,
            });
        }

        let has_segmented = entries.iter().any(|entry| entry.codec == 3);
        let segmented_blocks = if has_segmented {
            if version != 10 {
                return Err("Codec 3 entries require version-10 auxiliary metadata".to_owned());
            }
            read_segmented_blocks(
                &mut file,
                file_size,
                directory_offset + BLOCK_HEADER_SIZE + u64::from(directory_compressed_size),
                &entries,
            )?
        } else {
            HashMap::new()
        };

        Ok(Self {
            path: path.to_path_buf(),
            version,
            entries,
            segmented_blocks,
        })
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn entries(&self) -> &[SdfEntry] {
        &self.entries
    }

    /// Read and decode one entry. The archive is only ever opened for reading.
    pub fn read_entry(&self, entry: &SdfEntry) -> Result<Vec<u8>, String> {
        validate_compressed_size(entry.compressed_size, "Entry")?;
        let mut file = File::open(&self.path)
            .map_err(|error| format!("Cannot open {}: {error}", self.path.display()))?;
        let payload = read_at(
            &mut file,
            u64::from(entry.data_offset),
            entry.compressed_size as usize,
        )?;
        decode_block(
            &payload,
            (u32::from(entry.codec) << CODEC_SHIFT) | entry.compressed_size,
            entry.uncompressed_size,
            self.segmented_blocks.get(&entry.record_offset),
        )
    }
}

/// Read the codec-3 descriptors from the two version-10 auxiliary blocks that
/// follow the directory.
///
/// A descriptor's span is either 140 bytes, carrying its own inline 128-byte
/// prefix after the 12-byte header, or 12 bytes, whose `prefix_offset` points at
/// a prefix shared with other entries. Slicing at `prefix_offset` handles both.
fn read_segmented_blocks(
    file: &mut File,
    file_size: u64,
    auxiliary_offset: u64,
    entries: &[SdfEntry],
) -> Result<HashMap<u32, SegmentedBlock>, String> {
    if auxiliary_offset + AUXILIARY_HEADER_SIZE as u64 > file_size {
        return Err("Version-10 auxiliary header extends past end of archive".to_owned());
    }
    let header = read_at(file, auxiliary_offset, AUXILIARY_HEADER_SIZE)?;
    let auxiliary_entry_count = u32_at(&header, 0, "auxiliary entry count")?;
    let descriptor_size = u32_at(&header, 8, "descriptor block size")?;
    let descriptor_size_word = u32_at(&header, 12, "descriptor size word")?;
    let table_size = u32_at(&header, 20, "segment table size")?;
    let table_size_word = u32_at(&header, 24, "segment table size word")?;
    if auxiliary_entry_count as usize != entries.len() {
        return Err(format!(
            "Version-10 auxiliary entry count mismatch: expected {}, found {auxiliary_entry_count}",
            entries.len()
        ));
    }

    let descriptor_compressed_size = descriptor_size_word & SIZE_MASK;
    let table_compressed_size = table_size_word & SIZE_MASK;
    validate_compressed_size(descriptor_compressed_size, "Descriptor block")?;
    validate_compressed_size(table_compressed_size, "Segment table block")?;
    let auxiliary_compressed_size = descriptor_compressed_size
        .checked_add(table_compressed_size)
        .ok_or_else(|| "Version-10 auxiliary compressed size overflows".to_owned())?;
    let auxiliary_decoded_size = descriptor_size
        .checked_add(table_size)
        .ok_or_else(|| "Version-10 auxiliary decoded size overflows".to_owned())?;
    if auxiliary_compressed_size > MAX_AUXILIARY_TOTAL_BYTES
        || auxiliary_decoded_size > MAX_AUXILIARY_TOTAL_BYTES
    {
        return Err(format!(
            "Version-10 auxiliary blocks exceed the {} MiB aggregate limit",
            MAX_AUXILIARY_TOTAL_BYTES / 1024 / 1024
        ));
    }
    let blocks_start = auxiliary_offset + AUXILIARY_HEADER_SIZE as u64;
    let blocks_end =
        blocks_start + u64::from(descriptor_compressed_size) + u64::from(table_compressed_size);
    if blocks_end > file_size {
        return Err("Version-10 auxiliary blocks extend past end of archive".to_owned());
    }

    let descriptor_payload = read_at(file, blocks_start, descriptor_compressed_size as usize)?;
    let descriptors = decode_block(
        &descriptor_payload,
        descriptor_size_word,
        descriptor_size,
        None,
    )?;
    drop(descriptor_payload);
    let table_payload = read_at(
        file,
        blocks_start + u64::from(descriptor_compressed_size),
        table_compressed_size as usize,
    )?;
    let segment_table = decode_block(&table_payload, table_size_word, table_size, None)?;
    drop(table_payload);

    let expected_table_size = entries.len().saturating_mul(8);
    if segment_table.len() != expected_table_size {
        return Err(format!(
            "Version-10 segment table size mismatch: expected {expected_table_size}, found {}",
            segment_table.len()
        ));
    }

    let mut result = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if entry.codec != 3 {
            continue;
        }
        let base = index * 8;
        let kind = u16::from_le_bytes([segment_table[base], segment_table[base + 1]]);
        let span = u16::from_le_bytes([segment_table[base + 2], segment_table[base + 3]]);
        let descriptor_offset = u32_at(&segment_table, base + 4, "descriptor offset")?;
        if kind != 1 {
            return Err(format!("Codec 3 entry {index} has descriptor kind {kind}"));
        }
        if span < 12 || descriptor_offset as usize + span as usize > descriptors.len() {
            return Err(format!(
                "Codec 3 entry {index} descriptor is outside metadata"
            ));
        }

        let descriptor_base = descriptor_offset as usize;
        let prefix_offset = u32_at(&descriptors, descriptor_base, "prefix offset")? as usize;
        let first_size = u32_at(&descriptors, descriptor_base + 4, "first segment size")?;
        let second_size = u32_at(&descriptors, descriptor_base + 8, "second segment size")?;
        let prefix_end = prefix_offset
            .checked_add(SEGMENTED_PREFIX_SIZE)
            .ok_or_else(|| format!("Codec 3 entry {index} prefix overflows"))?;
        if prefix_end > descriptors.len() {
            return Err(format!("Codec 3 entry {index} prefix is outside metadata"));
        }
        if first_size == 0 || second_size == 0 {
            return Err(format!("Codec 3 entry {index} has an empty stored segment"));
        }
        if first_size.saturating_add(second_size) >= entry.compressed_size {
            return Err(format!(
                "Codec 3 entry {index} segment sizes exceed its payload"
            ));
        }

        result.insert(
            entry.record_offset,
            SegmentedBlock {
                prefix: descriptors[prefix_offset..prefix_end].to_vec(),
                first_compressed_size: first_size,
                second_compressed_size: second_size,
            },
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    fn deflate(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    struct Fixture {
        descriptor_kind: u16,
        auxiliary_entry_count: u32,
        first_size: Option<u32>,
        second_size: Option<u32>,
    }

    impl Default for Fixture {
        fn default() -> Self {
            Self {
                descriptor_kind: 1,
                auxiliary_entry_count: 1,
                first_size: None,
                second_size: None,
            }
        }
    }

    /// Mirrors `segmented_fixture_bytes` in `scripts/test_wic_sdf.py`, so both
    /// implementations are exercised against the same synthetic layout.
    impl Fixture {
        fn build(&self) -> (Vec<u8>, Vec<u8>) {
            let mut prefix = b"DDS ".to_vec();
            prefix.extend((0..124_u8).map(|byte| byte.wrapping_mul(3)));
            let parts: [Vec<u8>; 3] = [
                b"first segment\n".to_vec(),
                b"second segment\n".repeat(2),
                b"third segment\n".repeat(3),
            ];
            let compressed: Vec<Vec<u8>> = parts.iter().map(|part| deflate(part)).collect();
            let payload: Vec<u8> = compressed.concat();
            let mut expected = prefix.clone();
            for part in &parts {
                expected.extend(part);
            }

            let basename = b"overviewmap.dds\0";
            let path_prefix = b"maps/example\0";
            let record_offset: u32 = 12;
            let prefix_offset = record_offset + RECORD_SIZE + basename.len() as u32;

            let mut directory = Vec::new();
            directory.extend(1_u32.to_le_bytes());
            directory.extend(0xAABB_CCDD_u32.to_le_bytes());
            directory.extend(record_offset.to_le_bytes());
            directory.extend(0x8765_4321_u32.to_le_bytes());
            directory.extend((expected.len() as u32).to_le_bytes());
            directory.extend((((3_u32) << CODEC_SHIFT) | payload.len() as u32).to_le_bytes());
            directory.extend(8_u32.to_le_bytes());
            directory.extend(prefix_offset.to_le_bytes());
            directory.extend(basename);
            directory.extend(path_prefix);
            let compressed_directory = deflate(&directory);

            let mut descriptor = Vec::new();
            descriptor.extend(12_u32.to_le_bytes());
            descriptor.extend(
                self.first_size
                    .unwrap_or(compressed[0].len() as u32)
                    .to_le_bytes(),
            );
            descriptor.extend(
                self.second_size
                    .unwrap_or(compressed[1].len() as u32)
                    .to_le_bytes(),
            );
            descriptor.extend(&prefix);
            let compressed_descriptor = deflate(&descriptor);

            let mut segment_table = Vec::new();
            segment_table.extend(self.descriptor_kind.to_le_bytes());
            segment_table.extend((descriptor.len() as u16).to_le_bytes());
            segment_table.extend(0_u32.to_le_bytes());
            let compressed_table = deflate(&segment_table);

            let mut auxiliary = Vec::new();
            auxiliary.extend(self.auxiliary_entry_count.to_le_bytes());
            auxiliary.extend(0_u32.to_le_bytes());
            auxiliary.extend((descriptor.len() as u32).to_le_bytes());
            auxiliary.extend(
                (((1_u32) << CODEC_SHIFT) | compressed_descriptor.len() as u32).to_le_bytes(),
            );
            auxiliary.extend(0_u32.to_le_bytes());
            auxiliary.extend((segment_table.len() as u32).to_le_bytes());
            auxiliary
                .extend((((1_u32) << CODEC_SHIFT) | compressed_table.len() as u32).to_le_bytes());

            let mut archive = Vec::new();
            archive.extend(b"RYS\x0a");
            archive.extend((8 + payload.len() as u32).to_le_bytes());
            archive.extend(&payload);
            archive.extend((directory.len() as u32).to_le_bytes());
            archive.extend(
                (((1_u32) << CODEC_SHIFT) | compressed_directory.len() as u32).to_le_bytes(),
            );
            archive.extend(&compressed_directory);
            archive.extend(&auxiliary);
            archive.extend(&compressed_descriptor);
            archive.extend(&compressed_table);
            (archive, expected)
        }

        fn open(&self) -> (tempfile::TempDir, Result<SdfArchive, String>, Vec<u8>) {
            let (bytes, expected) = self.build();
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("fixture.sdf");
            std::fs::write(&path, &bytes).unwrap();
            let archive = SdfArchive::open(&path);
            (directory, archive, expected)
        }
    }

    #[test]
    fn extracts_a_three_stream_codec_3_entry() {
        let (_guard, archive, expected) = Fixture::default().open();
        let archive = archive.expect("fixture opens");
        assert_eq!(archive.version(), 10);
        assert_eq!(archive.entries().len(), 1);
        let entry = &archive.entries()[0];
        assert_eq!(entry.path, "maps/example/overviewmap.dds");
        assert_eq!(entry.codec, 3);
        assert_eq!(archive.read_entry(entry).unwrap(), expected);
    }

    #[test]
    fn rejects_a_descriptor_of_the_wrong_kind() {
        let (_guard, archive, _) = Fixture {
            descriptor_kind: 0,
            ..Fixture::default()
        }
        .open();
        assert!(archive.unwrap_err().contains("descriptor kind"));
    }

    #[test]
    fn rejects_an_auxiliary_entry_count_mismatch() {
        let (_guard, archive, _) = Fixture {
            auxiliary_entry_count: 2,
            ..Fixture::default()
        }
        .open();
        assert!(archive.unwrap_err().contains("entry count mismatch"));
    }

    #[test]
    fn rejects_segments_that_overrun_the_payload() {
        let (_guard, archive, _) = Fixture {
            first_size: Some(0x3FFF_FFF0),
            ..Fixture::default()
        }
        .open();
        assert!(archive.unwrap_err().contains("exceed its payload"));
    }

    #[test]
    fn rejects_an_empty_stored_segment() {
        let (_guard, archive, _) = Fixture {
            second_size: Some(0),
            ..Fixture::default()
        }
        .open();
        assert!(archive.unwrap_err().contains("empty stored segment"));
    }

    #[test]
    fn rejects_zlib_output_beyond_the_declared_block_size() {
        let payload = deflate(&[0x41; 64]);
        let size_word = (1_u32 << CODEC_SHIFT) | payload.len() as u32;
        let error = decode_block(&payload, size_word, 8, None).expect_err("output limit");
        assert!(error.contains("8-byte output limit"));
    }

    #[test]
    fn rejects_compressed_block_sizes_before_allocation() {
        let error = validate_compressed_size(MAX_COMPRESSED_BLOCK_SIZE + 1, "Entry")
            .expect_err("compressed block limit");
        assert!(error.contains("implausible compressed size"));
    }

    #[test]
    fn bounds_individual_and_aggregate_archive_path_storage() {
        let mut total = 0;
        let maximum = "a".repeat(MAX_ENTRY_PATH_BYTES);
        account_entry_path(&mut total, "", &maximum, 0).expect("path at limit");

        let oversized = format!("{maximum}b");
        let error =
            account_entry_path(&mut total, "", &oversized, 1).expect_err("individual path limit");
        assert!(error.contains("4096-byte limit"));

        let mut total = MAX_ARCHIVE_PATH_BYTES;
        let error = account_entry_path(&mut total, "", "x", 2).expect_err("aggregate path limit");
        assert!(error.contains("aggregate limit"));
    }

    #[test]
    fn rejects_a_bad_signature() {
        let (mut bytes, _) = Fixture::default().build();
        bytes[..3].copy_from_slice(b"BAD");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.sdf");
        std::fs::write(&path, &bytes).unwrap();
        assert!(SdfArchive::open(&path).unwrap_err().contains("RYS"));
    }

    #[test]
    fn rejects_a_truncated_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tiny.sdf");
        std::fs::write(&path, b"RYS\x0a").unwrap();
        assert!(SdfArchive::open(&path).is_err());
    }
}
