//! Pure, bounded parser for the IP Messenger text header.
//!
//! The first five ':' delimiters separate the fixed header fields. Everything
//! after the fifth delimiter is opaque command-specific data; in particular,
//! colons in a message body are preserved.

use vstd::prelude::*;
use crate::{BoundedDatagram, MAX_PACKET_SIZE};

verus! {

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandKind {
    BroadcastEntry,
    BroadcastExit,
    AnswerEntry,
    SendMessage,
    Unsupported(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    PacketTooLarge,
    MissingDelimiter,
    EmptyIdentityField,
    InvalidDecimal,
    NumericOverflow,
    UnsupportedVersion(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Packet<'a> {
    pub version: u32,
    pub packet_number: u64,
    pub username: &'a [u8],
    pub hostname: &'a [u8],
    /// The complete 32-bit command word as transmitted.
    pub command_word: u32,
    /// The low eight bits select the command.
    pub command: CommandKind,
    /// The upper 24 bits are option flags.
    pub option_flags: u32,
    /// Opaque bytes after the fifth delimiter. May contain ':'.
    pub additional_data: &'a [u8],
}

/// Find the next ':' at or after start.
///
/// A missing delimiter is represented as None; no index is fabricated.
fn find_delimiter(bytes: &[u8], start: usize) -> (result: Option<usize>)
    requires
        start <= bytes.len(),
    ensures
        match result {
            Some(index) => start <= index && index < bytes.len() && bytes[index] == b':',
            None => true,
        }
{
    let mut index = start;
    while index < bytes.len()
        invariant
            start <= index,
            index <= bytes.len(),
        decreases
            bytes.len() - index,
    {
        if bytes[index] == b':' {
            return Some(index);
        }
        index = index + 1;
    }
    None
}

/// Parse a non-empty sequence of ASCII decimal digits as u64.
/// Overflow is detected before multiplication/addition.
fn parse_decimal(digits: &[u8]) -> (result: Result<u64, ParseError>)
{
    if digits.len() == 0 {
        return Err(ParseError::InvalidDecimal);
    }

    let mut index: usize = 0;
    let mut value: u64 = 0;
    while index < digits.len()
        invariant
            index <= digits.len(),
        decreases
            digits.len() - index,
    {
        let byte = digits[index];
        if byte < b'0' || byte > b'9' {
            return Err(ParseError::InvalidDecimal);
        }

        let digit = (byte - b'0') as u64;
        if value > (u64::MAX - digit) / 10 {
            return Err(ParseError::NumericOverflow);
        }

        value = value * 10 + digit;
        index = index + 1;
    }

    Ok(value)
}

/// Parse a bounded IP Messenger datagram.
///
/// This function is pure: it borrows the input and neither allocates nor
/// mutates application state. It consumes exactly five header delimiters.
pub fn parse_packet<'a>(datagram: BoundedDatagram<'a>) -> (result: Result<Packet<'a>, ParseError>)
{
    let bytes = datagram.as_bytes();
    if bytes.len() > MAX_PACKET_SIZE {
        return Err(ParseError::PacketTooLarge);
    }

    let mut cursor: usize = 0;

    let version_end = match find_delimiter(bytes, cursor) {
        Some(index) => index,
        None => return Err(ParseError::MissingDelimiter),
    };
    let version_value = match parse_decimal(&bytes[cursor..version_end]) {
        Ok(value) => value,
        Err(error) => return Err(error),
    };
    if version_value != 1 {
        return Err(ParseError::UnsupportedVersion(version_value));
    }
    cursor = version_end + 1;

    let packet_number_end = match find_delimiter(bytes, cursor) {
        Some(index) => index,
        None => return Err(ParseError::MissingDelimiter),
    };
    let packet_number = match parse_decimal(&bytes[cursor..packet_number_end]) {
        Ok(value) => value,
        Err(error) => return Err(error),
    };
    cursor = packet_number_end + 1;

    let username_end = match find_delimiter(bytes, cursor) {
        Some(index) => index,
        None => return Err(ParseError::MissingDelimiter),
    };
    if username_end == cursor {
        return Err(ParseError::EmptyIdentityField);
    }
    let username = &bytes[cursor..username_end];
    cursor = username_end + 1;

    let hostname_end = match find_delimiter(bytes, cursor) {
        Some(index) => index,
        None => return Err(ParseError::MissingDelimiter),
    };
    if hostname_end == cursor {
        return Err(ParseError::EmptyIdentityField);
    }
    let hostname = &bytes[cursor..hostname_end];
    cursor = hostname_end + 1;

    let command_end = match find_delimiter(bytes, cursor) {
        Some(index) => index,
        None => return Err(ParseError::MissingDelimiter),
    };
    let command_value = match parse_decimal(&bytes[cursor..command_end]) {
        Ok(value) => value,
        Err(error) => return Err(error),
    };
    if command_value > u32::MAX as u64 {
        return Err(ParseError::NumericOverflow);
    }
    let command_word = command_value as u32;
    let command_id = (command_word & 0xff) as u8;
    let option_flags = command_word & !0xffu32;

    let command = match command_id {
        1 => CommandKind::BroadcastEntry,
        2 => CommandKind::BroadcastExit,
        3 => CommandKind::AnswerEntry,
        32 => CommandKind::SendMessage,
        other => CommandKind::Unsupported(other),
    };

    cursor = command_end + 1;
    let additional_data = &bytes[cursor..bytes.len()];

    Ok(Packet {
        version: 1,
        packet_number,
        username,
        hostname,
        command_word,
        command,
        option_flags,
        additional_data,
    })
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BoundedDatagram;

    fn parse(input: &[u8]) -> Result<Packet<'_>, ParseError> {
        let datagram = BoundedDatagram::new(input).expect("test input is bounded");
        parse_packet(datagram)
    }

    #[test]
    fn parses_send_message_and_preserves_colons_in_body() {
        let packet = parse(b"1:100:alice:workstation:32:hello:world").unwrap();
        assert_eq!(packet.version, 1);
        assert_eq!(packet.packet_number, 100);
        assert_eq!(packet.username, b"alice");
        assert_eq!(packet.hostname, b"workstation");
        assert_eq!(packet.command, CommandKind::SendMessage);
        assert_eq!(packet.command_word, 32);
        assert_eq!(packet.option_flags, 0);
        assert_eq!(packet.additional_data, b"hello:world");
    }

    #[test]
    fn separates_option_flags_from_command_id() {
        let packet = parse(b"1:7:alice:pc:16777248:hello").unwrap();
        assert_eq!(packet.command, CommandKind::SendMessage);
        assert_eq!(packet.option_flags, 0x0100_0000);
        assert_eq!(packet.additional_data, b"hello");
    }

    #[test]
    fn preserves_unknown_command_as_unsupported() {
        let packet = parse(b"1:7:alice:pc:999:body").unwrap();
        assert_eq!(packet.command, CommandKind::Unsupported(999u16 as u8));
        assert_eq!(packet.command_word, 999);
    }

    #[test]
    fn rejects_every_truncated_prefix_of_a_valid_packet() {
        let valid = b"1:100:alice:workstation:32:hello";
        for end in 0..valid.len() {
            assert!(parse(&valid[..end]).is_err(), "prefix length {end}");
        }
    }

    #[test]
    fn rejects_missing_delimiters() {
        assert_eq!(parse(b"1:2:alice:pc:32"), Err(ParseError::MissingDelimiter));
    }

    #[test]
    fn rejects_empty_identity_fields() {
        assert_eq!(
            parse(b"1:2::pc:32:body"),
            Err(ParseError::EmptyIdentityField)
        );
        assert_eq!(
            parse(b"1:2:alice::32:body"),
            Err(ParseError::EmptyIdentityField)
        );
    }

    #[test]
    fn rejects_non_decimal_and_negative_numeric_fields() {
        assert_eq!(parse(b"x:2:alice:pc:32:body"), Err(ParseError::InvalidDecimal));
        assert_eq!(parse(b"1:-2:alice:pc:32:body"), Err(ParseError::InvalidDecimal));
        assert_eq!(parse(b"1:2:alice:pc:+32:body"), Err(ParseError::InvalidDecimal));
    }

    #[test]
    fn rejects_numeric_overflow() {
        assert_eq!(
            parse(b"1:18446744073709551616:alice:pc:32:body"),
            Err(ParseError::NumericOverflow)
        );
        assert_eq!(
            parse(b"1:2:alice:pc:4294967296:body"),
            Err(ParseError::NumericOverflow)
        );
    }

    #[test]
    fn rejects_unsupported_protocol_version() {
        assert_eq!(
            parse(b"2:2:alice:pc:32:body"),
            Err(ParseError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn empty_additional_data_is_preserved() {
        let packet = parse(b"1:2:alice:pc:32:").unwrap();
        assert_eq!(packet.additional_data, b"");
    }
}
