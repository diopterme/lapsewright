use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Writes `com.apple.quicktime.*` keys where Apple reads them: a `meta` box (handler `mdta`)
/// directly in `moov`. ffmpeg can only put them in `udta/meta`, which AVFoundation ignores.
///
/// The file must have `moov` followed by a `free` box with room for the keys, which ffmpeg
/// leaves with `-moov_size`. The box grows into that room, so nothing after it moves.
pub fn add_keys(path: &Path, keys: &[(&str, String)]) -> io::Result<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    insert_meta(&mut file, &meta_box(keys))
}

fn insert_meta(file: &mut (impl Read + Write + Seek), meta: &[u8]) -> io::Result<()> {
    let (moov_start, moov_size) = find_moov(file)?;
    let free_start = moov_start + moov_size;
    let (kind, free_size) = header(file, free_start)?;
    // A shrunk `free` still needs its own 8-byte header.
    if &kind != b"free" || free_size < meta.len() as u64 + 8 {
        return Err(io::Error::other("no room reserved after moov for the metadata keys"));
    }
    let new_moov = u32::try_from(moov_size + meta.len() as u64).map_err(|_| io::Error::other("moov too large"))?;
    let rest = u32::try_from(free_size - meta.len() as u64).map_err(|_| io::Error::other("free too large"))?;
    // Order keeps the file valid if writing stops midway: the new `free` header lands inside the
    // old `free`, then `meta` sits as a top-level box, and only the last write pulls it into `moov`.
    file.seek(SeekFrom::Start(free_start + meta.len() as u64))?;
    file.write_all(&[&rest.to_be_bytes()[..], b"free"].concat())?;
    file.seek(SeekFrom::Start(free_start))?;
    file.write_all(meta)?;
    file.seek(SeekFrom::Start(moov_start))?;
    file.write_all(&new_moov.to_be_bytes())
}

/// Start and size of the top-level `moov`. Only 32-bit sizes: with `-moov_size` ffmpeg writes
/// `moov` right after `ftyp`, so no large `mdat` comes before it.
fn find_moov(file: &mut (impl Read + Seek)) -> io::Result<(u64, u64)> {
    let mut at = 0;
    loop {
        match header(file, at)? {
            (kind, size) if &kind == b"moov" => return Ok((at, size)),
            (_, size) => at += size,
        }
    }
}

fn header(file: &mut (impl Read + Seek), at: u64) -> io::Result<([u8; 4], u64)> {
    file.seek(SeekFrom::Start(at))?;
    let mut bytes = [0u8; 8];
    file.read_exact(&mut bytes)?;
    let size = u64::from(u32::from_be_bytes(bytes[..4].try_into().expect("4 bytes")));
    if size < 8 {
        return Err(io::Error::other("unsupported box size before moov"));
    }
    Ok((bytes[4..].try_into().expect("4 bytes"), size))
}

/// Layout as QuickTime defines it (and exiftool writes it): `meta` without version/flags,
/// `hdlr` naming `mdta`, `keys` listing the names, `ilst` holding UTF-8 `data` per key index.
fn meta_box(keys: &[(&str, String)]) -> Vec<u8> {
    let hdlr = boxed(b"hdlr", &[&[0u8; 8][..], b"mdta", &[0; 12]].concat());
    let names: Vec<u8> = keys.iter().flat_map(|(name, _)| boxed(b"mdta", name.as_bytes())).collect();
    let keys_box = boxed(b"keys", &[&[0; 4], &(keys.len() as u32).to_be_bytes()[..], &names].concat());
    let items: Vec<u8> = (1u32..)
        .zip(keys)
        .flat_map(|(index, (_, value))| {
            // Type 1 = UTF-8, locale 0 = any.
            let data = boxed(b"data", &[&1u32.to_be_bytes()[..], &[0; 4], value.as_bytes()].concat());
            boxed(&index.to_be_bytes(), &data)
        })
        .collect();
    boxed(b"meta", &[hdlr, keys_box, boxed(b"ilst", &items)].concat())
}

fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [&((body.len() + 8) as u32).to_be_bytes()[..], kind, body].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn file(free: usize) -> Vec<u8> {
        [boxed(b"ftyp", b"qt  "), boxed(b"moov", &boxed(b"mvhd", &[7; 12])), boxed(b"free", &vec![0; free - 8]), boxed(b"mdat", &[9; 20])]
            .concat()
    }

    #[test]
    fn meta_box_matches_the_layout_apple_reads() {
        // Bytes exiftool 13.55 wrote for this key; AVFoundation reads them back.
        let expected = b"\x00\x00\x00\x98meta\
            \x00\x00\x00\x20hdlr\x00\x00\x00\x00\x00\x00\x00\x00mdta\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\
            \x00\x00\x00\x38keys\x00\x00\x00\x00\x00\x00\x00\x01\
            \x00\x00\x00\x28mdtacom.apple.quicktime.creationdate\
            \x00\x00\x00\x38ilst\x00\x00\x00\x30\x00\x00\x00\x01\
            \x00\x00\x00\x28data\x00\x00\x00\x01\x00\x00\x00\x002026-06-01T14:22:10+0900";
        let meta = meta_box(&[("com.apple.quicktime.creationdate", "2026-06-01T14:22:10+0900".into())]);
        assert_eq!(meta, expected);
    }

    #[test]
    fn meta_grows_into_free_and_nothing_after_moves() {
        let before = file(400);
        let meta = meta_box(&[("com.apple.quicktime.camera.lens_model", "FE 24mm F1.4 GM".into())]);
        let mut cursor = Cursor::new(before.clone());
        insert_meta(&mut cursor, &meta).unwrap();
        let after = cursor.into_inner();

        assert_eq!(after.len(), before.len());
        let (moov_start, moov_end) = (12, 12 + 28);
        assert_eq!(&after[moov_start..moov_start + 4], &((28 + meta.len()) as u32).to_be_bytes());
        assert_eq!(&after[moov_start + 4..moov_end], &before[moov_start + 4..moov_end], "existing moov children kept");
        assert_eq!(&after[moov_end..moov_end + meta.len()], &meta[..]);
        let free_at = moov_end + meta.len();
        assert_eq!(&after[free_at..free_at + 8], &[&((400 - meta.len()) as u32).to_be_bytes()[..], b"free"].concat());
        assert_eq!(&after[moov_end + 400..], &before[moov_end + 400..], "mdat untouched");
    }

    #[test]
    fn too_little_room_fails_without_writing() {
        let before = file(100);
        let meta = meta_box(&[("com.apple.quicktime.camera.lens_model", "FE 24mm F1.4 GM".into())]);
        let mut cursor = Cursor::new(before.clone());
        assert!(insert_meta(&mut cursor, &meta).is_err());
        assert_eq!(cursor.into_inner(), before);
    }

    #[test]
    fn missing_free_after_moov_fails() {
        let before = [boxed(b"ftyp", b"qt  "), boxed(b"moov", &[0; 8]), boxed(b"mdat", &[9; 20])].concat();
        assert!(insert_meta(&mut Cursor::new(before), &meta_box(&[("k", "v".into())])).is_err());
    }
}
