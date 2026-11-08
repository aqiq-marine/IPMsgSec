//! Pure, bounded parser for the IP Messenger text header.
//!
//! The first five ':' delimiters separate the fixed header fields. Everything
//! after the fifth delimiter is opaque command-specific data; in particular,
//! colons in a message body are preserved.

use vstd::prelude::*;
use crate::MAX_PACKET_SIZE;

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
            Some(index) =>
                start <= index
                && index < bytes.len()
                && bytes[index] == b':'
                && forall|i: int| start as int <= i < index as int ==> bytes@[i] != b':',
            None =>
                forall|i: int| start as int <= i < bytes.len() as int ==> bytes@[i] != b':',
        }
{
    let mut index = start;
    while index < bytes.len()
        invariant
            start <= index,
            index <= bytes.len(),
            forall|i: int| start as int <= i < index as int ==> bytes@[i] != b':',
        decreases
            bytes.len() - index,
    {
        if bytes[index] == b':' {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// Mathematical value of a sequence of ASCII decimal digits.
pub open spec fn decimal_value(bytes: Seq<u8>) -> int
    decreases bytes.len()
{
    if bytes.len() == 0 {
        0
    } else {
        decimal_value(bytes.take((bytes.len() - 1) as int)) * 10
            + bytes[(bytes.len() - 1) as int] as int - b'0' as int
    }
}

/// Appending one digit extends the decimal value by one base-10 place.
proof fn decimal_value_push(bytes: Seq<u8>, digit: u8)
    requires
        b'0' <= digit <= b'9',
    ensures
        decimal_value(bytes.push(digit))
            == decimal_value(bytes) * 10 + digit as int - b'0' as int,
{
    reveal(decimal_value);
}

/// Parse a non-empty sequence of ASCII decimal digits as u64.
/// Overflow is detected before multiplication/addition.
#[allow(clippy::manual_range_contains)]
fn parse_decimal(digits: &[u8]) -> (result: Result<u64, ParseError>)
    ensures
        match result {
            Ok(value) =>
                value as int == decimal_value(digits@)
                && forall|i: int| 0 <= i < digits.len() as int
                    ==> digits@[i] >= b'0' && digits@[i] <= b'9',
            Err(_) => true,
        }
{
    if digits.is_empty() {
        return Err(ParseError::InvalidDecimal);
    }

    let mut index: usize = 0;
    let mut value: u64 = 0;
    assert(decimal_value(digits@.take(0)) == 0) by {
        reveal(decimal_value);
    };
    while index < digits.len()
        invariant
            index <= digits.len(),
            value as int == decimal_value(digits@.take(index as int)),
            forall|i: int| 0 <= i < index as int
                ==> digits@[i] >= b'0' && digits@[i] <= b'9',
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

        let prefix = digits@.take(index as int);
        decimal_value_push(prefix, byte);
        assert(digits@.take(index as int + 1) == prefix.push(byte));
        assert(
            decimal_value(digits@.take(index as int + 1))
                == decimal_value(prefix) * 10 + digit as int
        );
        value = value * 10 + digit;
        index = index + 1;
    }

    Ok(value)
}

/// Logical relation between a command classification and its wire ID.
pub open spec fn command_matches(command: CommandKind, command_id: u8) -> bool
{
    match command {
        CommandKind::BroadcastEntry => command_id == 1,
        CommandKind::BroadcastExit => command_id == 2,
        CommandKind::AnswerEntry => command_id == 3,
        CommandKind::SendMessage => command_id == 32,
        CommandKind::Unsupported(other) =>
            other == command_id
            && command_id != 1
            && command_id != 2
            && command_id != 3
            && command_id != 32,
    }
}

/// Convert a command's low byte to the supported command classification.
fn classify_command(command_id: u8) -> (result: CommandKind)
    ensures
        command_matches(result, command_id),
{
    match command_id {
        1 => CommandKind::BroadcastEntry,
        2 => CommandKind::BroadcastExit,
        3 => CommandKind::AnswerEntry,
        32 => CommandKind::SendMessage,
        other => CommandKind::Unsupported(other),
    }
}

/// Parse a bounded IP Messenger datagram.
///
/// This function is pure: it borrows the input and neither allocates nor
/// mutates application state. It consumes exactly five header delimiters.
pub fn parse_packet<'a>(bytes: &'a [u8]) -> (result: Result<Packet<'a>, ParseError>)
    ensures
        match result {
            Ok(packet) =>
                bytes.len() <= MAX_PACKET_SIZE
                && packet.version == 1
                && packet.username.len() > 0
                && packet.hostname.len() > 0
                && packet.additional_data.len() <= MAX_PACKET_SIZE
                && packet.option_flags == (packet.command_word & !0xffu32)
                && command_matches(packet.command, (packet.command_word & 0xff) as u8),
            Err(_) => true,
        }
{
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

    let command = classify_command(command_id);

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
    fn parse(input: &[u8]) -> Result<Packet<'_>, ParseError> {
        parse_packet(input)
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
        assert_eq!(packet.command, CommandKind::Unsupported(231));
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

    #[test]
    fn accepts_maximum_u32_command_word() {
        let packet = parse(b"1:2:alice:pc:4294967295:body").unwrap();
        assert_eq!(packet.command_word, u32::MAX);
        assert_eq!(packet.command, CommandKind::Unsupported(255));
        assert_eq!(packet.option_flags, 0xffff_ff00);
    }

    #[test]
    fn rejects_oversized_datagram() {
        let input = vec![b'x'; MAX_PACKET_SIZE + 1];
        assert_eq!(parse(&input), Err(ParseError::PacketTooLarge));
    }

    #[test]
    fn accepts_command_flag_bits_without_changing_command_id() {
        let packet = parse(b"1:2:alice:pc:16777217:body").unwrap();
        assert_eq!(packet.command, CommandKind::BroadcastEntry);
        assert_eq!(packet.option_flags, 0x0100_0000);
    }

    #[test]
    fn accepts_all_short_byte_strings_without_panicking() {
        let _ = parse(&[]);
        for first in 0_u8..=u8::MAX {
            let one = [first];
            let _ = parse(&one);
            for second in 0_u8..=u8::MAX {
                let two = [first, second];
                let _ = parse(&two);
            }
        }
    }
}
