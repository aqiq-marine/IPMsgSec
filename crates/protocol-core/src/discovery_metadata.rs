//! Bounded parser for the BR-family legacy and UTF-8 metadata extension.
//!
//! The common packet parser leaves this command-specific area opaque. This
//! module interprets only the documented BR_ENTRY/ANSENTRY/BR_ABSENCE layout.

use vstd::prelude::*;

verus! {

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscoveryData<'a> {
    /// Legacy nickname bytes before the first NUL, if present.
    pub legacy_nickname: Option<&'a [u8]>,
    /// Legacy group bytes between the first NUL and the UTF-8 marker.
    pub legacy_group: Option<&'a [u8]>,
    /// UTF-8 metadata values. These override their corresponding legacy fields.
    pub username: Option<&'a [u8]>,
    pub hostname: Option<&'a [u8]>,
    pub nickname: Option<&'a [u8]>,
    pub group: Option<&'a [u8]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryDataError {
    MalformedExtension,
    InvalidUtf8,
    DuplicateField,
}


/// Validate UTF-8 directly over bytes so this parser remains inside Verus'
/// supported primitive model. This checks overlong encodings, surrogates,
/// truncated sequences, and code points above U+10FFFF.
#[allow(clippy::manual_range_contains)] // Explicit comparisons keep this verifier-friendly.
fn is_valid_utf8(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len()
        invariant i <= bytes.len(),
        decreases bytes.len() - i,
    {
        let b0 = bytes[i];
        if b0 <= 0x7f {
            i += 1;
        } else if b0 >= 0xc2 && b0 <= 0xdf {
            if bytes.len() - i < 2 { return false; }
            let b1 = bytes[i + 1];
            if b1 < 0x80 || b1 > 0xbf { return false; }
            i += 2;
        } else if b0 == 0xe0 {
            if bytes.len() - i < 3 { return false; }
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];
            if b1 < 0xa0 || b1 > 0xbf || b2 < 0x80 || b2 > 0xbf { return false; }
            i += 3;
        } else if (b0 >= 0xe1 && b0 <= 0xec) || (b0 >= 0xee && b0 <= 0xef) {
            if bytes.len() - i < 3 { return false; }
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];
            if b1 < 0x80 || b1 > 0xbf || b2 < 0x80 || b2 > 0xbf { return false; }
            i += 3;
        } else if b0 == 0xed {
            if bytes.len() - i < 3 { return false; }
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];
            if b1 < 0x80 || b1 > 0x9f || b2 < 0x80 || b2 > 0xbf { return false; }
            i += 3;
        } else if b0 == 0xf0 {
            if bytes.len() - i < 4 { return false; }
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];
            let b3 = bytes[i + 3];
            if b1 < 0x90 || b1 > 0xbf || b2 < 0x80 || b2 > 0xbf || b3 < 0x80 || b3 > 0xbf { return false; }
            i += 4;
        } else if b0 >= 0xf1 && b0 <= 0xf3 {
            if bytes.len() - i < 4 { return false; }
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];
            let b3 = bytes[i + 3];
            if b1 < 0x80 || b1 > 0xbf || b2 < 0x80 || b2 > 0xbf || b3 < 0x80 || b3 > 0xbf { return false; }
            i += 4;
        } else if b0 == 0xf4 {
            if bytes.len() - i < 4 { return false; }
            let b1 = bytes[i + 1];
            let b2 = bytes[i + 2];
            let b3 = bytes[i + 3];
            if b1 < 0x80 || b1 > 0x8f || b2 < 0x80 || b2 > 0xbf || b3 < 0x80 || b3 > 0xbf { return false; }
            i += 4;
        } else {
            return false;
        }
    }
    true
}

/// Parse the command-specific discovery additional-data area.
///
/// Legacy layout: nickname [NUL group]
/// UTF-8 layout: nickname NUL group NUL LF followed by UN/HN/NN/GN lines.
/// With no group, the required dummy empty group produces NUL NUL LF.
///
/// The returned slices borrow the packet. Unknown metadata keys are ignored;
/// known keys may occur at most once. The UTF-8 metadata takes precedence over
/// the legacy header/additional-data values.
pub fn parse_discovery_data(data: &[u8]) -> Result<DiscoveryData<'_>, DiscoveryDataError> {
    let mut first_nul: Option<usize> = None;
    let mut scan: usize = 0;
    while scan < data.len()
        invariant scan <= data.len(),
        decreases data.len() - scan,
    {
        if data[scan] == 0 {
            first_nul = Some(scan);
            break;
        }
        scan += 1;
    }
    let (legacy_nickname, legacy_group, metadata) = match first_nul {
        None => (if data.is_empty() { None } else { Some(data) }, None, None),
        Some(nul) => {
            assert(nul < data.len());
            let after_nul = nul + 1;
            let tail = &data[after_nul..];
            let mut marker = 0;
            while marker < tail.len()
                && tail.len() - marker >= 2
                && !(tail[marker] == 0 && tail[marker + 1] == b'\n')
                invariant marker <= tail.len(),
                decreases tail.len() - marker,
            {
                marker += 1;
            }
            if tail.len() - marker >= 2 {
                let group = &tail[..marker];
                (Some(&data[..nul]), Some(group), Some(&tail[marker + 2..]))
            } else {
                (Some(&data[..nul]), Some(tail), None)
            }
        }
    };

    let mut username = None;
    let mut hostname = None;
    let mut nickname = None;
    let mut group = None;

    if let Some(extension) = metadata {
        let mut rest = extension;
        while !rest.is_empty()
            decreases rest.len(),
        {
            let newline = match rest.iter().position(|byte| *byte == b'\n') {
                Some(index) => index,
                None => return Err(DiscoveryDataError::MalformedExtension),
            };
            let line = &rest[..newline];
            rest = &rest[newline + 1..];
            if line.is_empty() {
                continue;
            }
            if !is_valid_utf8(line) {
                return Err(DiscoveryDataError::InvalidUtf8);
            }
            if line.len() >= 3 && line[0] == b'U' && line[1] == b'N' && line[2] == b':' {
                if username.is_some() { return Err(DiscoveryDataError::DuplicateField); }
                username = Some(&line[3..]);
            } else if line.len() >= 3 && line[0] == b'H' && line[1] == b'N' && line[2] == b':' {
                if hostname.is_some() { return Err(DiscoveryDataError::DuplicateField); }
                hostname = Some(&line[3..]);
            } else if line.len() >= 3 && line[0] == b'N' && line[1] == b'N' && line[2] == b':' {
                if nickname.is_some() { return Err(DiscoveryDataError::DuplicateField); }
                nickname = Some(&line[3..]);
            } else if line.len() >= 3 && line[0] == b'G' && line[1] == b'N' && line[2] == b':' {
                if group.is_some() { return Err(DiscoveryDataError::DuplicateField); }
                group = Some(&line[3..]);
            }
        }
    }

    Ok(DiscoveryData {
        legacy_nickname,
        legacy_group,
        username,
        hostname,
        nickname,
        group,
    })
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_nickname_without_group() {
        let parsed = parse_discovery_data(b"alice").unwrap();
        assert_eq!(parsed.legacy_nickname, Some(b"alice".as_slice()));
        assert_eq!(parsed.legacy_group, None);
        assert_eq!(parsed.username, None);
    }

    #[test]
    fn parses_legacy_nickname_and_group() {
        let parsed = parse_discovery_data(b"alice\0chemistry").unwrap();
        assert_eq!(parsed.legacy_nickname, Some(b"alice".as_slice()));
        assert_eq!(parsed.legacy_group, Some(b"chemistry".as_slice()));
    }

    #[test]
    fn parses_utf8_metadata_with_group() {
        let parsed = parse_discovery_data(
            "legacy\0old-group\0\nUN:ユーザー\nHN:ホスト\nNN:表示名\nGN:新グループ\n".as_bytes(),
        )
        .unwrap();
        assert_eq!(parsed.username, Some("ユーザー".as_bytes()));
        assert_eq!(parsed.hostname, Some("ホスト".as_bytes()));
        assert_eq!(parsed.nickname, Some("表示名".as_bytes()));
        assert_eq!(parsed.group, Some("新グループ".as_bytes()));
        assert_eq!(parsed.legacy_group, Some(b"old-group".as_slice()));
    }

    #[test]
    fn parses_utf8_metadata_without_group_using_dummy_nul() {
        let parsed = parse_discovery_data(b"legacy\0\0\nUN:alice\nHN:pc\n").unwrap();
        assert_eq!(parsed.legacy_group, Some(b"".as_slice()));
        assert_eq!(parsed.username, Some(b"alice".as_slice()));
        assert_eq!(parsed.hostname, Some(b"pc".as_slice()));
    }

    #[test]
    fn rejects_malformed_or_duplicate_metadata() {
        assert_eq!(
            parse_discovery_data(b"n\0g\0\nUN:alice"),
            Err(DiscoveryDataError::MalformedExtension)
        );
        assert_eq!(
            parse_discovery_data(b"n\0g\0\nUN:a\nUN:b\n"),
            Err(DiscoveryDataError::DuplicateField)
        );
        assert_eq!(
            parse_discovery_data(b"n\0g\0\nUN:\xff\n"),
            Err(DiscoveryDataError::InvalidUtf8)
        );
    }
}
