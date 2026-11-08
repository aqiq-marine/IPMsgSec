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
    let first_nul = data.iter().position(|byte| *byte == 0);
    let (legacy_nickname, legacy_group, metadata) = match first_nul {
        None => (if data.is_empty() { None } else { Some(data) }, None, None),
        Some(nul) => {
            let tail = &data[nul + 1..];
            if let Some(marker) = tail.windows(2).position(|window| window == b"\0\n") {
                let group = &tail[..marker];
                let metadata_start = nul + 1 + marker + 2;
                (Some(&data[..nul]), Some(group), Some(&data[metadata_start..]))
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
        while !rest.is_empty() {
            let newline = match rest.iter().position(|byte| *byte == b'\n') {
                Some(index) => index,
                None => return Err(DiscoveryDataError::MalformedExtension),
            };
            let line = &rest[..newline];
            rest = &rest[newline + 1..];
            if line.is_empty() {
                continue;
            }
            let text = core::str::from_utf8(line)
                .map_err(|_| DiscoveryDataError::InvalidUtf8)?;
            if let Some(value) = text.strip_prefix("UN:") {
                if username.is_some() { return Err(DiscoveryDataError::DuplicateField); }
                username = Some(value.as_bytes());
            } else if let Some(value) = text.strip_prefix("HN:") {
                if hostname.is_some() { return Err(DiscoveryDataError::DuplicateField); }
                hostname = Some(value.as_bytes());
            } else if let Some(value) = text.strip_prefix("NN:") {
                if nickname.is_some() { return Err(DiscoveryDataError::DuplicateField); }
                nickname = Some(value.as_bytes());
            } else if let Some(value) = text.strip_prefix("GN:") {
                if group.is_some() { return Err(DiscoveryDataError::DuplicateField); }
                group = Some(value.as_bytes());
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
        ).unwrap();
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
