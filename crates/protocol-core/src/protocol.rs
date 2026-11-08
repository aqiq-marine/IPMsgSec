//! Pure conversion from parsed wire packets into protocol-level events.
//!
//! This module deliberately keeps identity and message data borrowed. It does
//! not perform I/O, mutate peer state, decode text, or interpret command data
//! beyond the command identifier already classified by the parser.

use crate::discovery_metadata::{DiscoveryDataError, parse_discovery_data};
use crate::parser::{CommandKind, Packet};
use crate::state::MAX_IDENTITY_BYTES;
use vstd::prelude::*;

verus! {

/// Maximum plain-text body size for the initial message subset.
pub const MAX_MESSAGE_BODY_BYTES: usize = 4 * 1024;

/// IPMSG_CAPUTF8OPT: advertised in BR_ENTRY and ANSENTRY command words.
pub const IPMSG_CAPUTF8OPT: u32 = 0x0100_0000;

/// IPMSG_ABSENCEOPT: peer is currently in absence mode.
pub const IPMSG_ABSENCEOPT: u32 = 0x0000_0100;

/// IPMSG_UTF8OPT: marks a message body encoded as UTF-8.
pub const IPMSG_UTF8OPT: u32 = 0x0080_0000;

/// Text encoding selected by the wire-level UTF-8 option.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageEncoding {
    Cp932,
    Utf8,
}

/// Distinguishes an unsolicited discovery announcement from its reply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerJoinKind {
    BroadcastEntry,
    AnswerEntry,
    BroadcastAbsence,
}

/// A byte-exact peer identity borrowed from a parsed packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerIdentity<'a> {
    username: &'a [u8],
    hostname: &'a [u8],
}

impl<'a> PeerIdentity<'a> {
    /// Validate and construct a borrowed peer identity.
    pub fn new(
        username: &'a [u8],
        hostname: &'a [u8],
    ) -> (result: Result<Self, ProtocolError>)
        ensures
            match result {
                Ok(identity) => peer_identity_well_formed(&identity),
                Err(_) => true,
            }
    {
        make_identity(username, hostname)
    }

    pub fn username(&self) -> &'a [u8] {
        self.username
    }

    pub fn hostname(&self) -> &'a [u8] {
        self.hostname
    }
}

/// A semantic event represented by a supported packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolEvent<'a> {
    PeerJoined {
        identity: PeerIdentity<'a>,
        kind: PeerJoinKind,
        supports_utf8: bool,
        is_absent: bool,
        nickname: Option<&'a [u8]>,
        group: Option<&'a [u8]>,
    },
    PeerDeparted(PeerIdentity<'a>),
    MessageReceived {
        sender: PeerIdentity<'a>,
        body: &'a [u8],
        encoding: MessageEncoding,
    },
}

/// A parsed packet that cannot be represented by the current protocol model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidIdentity,
    UnsupportedCommand(u8),
    InconsistentPacket,
    /// A recognized command carries option bits whose semantics are not implemented.
    /// The raw flags remain available on the parsed Packet for diagnostics or future support.
    UnsupportedOptions { command_id: u8, flags: u32 },
    EmptyMessageBody,
    MessageBodyTooLong,
    UnsupportedMessageExtension,
    InvalidDiscoveryExtension,
}

pub closed spec fn peer_identity_well_formed(identity: &PeerIdentity) -> bool {
    identity.username@.len() > 0
        && identity.username@.len() <= MAX_IDENTITY_BYTES as int
        && identity.hostname@.len() > 0
        && identity.hostname@.len() <= MAX_IDENTITY_BYTES as int
}

pub closed spec fn protocol_event_well_formed(event: &ProtocolEvent) -> bool {
    match event {
        ProtocolEvent::PeerJoined { identity, .. } =>
            peer_identity_well_formed(identity),
        ProtocolEvent::PeerDeparted(identity) =>
            peer_identity_well_formed(identity),
        ProtocolEvent::MessageReceived { sender, .. } =>
            peer_identity_well_formed(sender),
    }
}

/// Construct an identity only when it satisfies the peer-table bounds.
fn make_identity<'a>(
    username: &'a [u8],
    hostname: &'a [u8],
) -> (result: Result<PeerIdentity<'a>, ProtocolError>)
    ensures
        match result {
            Ok(identity) => peer_identity_well_formed(&identity),
            Err(_) => true,
        }
{
    if username.is_empty()
        || hostname.is_empty()
        || username.len() > MAX_IDENTITY_BYTES
        || hostname.len() > MAX_IDENTITY_BYTES
    {
        return Err(ProtocolError::InvalidIdentity);
    }

    let identity = PeerIdentity { username, hostname };
    assert(peer_identity_well_formed(&identity));
    Ok(identity)
}

/// Convert a parsed packet into a protocol event.
///
/// This function is deterministic and allocation-free. Additional data for
/// discovery commands is intentionally left uninterpreted until its grammar
/// is specified. Message bodies remain opaque borrowed bytes.
pub fn classify_packet<'a>(
    packet: &Packet<'a>,
) -> (result: Result<ProtocolEvent<'a>, ProtocolError>)
    ensures
        match result {
            Ok(event) => protocol_event_well_formed(&event),
            Err(_) => true,
        }
{
    let command_id = (packet.command_word & 0xff) as u8;
    let command_matches = match packet.command {
        CommandKind::BroadcastEntry => command_id == 1,
        CommandKind::BroadcastExit => command_id == 2,
        CommandKind::AnswerEntry => command_id == 3,
        CommandKind::BroadcastAbsence => command_id == 4,
        CommandKind::SendMessage => command_id == 32,
        CommandKind::Unsupported(expected) => command_id == expected,
    };
    if !command_matches || packet.option_flags != (packet.command_word & !0xffu32) {
        return Err(ProtocolError::InconsistentPacket);
    }

    if let CommandKind::SendMessage = packet.command {
        if packet.option_flags & !IPMSG_UTF8OPT != 0 {
            return Err(ProtocolError::UnsupportedOptions {
                command_id,
                flags: packet.option_flags,
            });
        }
        if packet.additional_data.is_empty() {
            return Err(ProtocolError::EmptyMessageBody);
        }
        if packet.additional_data.len() > MAX_MESSAGE_BODY_BYTES {
            return Err(ProtocolError::MessageBodyTooLong);
        }
        if packet.additional_data.contains(&0) {
            return Err(ProtocolError::UnsupportedMessageExtension);
        }
    }

    let supports_utf8 = match packet.command {
        CommandKind::BroadcastEntry | CommandKind::AnswerEntry | CommandKind::BroadcastAbsence => {
            if packet.option_flags & !(IPMSG_CAPUTF8OPT | IPMSG_ABSENCEOPT) != 0 {
                return Err(ProtocolError::UnsupportedOptions {
                    command_id,
                    flags: packet.option_flags,
                });
            }
            packet.option_flags & IPMSG_CAPUTF8OPT != 0
        }
        CommandKind::BroadcastExit => {
            if packet.option_flags != 0 {
                return Err(ProtocolError::UnsupportedOptions {
                    command_id,
                    flags: packet.option_flags,
                });
            }
            false
        }
        _ => false,
    };

    let is_absent = packet.option_flags & IPMSG_ABSENCEOPT != 0;
    let discovery_data = match packet.command {
        CommandKind::BroadcastEntry | CommandKind::AnswerEntry | CommandKind::BroadcastAbsence =>
            Some(parse_discovery_data(packet.additional_data).map_err(|error| match error {
                DiscoveryDataError::MalformedExtension
                | DiscoveryDataError::InvalidUtf8
                | DiscoveryDataError::DuplicateField => ProtocolError::InvalidDiscoveryExtension,
            })?),
        _ => None,
    };
    let username = match discovery_data { Some(data) => data.username.unwrap_or(packet.username), None => packet.username };
    let hostname = match discovery_data { Some(data) => data.hostname.unwrap_or(packet.hostname), None => packet.hostname };
    let identity = make_identity(username, hostname)?;
    let nickname = match discovery_data { Some(data) => data.nickname.or(data.legacy_nickname), None => None };
    let group = match discovery_data { Some(data) => data.group.or(data.legacy_group), None => None };

    let event = match packet.command {
        CommandKind::BroadcastEntry =>
            ProtocolEvent::PeerJoined {
                identity,
                kind: PeerJoinKind::BroadcastEntry,
                supports_utf8,
                is_absent,
                nickname,
                group,
            },
        CommandKind::AnswerEntry =>
            ProtocolEvent::PeerJoined {
                identity,
                kind: PeerJoinKind::AnswerEntry,
                supports_utf8,
                is_absent,
                nickname,
                group,
            },
        CommandKind::BroadcastAbsence =>
            ProtocolEvent::PeerJoined {
                identity,
                kind: PeerJoinKind::BroadcastAbsence,
                supports_utf8,
                is_absent,
                nickname,
                group,
            },
        CommandKind::BroadcastExit =>
            ProtocolEvent::PeerDeparted(identity),
        CommandKind::SendMessage =>
            ProtocolEvent::MessageReceived {
                sender: identity,
                body: packet.additional_data,
                encoding: if packet.option_flags & IPMSG_UTF8OPT != 0 {
                    MessageEncoding::Utf8
                } else {
                    MessageEncoding::Cp932
                },
            },
        CommandKind::Unsupported(command_id) =>
            return Err(ProtocolError::UnsupportedCommand(command_id)),
    };

    assert(protocol_event_well_formed(&event));
    Ok(event)
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    fn packet<'a>(
        username: &'a [u8],
        hostname: &'a [u8],
        command: CommandKind,
        additional_data: &'a [u8],
    ) -> Packet<'a> {
        Packet {
            version: 1,
            packet_number: 7,
            username,
            hostname,
            command_word: match command {
                CommandKind::BroadcastEntry => 1,
                CommandKind::BroadcastExit => 2,
                CommandKind::AnswerEntry => 3,
                CommandKind::BroadcastAbsence => 4,
                CommandKind::SendMessage => 32,
                CommandKind::Unsupported(id) => id as u32,
            },
            command,
            option_flags: 0,
            additional_data,
        }
    }

    #[test]
    fn classifies_broadcast_entry_with_its_join_kind() {
        let parsed = packet(b"alice", b"workstation", CommandKind::BroadcastEntry, b"");
        assert_eq!(
            classify_packet(&parsed),
            Ok(ProtocolEvent::PeerJoined {
                identity: PeerIdentity {
                    username: b"alice",
                    hostname: b"workstation",
                },
                kind: PeerJoinKind::BroadcastEntry,
                supports_utf8: false,
                is_absent: false,
                nickname: None,
                group: None,
            })
        );
    }

    #[test]
    fn classifies_answer_entry_with_its_join_kind() {
        let parsed = packet(b"alice", b"workstation", CommandKind::AnswerEntry, b"");
        assert_eq!(
            classify_packet(&parsed),
            Ok(ProtocolEvent::PeerJoined {
                identity: PeerIdentity {
                    username: b"alice",
                    hostname: b"workstation",
                },
                kind: PeerJoinKind::AnswerEntry,
                supports_utf8: false,
                is_absent: false,
                nickname: None,
                group: None,
            })
        );
    }

    #[test]
    fn classifies_broadcast_exit_as_departure() {
        let parsed = packet(b"alice", b"workstation", CommandKind::BroadcastExit, b"");
        assert_eq!(
            classify_packet(&parsed),
            Ok(ProtocolEvent::PeerDeparted(PeerIdentity {
                username: b"alice",
                hostname: b"workstation",
            }))
        );
    }

    #[test]
    fn preserves_message_body_as_borrowed_opaque_bytes() {
        let body = b"hello:\xff:world";
        let parsed = packet(b"alice", b"workstation", CommandKind::SendMessage, body);
        let event = classify_packet(&parsed).unwrap();
        match event {
            ProtocolEvent::MessageReceived {
                sender,
                body: actual,
                encoding,
            } => {
                assert_eq!(encoding, MessageEncoding::Cp932);
                assert_eq!(sender.username(), b"alice");
                assert_eq!(sender.hostname(), b"workstation");
                assert_eq!(actual, body);
                assert_eq!(actual.as_ptr(), body.as_ptr());
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn rejects_send_message_with_unimplemented_option_flags() {
        let mut parsed = packet(b"alice", b"workstation", CommandKind::SendMessage, b"hello");
        parsed.command_word = 0x0100_0020;
        parsed.option_flags = 0x0100_0000;
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::UnsupportedOptions {
                command_id: 32,
                flags: 0x0100_0000,
            })
        );
    }

    #[test]
    fn rejects_empty_oversized_and_extension_message_bodies() {
        let empty = packet(b"alice", b"workstation", CommandKind::SendMessage, b"");
        assert_eq!(
            classify_packet(&empty),
            Err(ProtocolError::EmptyMessageBody)
        );

        let oversized = [b'a'; MAX_MESSAGE_BODY_BYTES + 1];
        let too_long = packet(
            b"alice",
            b"workstation",
            CommandKind::SendMessage,
            &oversized,
        );
        assert_eq!(
            classify_packet(&too_long),
            Err(ProtocolError::MessageBodyTooLong)
        );

        let extension = packet(
            b"alice",
            b"workstation",
            CommandKind::SendMessage,
            b"body\0attachment",
        );
        assert_eq!(
            classify_packet(&extension),
            Err(ProtocolError::UnsupportedMessageExtension)
        );
    }

    #[test]
    fn rejects_inconsistent_command_word_and_classification() {
        let mut parsed = packet(b"alice", b"workstation", CommandKind::BroadcastEntry, b"");
        parsed.command_word = 32;
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::InconsistentPacket)
        );

        let mut parsed = packet(b"alice", b"workstation", CommandKind::BroadcastEntry, b"");
        parsed.option_flags = 0x0100_0000;
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::InconsistentPacket)
        );
    }

    #[test]
    fn rejects_unsupported_command_without_reinterpreting_it() {
        let parsed = packet(
            b"alice",
            b"workstation",
            CommandKind::Unsupported(231),
            b"ignored",
        );
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::UnsupportedCommand(231))
        );
    }

    #[test]
    fn rejects_empty_identity_components() {
        for (username, hostname) in [(b"".as_slice(), b"pc".as_slice()), (b"alice", b"")] {
            let parsed = packet(username, hostname, CommandKind::BroadcastEntry, b"");
            assert_eq!(
                classify_packet(&parsed),
                Err(ProtocolError::InvalidIdentity)
            );
        }
    }

    #[test]
    fn rejects_identity_components_over_peer_table_limit() {
        let oversized = [b'x'; MAX_IDENTITY_BYTES + 1];
        let parsed = packet(&oversized, b"pc", CommandKind::BroadcastEntry, b"");
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::InvalidIdentity)
        );

        let parsed = packet(b"alice", &oversized, CommandKind::BroadcastEntry, b"");
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::InvalidIdentity)
        );
    }

    #[test]
    fn accepts_identity_components_at_exact_limit() {
        let exact = [b'x'; MAX_IDENTITY_BYTES];
        let parsed = packet(&exact, &exact, CommandKind::BroadcastEntry, b"");
        let event = classify_packet(&parsed).unwrap();
        match event {
            ProtocolEvent::PeerJoined { identity, .. } => {
                assert_eq!(identity.username().len(), MAX_IDENTITY_BYTES);
                assert_eq!(identity.hostname().len(), MAX_IDENTITY_BYTES);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn accepts_utf8_capability_on_entry_and_answer_entry_only() {
        let mut entry = packet(b"alice", b"workstation", CommandKind::BroadcastEntry, b"");
        entry.command_word = 0x0100_0001;
        entry.option_flags = IPMSG_CAPUTF8OPT;
        assert!(matches!(
            classify_packet(&entry),
            Ok(ProtocolEvent::PeerJoined {
                supports_utf8: true,
                ..
            })
        ));

        let mut answer = packet(b"alice", b"workstation", CommandKind::AnswerEntry, b"");
        answer.command_word = 0x0100_0003;
        answer.option_flags = IPMSG_CAPUTF8OPT;
        assert!(matches!(
            classify_packet(&answer),
            Ok(ProtocolEvent::PeerJoined {
                supports_utf8: true,
                ..
            })
        ));

        let mut exit = packet(b"alice", b"workstation", CommandKind::BroadcastExit, b"");
        exit.command_word = 0x0100_0002;
        exit.option_flags = IPMSG_CAPUTF8OPT;
        assert_eq!(
            classify_packet(&exit),
            Err(ProtocolError::UnsupportedOptions {
                command_id: 2,
                flags: IPMSG_CAPUTF8OPT,
            })
        );
    }

    #[test]
    fn recognizes_utf8_and_legacy_message_encodings() {
        let body = "こんにちは".as_bytes();
        let mut utf8 = packet(b"alice", b"pc", CommandKind::SendMessage, body);
        utf8.command_word = 0x0080_0020;
        utf8.option_flags = IPMSG_UTF8OPT;
        assert!(matches!(
            classify_packet(&utf8),
            Ok(ProtocolEvent::MessageReceived { encoding: MessageEncoding::Utf8, body: actual, .. })
                if actual == body
        ));

        let legacy = packet(b"alice", b"pc", CommandKind::SendMessage, b"hello");
        assert!(matches!(
            classify_packet(&legacy),
            Ok(ProtocolEvent::MessageReceived {
                encoding: MessageEncoding::Cp932,
                ..
            })
        ));
    }

    #[test]
    fn preserves_unknown_discovery_flags_at_parse_layer_but_does_not_apply_them() {
        let mut parsed = packet(
            b"alice",
            b"workstation",
            CommandKind::BroadcastEntry,
            b"opaque-extension",
        );
        let unknown_and_unimplemented = 0x0200_0000;
        parsed.command_word = 0x0200_0001;
        parsed.option_flags = unknown_and_unimplemented;

        assert_eq!(parsed.command_word, 0x0200_0001);
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::UnsupportedOptions {
                command_id: 1,
                flags: unknown_and_unimplemented,
            })
        );
    }

    #[test]
    fn discovery_additional_data_is_not_interpreted_by_classifier() {
        let parsed = packet(
            b"alice",
            b"workstation",
            CommandKind::BroadcastEntry,
            b"unresolved:extension:data",
        );
        assert!(matches!(
            classify_packet(&parsed),
            Ok(ProtocolEvent::PeerJoined { .. })
        ));
    }

    #[test]
    fn classifies_broadcast_absence_and_utf8_metadata_overrides_legacy_fields() {
        let data = b"legacy-nick\\0legacy-group\\0\\nUN:alice-utf8\\nHN:host-utf8\\nNN:display-name\\nGN:chemistry\\n";
        let mut parsed = packet(
            b"legacy-user",
            b"legacy-host",
            CommandKind::BroadcastAbsence,
            data,
        );
        parsed.command_word = 0x0100_0104;
        parsed.option_flags = IPMSG_CAPUTF8OPT | IPMSG_ABSENCEOPT;

        let event = classify_packet(&parsed).unwrap();
        match event {
            ProtocolEvent::PeerJoined {
                identity,
                kind,
                supports_utf8,
                is_absent,
                nickname,
                group,
            } => {
                assert_eq!(kind, PeerJoinKind::BroadcastAbsence);
                assert!(supports_utf8);
                assert!(is_absent);
                assert_eq!(identity.username(), b"alice-utf8");
                assert_eq!(identity.hostname(), b"host-utf8");
                assert_eq!(nickname, Some(b"display-name".as_slice()));
                assert_eq!(group, Some(b"chemistry".as_slice()));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn classifies_broadcast_absence_without_absence_flag_as_return_to_presence() {
        let mut parsed = packet(
            b"alice",
            b"pc",
            CommandKind::BroadcastAbsence,
            b"nickname",
        );
        parsed.command_word = 0x0100_0004;
        parsed.option_flags = IPMSG_CAPUTF8OPT;
        assert!(matches!(
            classify_packet(&parsed),
            Ok(ProtocolEvent::PeerJoined {
                kind: PeerJoinKind::BroadcastAbsence,
                is_absent: false,
                supports_utf8: true,
                ..
            })
        ));
    }

    #[test]
    fn rejects_malformed_utf8_discovery_metadata_without_emitting_event() {
        let mut parsed = packet(
            b"alice",
            b"pc",
            CommandKind::BroadcastAbsence,
            b"nick\\0group\\0\\nUN:\xff\\n",
        );
        parsed.command_word = 0x0000_0104;
        parsed.option_flags = IPMSG_ABSENCEOPT;
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::InvalidDiscoveryExtension)
        );
    }
}
