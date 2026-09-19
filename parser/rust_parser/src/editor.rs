//! Source-preserving `.wicdemo` metadata rewriting.
//!
//! World in Conflict stores the user-facing in-game replay title in the
//! `ReplayName` BinTag field. Changing its variable-length UTF-16 payload shifts
//! the decompressed stream, so the complete stream is re-chunked using the
//! game's 16 KiB chunk size. The writer then decodes its own output and requires
//! the decompressed bytes to equal the intended one-field transformation.

use std::io::Write as _;

use flate2::write::ZlibEncoder;
use flate2::{Compression, Decompress, FlushDecompress, Status};

use crate::parser::{MAX_REPLAY_COMPRESSED_BYTES, WicReplayParser};

const FILE_HEADER: &[u8; 15] = b"\r\0BinTagFormat2";
const REPLAY_NAME_HASH: [u8; 4] = [0xef, 0x03, 0x7c, 0x15];
const STRING_FIELD_FLAG: u8 = 5;
const FIELD_HEADER_BYTES: usize = 13;
const WRITTEN_CHUNK_BYTES: usize = 16 * 1024;
const MAX_DECOMPRESSED_CHUNK_BYTES: usize = 64 * 1024;
const MAX_DECOMPRESSED_REPLAY_BYTES: usize = 96 * 1024 * 1024;
const MAX_REPLAY_CHUNKS: usize = 6_144;

/// The longest in-game replay title observed in the complete local corpus.
/// Until the game's input widget limit is independently recovered, keeping the
/// writer inside this demonstrated range is preferable to producing a title the
/// original UI may truncate or reject.
pub const MAX_REPLAY_NAME_UTF16_UNITS: usize = 49;

#[derive(Debug)]
struct DecodedReplay {
    chunks: Vec<Vec<u8>>,
    stream: Vec<u8>,
}

#[derive(Debug)]
struct ReplayNameField {
    start: usize,
    end: usize,
    value: String,
}

fn read_u32(input: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        input.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn decode_replay(raw: &[u8]) -> Result<DecodedReplay, String> {
    if raw.len() > MAX_REPLAY_COMPRESSED_BYTES {
        return Err(format!(
            "Replay exceeds the {} MiB compressed size limit",
            MAX_REPLAY_COMPRESSED_BYTES / 1024 / 1024
        ));
    }
    if raw.get(..FILE_HEADER.len()) != Some(FILE_HEADER) {
        return Err("Replay does not have the BinTagFormat2 header".to_owned());
    }

    let mut cursor = FILE_HEADER.len();
    let mut chunks = Vec::new();
    let mut stream = Vec::new();
    while cursor < raw.len() {
        let compressed_size = usize::try_from(
            read_u32(raw, cursor)
                .ok_or_else(|| "Replay has a truncated compressed-length prefix".to_owned())?,
        )
        .map_err(|_| "Replay compressed chunk length is not representable".to_owned())?;
        cursor = cursor
            .checked_add(4)
            .ok_or_else(|| "Replay chunk offset overflow".to_owned())?;
        if compressed_size == 0 {
            return Err("Replay contains an empty compressed chunk".to_owned());
        }
        let end = cursor
            .checked_add(compressed_size)
            .ok_or_else(|| "Replay chunk length overflow".to_owned())?;
        let compressed = raw
            .get(cursor..end)
            .ok_or_else(|| "Replay has a truncated compressed chunk".to_owned())?;

        let mut decompressor = Decompress::new(true);
        let mut output = vec![0; MAX_DECOMPRESSED_CHUNK_BYTES];
        let status = decompressor
            .decompress(compressed, &mut output, FlushDecompress::Finish)
            .map_err(|error| format!("Replay contains an invalid zlib chunk: {error}"))?;
        if status != Status::StreamEnd
            || usize::try_from(decompressor.total_in()).ok() != Some(compressed.len())
        {
            return Err("Replay contains a truncated or overlong zlib chunk".to_owned());
        }
        let output_size = usize::try_from(decompressor.total_out())
            .map_err(|_| "Replay decompressed chunk length is not representable".to_owned())?;
        if output_size == 0 || output_size > MAX_DECOMPRESSED_CHUNK_BYTES {
            return Err("Replay contains an empty or oversized decompressed chunk".to_owned());
        }
        output.truncate(output_size);

        if chunks.len() >= MAX_REPLAY_CHUNKS {
            return Err(format!(
                "Replay exceeds the maximum of {MAX_REPLAY_CHUNKS} compressed chunks"
            ));
        }
        let next_size = stream
            .len()
            .checked_add(output.len())
            .ok_or_else(|| "Replay decompressed size overflow".to_owned())?;
        if next_size > MAX_DECOMPRESSED_REPLAY_BYTES {
            return Err(format!(
                "Replay exceeds the {} MiB decompressed size limit",
                MAX_DECOMPRESSED_REPLAY_BYTES / 1024 / 1024
            ));
        }
        stream.extend_from_slice(&output);
        chunks.push(output);
        cursor = end;
    }

    if chunks.is_empty() {
        return Err("Replay contains no compressed chunks".to_owned());
    }
    Ok(DecodedReplay { chunks, stream })
}

fn replay_name_field_at(metadata: &[u8], start: usize) -> Option<ReplayNameField> {
    if metadata.get(start..start.checked_add(4)?)? != REPLAY_NAME_HASH {
        return None;
    }
    let total_u32 = read_u32(metadata, start.checked_add(4)?)?;
    let total = usize::try_from(total_u32).ok()?;
    if total < FIELD_HEADER_BYTES + 2
        || metadata.get(start.checked_add(8)?) != Some(&STRING_FIELD_FLAG)
        || read_u32(metadata, start.checked_add(9)?)? != total_u32
    {
        return None;
    }
    let payload_start = start.checked_add(FIELD_HEADER_BYTES)?;
    let end = start.checked_add(total)?;
    let payload = metadata.get(payload_start..end)?;
    if payload.len() < 2 || !payload.len().is_multiple_of(2) || !payload.ends_with(&[0, 0]) {
        return None;
    }
    let units = payload[..payload.len() - 2]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    if units.is_empty() || units.contains(&0) {
        return None;
    }
    let value = String::from_utf16(&units).ok()?;
    if value.chars().any(char::is_control) {
        return None;
    }
    Some(ReplayNameField { start, end, value })
}

fn unique_replay_name_field(metadata: &[u8]) -> Result<ReplayNameField, String> {
    let mut fields = Vec::new();
    let mut cursor = 0;
    while cursor + REPLAY_NAME_HASH.len() <= metadata.len() {
        let Some(relative) = metadata[cursor..]
            .windows(REPLAY_NAME_HASH.len())
            .position(|candidate| candidate == REPLAY_NAME_HASH)
        else {
            break;
        };
        let start = cursor + relative;
        if let Some(field) = replay_name_field_at(metadata, start) {
            fields.push(field);
        }
        cursor = start + 1;
    }
    match fields.len() {
        0 => Err("Replay does not contain an editable in-game name".to_owned()),
        1 => Ok(fields.remove(0)),
        count => Err(format!(
            "Replay contains {count} valid in-game name fields; refusing an ambiguous edit"
        )),
    }
}

fn validated_name_units(name: &str) -> Result<Vec<u16>, String> {
    if name.is_empty() {
        return Err("In-game name cannot be empty".to_owned());
    }
    if name.trim() != name {
        return Err("In-game name cannot begin or end with whitespace".to_owned());
    }
    if name.chars().any(char::is_control) {
        return Err("In-game name cannot contain control characters".to_owned());
    }
    let units = name.encode_utf16().collect::<Vec<_>>();
    if units.len() > MAX_REPLAY_NAME_UTF16_UNITS {
        return Err(format!(
            "In-game name is {} UTF-16 units; the verified maximum is {MAX_REPLAY_NAME_UTF16_UNITS}",
            units.len()
        ));
    }
    Ok(units)
}

fn encode_stream(stream: &[u8]) -> Result<Vec<u8>, String> {
    let mut raw = Vec::with_capacity(stream.len() / 2);
    raw.extend_from_slice(FILE_HEADER);
    for chunk in stream.chunks(WRITTEN_CHUNK_BYTES) {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder
            .write_all(chunk)
            .map_err(|error| format!("Cannot compress replay chunk: {error}"))?;
        let compressed = encoder
            .finish()
            .map_err(|error| format!("Cannot finish replay chunk: {error}"))?;
        let length = u32::try_from(compressed.len())
            .map_err(|_| "Compressed replay chunk is too large".to_owned())?;
        raw.extend_from_slice(&length.to_le_bytes());
        raw.extend_from_slice(&compressed);
    }
    if raw.len() > MAX_REPLAY_COMPRESSED_BYTES {
        return Err(format!(
            "Edited replay exceeds the {} MiB compressed size limit",
            MAX_REPLAY_COMPRESSED_BYTES / 1024 / 1024
        ));
    }
    Ok(raw)
}

/// Return a complete replay whose only decompressed-stream change is the exact
/// `ReplayName` field. The input slice is never modified.
pub fn rewrite_replay_name(raw: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let name_units = validated_name_units(name)?;
    let decoded = decode_replay(raw)?;
    let metadata = decoded
        .chunks
        .first()
        .ok_or_else(|| "Replay metadata chunk is missing".to_owned())?;
    let field = unique_replay_name_field(metadata)?;
    if field.value == name {
        return Ok(raw.to_vec());
    }

    let mut payload = Vec::with_capacity((name_units.len() + 1) * 2);
    for unit in name_units {
        payload.extend_from_slice(&unit.to_le_bytes());
    }
    payload.extend_from_slice(&[0, 0]);
    let total = FIELD_HEADER_BYTES
        .checked_add(payload.len())
        .ok_or_else(|| "In-game name field length overflow".to_owned())?;
    let total_u32 =
        u32::try_from(total).map_err(|_| "In-game name field is too large".to_owned())?;

    let mut replacement = Vec::with_capacity(total);
    replacement.extend_from_slice(&REPLAY_NAME_HASH);
    replacement.extend_from_slice(&total_u32.to_le_bytes());
    replacement.push(STRING_FIELD_FLAG);
    replacement.extend_from_slice(&total_u32.to_le_bytes());
    replacement.extend_from_slice(&payload);

    let mut intended = Vec::with_capacity(decoded.stream.len() - (field.end - field.start) + total);
    intended.extend_from_slice(&decoded.stream[..field.start]);
    intended.extend_from_slice(&replacement);
    intended.extend_from_slice(&decoded.stream[field.end..]);

    let rewritten = encode_stream(&intended)?;
    let verified = decode_replay(&rewritten)?;
    if verified.stream != intended {
        return Err("Edited replay failed its decompressed byte-for-byte verification".to_owned());
    }
    let parsed = WicReplayParser::from_bytes(&rewritten)
        .map_err(|error| format!("Edited replay failed parser validation: {error}"))?;
    if parsed.parse().game_info.replay_name.as_deref() != Some(name) {
        return Err("Edited replay did not retain the requested in-game name".to_owned());
    }
    Ok(rewritten)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEAM_WINS_HASH: [u8; 4] = [0x29, 0x03, 0xb8, 0x0d];
    const EVENT_HASH: [u8; 4] = [0x03, 0x02, 0xb5, 0x05];

    fn replay_name_field(name: &str) -> Vec<u8> {
        let mut payload = name
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        payload.extend_from_slice(&[0, 0]);
        let total = u32::try_from(FIELD_HEADER_BYTES + payload.len()).expect("fixture length");
        let mut field = Vec::new();
        field.extend_from_slice(&REPLAY_NAME_HASH);
        field.extend_from_slice(&total.to_le_bytes());
        field.push(STRING_FIELD_FLAG);
        field.extend_from_slice(&total.to_le_bytes());
        field.extend_from_slice(&payload);
        field
    }

    fn fixture(fields: &[&str]) -> Vec<u8> {
        let mut stream = vec![0; 47];
        stream.extend_from_slice(b"maps/ustown1/ustown1.ice\0");
        for name in fields {
            stream.extend_from_slice(&replay_name_field(name));
        }
        stream.resize(WRITTEN_CHUNK_BYTES + 257, 0x42);
        let total = 21u32;
        stream.extend_from_slice(&EVENT_HASH);
        stream.extend_from_slice(&[0x15, 0, 0, 0]);
        stream.push(0x06);
        stream.extend_from_slice(&total.to_le_bytes());
        stream.extend_from_slice(&0f32.to_bits().to_le_bytes());
        stream.extend_from_slice(&TEAM_WINS_HASH);
        encode_stream(&stream).expect("fixture replay")
    }

    fn field_and_stream(raw: &[u8]) -> (ReplayNameField, Vec<u8>, Vec<Vec<u8>>) {
        let decoded = decode_replay(raw).expect("decode fixture");
        let field = unique_replay_name_field(&decoded.chunks[0]).expect("name field");
        (field, decoded.stream, decoded.chunks)
    }

    #[test]
    fn shorter_longer_and_unicode_names_change_only_the_structural_field() {
        let source = fixture(&["demo01"]);
        let (old_field, old_stream, _) = field_and_stream(&source);
        for name in ["A", "Riviera comeback", "Åland 😊"] {
            let rewritten = rewrite_replay_name(&source, name).expect("rewrite");
            let (new_field, new_stream, chunks) = field_and_stream(&rewritten);
            assert_eq!(new_field.value, name);
            assert_eq!(
                &old_stream[..old_field.start],
                &new_stream[..new_field.start]
            );
            assert_eq!(&old_stream[old_field.end..], &new_stream[new_field.end..]);
            assert!(
                chunks[..chunks.len() - 1]
                    .iter()
                    .all(|chunk| chunk.len() == WRITTEN_CHUNK_BYTES)
            );
            assert!(chunks.last().is_some_and(|chunk| !chunk.is_empty()));
        }
        assert_eq!(
            unique_replay_name_field(&decode_replay(&source).expect("source").chunks[0])
                .expect("source field")
                .value,
            "demo01"
        );
    }

    #[test]
    fn missing_and_duplicate_fields_are_refused() {
        assert!(
            rewrite_replay_name(&fixture(&[]), "New name")
                .expect_err("missing field")
                .contains("does not contain")
        );
        assert!(
            rewrite_replay_name(&fixture(&["one", "two"]), "New name")
                .expect_err("duplicate fields")
                .contains("ambiguous")
        );
    }

    #[test]
    fn invalid_names_are_refused() {
        let source = fixture(&["demo01"]);
        for name in ["", " leading", "trailing ", "line\nbreak"] {
            assert!(rewrite_replay_name(&source, name).is_err(), "{name:?}");
        }
        let too_long = "x".repeat(MAX_REPLAY_NAME_UTF16_UNITS + 1);
        assert!(rewrite_replay_name(&source, &too_long).is_err());
        let maximum = "x".repeat(MAX_REPLAY_NAME_UTF16_UNITS);
        assert!(rewrite_replay_name(&source, &maximum).is_ok());
    }

    #[test]
    fn corrupt_chunk_framing_is_refused() {
        let mut source = fixture(&["demo01"]);
        source.truncate(source.len() - 1);
        assert!(rewrite_replay_name(&source, "New name").is_err());
    }
}
