//! Pure conversion from parsed wire packets into protocol-level events.
//!
//! This module deliberately keeps identity and message data borrowed. It does
//! not perform I/O, mutate peer state, decode text, or interpret command data
//! beyond the command identifier already classified by the parser.

use crate::parser::{CommandKind, Packet};
use crate::state::MAX_IDENTITY_BYTES;
use vstd::prelude::*;

verus! {

/// A byte-exact peer identity borrowed from a parsed packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerIdentity<'a> {
    username: &'a [u8],
    hostname: &'a [u8],
}

impl<'a> PeerIdentity<'a> {
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
    PeerJoined(PeerIdentity<'a>),
    PeerDeparted(PeerIdentity<'a>),
    MessageReceived {
        sender: PeerIdentity<'a>,
        body: &'a [u8],
    },
}

/// A parsed packet that cannot be represented by the current protocol model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidIdentity,
    UnsupportedCommand(u8),
    InconsistentPacket,
}

pub open spec fn peer_identity_well_formed(identity: &PeerIdentity) -> bool {
    identity.username@.len() > 0
        && identity.username@.len() <= MAX_IDENTITY_BYTES as int
        && identity.hostname@.len() > 0
        && identity.hostname@.len() <= MAX_IDENTITY_BYTES as int
}

pub open spec fn protocol_event_well_formed(event: &ProtocolEvent) -> bool {
    match event {
        ProtocolEvent::PeerJoined(identity) =>
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
        CommandKind::SendMessage => command_id == 32,
        CommandKind::Unsupported(expected) => command_id == expected,
    };
    if !command_matches || packet.option_flags != (packet.command_word & !0xffu32) {
        return Err(ProtocolError::InconsistentPacket);
    }

    let identity = match make_identity(packet.username, packet.hostname) {
        Ok(identity) => identity,
        Err(error) => return Err(error),
    };

    let event = match packet.command {
        CommandKind::BroadcastEntry | CommandKind::AnswerEntry =>
            ProtocolEvent::PeerJoined(identity),
        CommandKind::BroadcastExit =>
            ProtocolEvent::PeerDeparted(identity),
        CommandKind::SendMessage =>
            ProtocolEvent::MessageReceived {
                sender: identity,
                body: packet.additional_data,
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
                CommandKind::SendMessage => 32,
                CommandKind::Unsupported(id) => id as u32,
            },
            command,
            option_flags: 0,
            additional_data,
        }
    }

    #[test]
    fn classifies_broadcast_entry_and_answer_entry_as_join() {
        for command in [CommandKind::BroadcastEntry, CommandKind::AnswerEntry] {
            let parsed = packet(b"alice", b"workstation", command, b"");
            let event = classify_packet(&parsed).unwrap();
            assert_eq!(
                event,
                ProtocolEvent::PeerJoined(PeerIdentity {
                    username: b"alice",
                    hostname: b"workstation",
                })
            );
        }
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
            ProtocolEvent::MessageReceived { sender, body: actual } => {
                assert_eq!(sender.username(), b"alice");
                assert_eq!(sender.hostname(), b"workstation");
                assert_eq!(actual, body);
                assert_eq!(actual.as_ptr(), body.as_ptr());
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn rejects_inconsistent_command_word_and_classification() {
        let mut parsed = packet(
            b"alice",
            b"workstation",
            CommandKind::BroadcastEntry,
            b"",
        );
        parsed.command_word = 32;
        assert_eq!(
            classify_packet(&parsed),
            Err(ProtocolError::InconsistentPacket)
        );

        let mut parsed = packet(
            b"alice",
            b"workstation",
            CommandKind::BroadcastEntry,
            b"",
        );
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
            assert_eq!(classify_packet(&parsed), Err(ProtocolError::InvalidIdentity));
        }
    }

    #[test]
    fn rejects_identity_components_over_peer_table_limit() {
        let oversized = [b'x'; MAX_IDENTITY_BYTES + 1];
        let parsed = packet(&oversized, b"pc", CommandKind::BroadcastEntry, b"");
        assert_eq!(classify_packet(&parsed), Err(ProtocolError::InvalidIdentity));

        let parsed = packet(b"alice", &oversized, CommandKind::BroadcastEntry, b"");
        assert_eq!(classify_packet(&parsed), Err(ProtocolError::InvalidIdentity));
    }

    #[test]
    fn accepts_identity_components_at_exact_limit() {
        let exact = [b'x'; MAX_IDENTITY_BYTES];
        let parsed = packet(&exact, &exact, CommandKind::BroadcastEntry, b"");
        let event = classify_packet(&parsed).unwrap();
        match event {
            ProtocolEvent::PeerJoined(identity) => {
                assert_eq!(identity.username().len(), MAX_IDENTITY_BYTES);
                assert_eq!(identity.hostname().len(), MAX_IDENTITY_BYTES);
            }
            other => panic!("unexpected event: {other:?}"),
        }
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
            Ok(ProtocolEvent::PeerJoined(_))
        ));
    }
}
