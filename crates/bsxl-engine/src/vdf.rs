//! Lossless reading and writing of Steam's binary `shortcuts.vdf`.
//!
//! `steam_shortcuts_util` only knows a fixed set of keys and drops the rest (Steam also
//! writes keys like `sortas` and `FlatpakAppID`). BoilerSteamXL keeps every record as its
//! raw key/value list instead: records it does not change are written back byte-for-byte,
//! changed records only have the changed keys replaced.
//!
//! Format: `00 "shortcuts" 00`, then per shortcut `00 "<index>" 00 <fields> 08`, then
//! `08 08`. A field is `<type> name 00 <value>`: type 01 string (value + 00), 02 int32,
//! 03 float32, 07 uint64 (raw bytes), 00 nested map (fields + 08).

use steam_shortcuts_util::{shortcut::ShortcutOwned, shortcuts_to_bytes};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub kind: u8,
    pub name: Vec<u8>,
    /// Raw payload: string bytes without the terminating NUL, the fixed-size number, or
    /// a nested map's fields including its closing 08.
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    pub fields: Vec<Field>,
}

fn err(pos: usize, what: &str) -> String {
    format!("unexpected data at byte {pos}: {what}")
}

fn read_cstr(b: &[u8], pos: &mut usize) -> Result<Vec<u8>, String> {
    let start = *pos;
    let len = b.get(start..).and_then(|rest| rest.iter().position(|c| *c == 0)).ok_or_else(|| err(start, "unterminated string"))?;
    *pos = start + len + 1;
    Ok(b.get(start..start + len).unwrap_or_default().to_vec())
}

/// Reads one field's value starting at `pos` for a field of type `kind`.
fn read_value(b: &[u8], pos: &mut usize, kind: u8) -> Result<Vec<u8>, String> {
    let start = *pos;
    let fixed = |n: usize, pos: &mut usize| -> Result<Vec<u8>, String> {
        let v = b.get(start..start + n).ok_or_else(|| err(start, "truncated number"))?.to_vec();
        *pos = start + n;
        Ok(v)
    };
    match kind {
        0x01 => read_cstr(b, pos),
        0x02 | 0x03 => fixed(4, pos),
        0x07 => fixed(8, pos),
        0x00 => {
            read_fields(b, pos)?;
            Ok(b.get(start..*pos).unwrap_or_default().to_vec())
        }
        other => Err(err(start, &format!("unsupported field type {other:#04x}"))),
    }
}

/// Reads fields until the closing 08 of the current map (consumed).
fn read_fields(b: &[u8], pos: &mut usize) -> Result<Vec<Field>, String> {
    let mut fields = Vec::new();
    loop {
        let kind = *b.get(*pos).ok_or_else(|| err(*pos, "missing end of map"))?;
        *pos += 1;
        if kind == 0x08 {
            return Ok(fields);
        }
        let name = read_cstr(b, pos)?;
        let value = read_value(b, pos, kind)?;
        fields.push(Field { kind, name, value });
    }
}

/// Splits a shortcuts.vdf into records. An empty or missing file has no records.
pub fn parse(bytes: &[u8]) -> Result<Vec<Record>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut pos = 0;
    if bytes.first() != Some(&0x00) {
        return Err(err(0, "not a shortcuts file"));
    }
    pos += 1;
    let root = read_cstr(bytes, &mut pos)?;
    if !root.eq_ignore_ascii_case(b"shortcuts") {
        return Err(err(1, "not a shortcuts file"));
    }
    let mut records = Vec::new();
    loop {
        let kind = *bytes.get(pos).ok_or_else(|| err(pos, "missing end of shortcuts"))?;
        pos += 1;
        match kind {
            0x08 => break,
            0x00 => {
                let _index = read_cstr(bytes, &mut pos)?;
                records.push(Record { fields: read_fields(bytes, &mut pos)? });
            }
            _ => return Err(err(pos - 1, "expected a shortcut")),
        }
    }
    Ok(records)
}

pub fn serialize(records: &[Record]) -> Vec<u8> {
    let mut out = vec![0x00];
    out.extend_from_slice(b"shortcuts");
    out.push(0x00);
    for (index, record) in records.iter().enumerate() {
        out.push(0x00);
        out.extend_from_slice(index.to_string().as_bytes());
        out.push(0x00);
        for f in &record.fields {
            out.push(f.kind);
            out.extend_from_slice(&f.name);
            out.push(0x00);
            out.extend_from_slice(&f.value);
            if f.kind == 0x01 {
                out.push(0x00);
            }
        }
        out.push(0x08);
    }
    out.extend_from_slice(&[0x08, 0x08]);
    out
}

impl Record {
    fn find(&self, key: &str) -> Option<usize> {
        self.fields.iter().position(|f| f.name.eq_ignore_ascii_case(key.as_bytes()))
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        let f = self.fields.get(self.find(key)?)?;
        (f.kind == 0x01).then(|| String::from_utf8_lossy(&f.value).to_string())
    }

    pub fn app_id(&self) -> Option<u32> {
        let f = self.fields.get(self.find("appid")?)?;
        let bytes: [u8; 4] = f.value.as_slice().try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    /// Replaces a 32-bit integer field, or appends it.
    pub fn set_u32(&mut self, key: &str, value: u32) {
        let value = value.to_le_bytes().to_vec();
        match self.find(key) {
            Some(i) => {
                if let Some(f) = self.fields.get_mut(i) {
                    f.kind = 0x02;
                    f.value = value;
                }
            }
            None => self.fields.push(Field { kind: 0x02, name: key.as_bytes().to_vec(), value }),
        }
    }

    /// Replaces a string field (keeping its position and spelling), or appends it.
    pub fn set_str(&mut self, key: &str, value: &str) {
        let value = value.replace('\0', "").into_bytes();
        match self.find(key) {
            Some(i) => {
                if let Some(f) = self.fields.get_mut(i) {
                    f.kind = 0x01;
                    f.value = value;
                }
            }
            None => self.fields.push(Field { kind: 0x01, name: key.as_bytes().to_vec(), value }),
        }
    }

    /// A record for a shortcut, using the library's field layout.
    pub fn from_shortcut(shortcut: &ShortcutOwned) -> Record {
        let bytes = shortcuts_to_bytes(&vec![shortcut.borrow()]);
        parse(&bytes).ok().and_then(|mut r| r.pop()).unwrap_or_default()
    }

    /// Copies a field (any type) from another record, replacing ours.
    pub fn copy_field(&mut self, from: &Record, key: &str) {
        let Some(field) = from.find(key).and_then(|i| from.fields.get(i)).cloned() else { return };
        match self.find(key) {
            Some(i) => {
                if let Some(f) = self.fields.get_mut(i) {
                    *f = field;
                }
            }
            None => self.fields.push(field),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steam_shortcuts_util::{parse_shortcuts, Shortcut};

    fn sample() -> Vec<u8> {
        let mut a = Shortcut::new("0", "Game A", "\"C:\\a.exe\"", "\"C:\\\"", "", "", "-x").to_owned();
        a.tags = vec!["one".into(), "two".into()];
        let b = Shortcut::new("1", "Game B", "\"C:\\b.exe\"", "", "", "", "").to_owned();
        shortcuts_to_bytes(&vec![a.borrow(), b.borrow()])
    }

    #[test]
    fn round_trips_byte_for_byte() {
        let bytes = sample();
        let records = parse(&bytes).unwrap_or_default();
        assert_eq!(records.len(), 2);
        assert_eq!(serialize(&records), bytes);
    }

    #[test]
    fn keeps_unknown_keys_and_edits_only_what_changes() {
        let mut records = parse(&sample()).unwrap_or_default();
        if let Some(r) = records.get_mut(0) {
            r.fields.push(Field { kind: 0x01, name: b"sortas".to_vec(), value: b"Alpha".to_vec() });
        }
        // 64-bit values round-trip in the lossless layer too.
        let mut wide = records.clone();
        if let Some(r) = wide.get_mut(1) {
            r.fields.push(Field { kind: 0x07, name: b"Something64".to_vec(), value: vec![1, 2, 3, 4, 5, 6, 7, 8] });
        }
        assert_eq!(parse(&serialize(&wide)).unwrap_or_default(), wide);
        let bytes = serialize(&records);
        let mut again = parse(&bytes).unwrap_or_default();
        assert_eq!(again.first().and_then(|r| r.get_str("sortas")).as_deref(), Some("Alpha"));
        if let Some(r) = again.get_mut(0) {
            r.set_str("Exe", "\"D:\\new.exe\"");
        }
        let edited = serialize(&again);
        let parsed = parse_shortcuts(&edited).unwrap_or_default();
        assert_eq!(parsed.first().map(|s| s.exe), Some("\"D:\\new.exe\""));
        assert_eq!(parsed.first().map(|s| s.tags.len()), Some(2));
        assert_eq!(parse(&edited).unwrap_or_default().first().and_then(|r| r.get_str("sortas")).as_deref(), Some("Alpha"));
    }

    #[test]
    fn rejects_garbage_and_accepts_empty() {
        assert!(parse(b"definitely not a shortcuts file").is_err());
        assert!(parse(&[]).unwrap_or_default().is_empty());
        assert!(parse(&serialize(&[])).unwrap_or_default().is_empty());
        let truncated = sample();
        assert!(parse(truncated.get(..truncated.len() - 5).unwrap_or_default()).is_err());
    }

    #[test]
    fn record_from_shortcut_matches_library_output() {
        let s = Shortcut::new("0", "X", "\"C:\\x.exe\"", "", "", "", "").to_owned();
        let r = Record::from_shortcut(&s);
        assert_eq!(r.app_id(), Some(s.app_id));
        assert_eq!(r.get_str("AppName").as_deref(), Some("X"));
    }
}
