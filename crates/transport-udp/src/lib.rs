//! Unverified UDP boundary for the IPMsgSec protocol core.
//!
//! The adapter owns socket I/O and source-address handling. It receives into
//! MAX_PACKET_SIZE + 1 bytes so oversized UDP datagrams are detectable even
//! though std::net::UdpSocket::recv_from does not report a truncation flag.
//! Parsing and event classification are delegated to protocol-core.

use std::io;
use std::net::{SocketAddr, UdpSocket};

use ipmsgsec_protocol_core::{
    BoundedDatagram, MAX_IDENTITY_BYTES, MAX_PACKET_SIZE, ParseError, ProtocolAction,
    ProtocolError, ProtocolEvent, ReplyTarget, classify_packet, parse_packet,
};

/// Conventional IP Messenger UDP port. Binding remains an explicit caller choice.
pub const IPMSG_UDP_PORT: u16 = 2425;

/// One extra byte makes truncation beyond the application limit observable.
pub const RECEIVE_BUFFER_SIZE: usize = MAX_PACKET_SIZE + 1;

/// Initial plain-text message body limit; deliberately below the datagram cap.
pub const MAX_MESSAGE_BODY_BYTES: usize = 4 * 1024;

/// A received, size-checked datagram and the source-address association for it.
#[derive(Debug)]
pub struct ReceivedDatagram<'a> {
    datagram: BoundedDatagram<'a>,
    source: SocketAddr,
    reply_target: ReplyTarget,
}

impl<'a> ReceivedDatagram<'a> {
    pub fn datagram(&self) -> BoundedDatagram<'a> {
        self.datagram
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.datagram.as_bytes()
    }

    pub fn source(&self) -> SocketAddr {
        self.source
    }

    pub fn reply_target(&self) -> ReplyTarget {
        self.reply_target
    }

    /// Parse and classify using the protocol-core implementation.
    pub fn classify(&self) -> Result<ProtocolEvent<'a>, InboundError> {
        let packet = parse_packet(self.datagram.as_bytes()).map_err(InboundError::Parse)?;
        classify_packet(&packet).map_err(InboundError::Protocol)
    }
}

/// Receive-side failures, kept distinct from parser/protocol failures.
#[derive(Debug)]
pub enum ReceiveError {
    Io(io::Error),
    PacketTooLarge {
        source: SocketAddr,
        observed_at_least: usize,
        maximum: usize,
    },
    ReplyTokenExhausted,
}

impl std::fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "UDP receive failed: {error}"),
            Self::PacketTooLarge {
                observed_at_least,
                maximum,
                ..
            } => write!(
                f,
                "UDP datagram is at least {observed_at_least} bytes (maximum {maximum})"
            ),
            Self::ReplyTokenExhausted => write!(f, "reply-target token space exhausted"),
        }
    }
}

impl std::error::Error for ReceiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// Parse/classification failures are not socket failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboundError {
    Parse(ParseError),
    Protocol(ProtocolError),
}

/// Errors while constructing or sending an abstract response action.
#[derive(Debug)]
pub enum SendError {
    TargetMismatch,
    Encode(EncodeError),
    Io(io::Error),
    ShortDatagramWrite { expected: usize, actual: usize },
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetMismatch => write!(f, "action target does not belong to this datagram"),
            Self::Encode(error) => write!(f, "cannot encode answer-entry packet: {error}"),
            Self::Io(error) => write!(f, "UDP send failed: {error}"),
            Self::ShortDatagramWrite { expected, actual } => {
                write!(
                    f,
                    "short UDP send: expected {expected} bytes, sent {actual}"
                )
            }
        }
    }
}

impl std::error::Error for SendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Encode(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// Local identity cannot be represented safely in the textual header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    EmptyIdentity,
    IdentityTooLong,
    ContainsDelimiter,
    EmptyMessage,
    MessageTooLong,
    NonAsciiMessage,
    ContainsNul,
    UnsupportedControl,
    PacketTooLarge,
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyIdentity => write!(f, "username and hostname must be non-empty"),
            Self::IdentityTooLong => write!(
                f,
                "username and hostname must each be at most {MAX_IDENTITY_BYTES} bytes"
            ),
            Self::ContainsDelimiter => write!(f, "identity fields cannot contain ':'"),
            Self::EmptyMessage => write!(f, "message body must not be empty"),
            Self::MessageTooLong => write!(
                f,
                "message body must be at most {MAX_MESSAGE_BODY_BYTES} bytes"
            ),
            Self::NonAsciiMessage => write!(f, "message body must be ASCII until UTF-8 negotiation is implemented"),
            Self::ContainsNul => write!(f, "message body cannot contain NUL in the plain-text MVP"),
            Self::UnsupportedControl => write!(
                f,
                "message body may contain only printable ASCII, TAB, and LF"
            ),
            Self::PacketTooLarge => write!(f, "encoded packet exceeds {MAX_PACKET_SIZE} bytes"),
        }
    }
}

impl std::error::Error for EncodeError {}

/// Construct an IPMSG_ANSENTRY packet using the caller-supplied packet number.
///
/// Identity bytes are copied verbatim; this function does not assume UTF-8 or
/// choose a legacy encoding. The ':' delimiter is rejected because it would
/// change the parser's five-field header structure. Packet-number allocation
/// and collision policy remain the caller's responsibility.
fn encode_identity_command(
    packet_number: u64,
    username: &[u8],
    hostname: &[u8],
    command: u8,
) -> Result<Vec<u8>, EncodeError> {
    if username.is_empty() || hostname.is_empty() {
        return Err(EncodeError::EmptyIdentity);
    }
    if username.len() > MAX_IDENTITY_BYTES || hostname.len() > MAX_IDENTITY_BYTES {
        return Err(EncodeError::IdentityTooLong);
    }
    if username.contains(&b':') || hostname.contains(&b':') {
        return Err(EncodeError::ContainsDelimiter);
    }

    let mut packet = Vec::with_capacity(64 + username.len() + hostname.len());
    packet.extend_from_slice(b"1:");
    packet.extend_from_slice(packet_number.to_string().as_bytes());
    packet.push(b':');
    packet.extend_from_slice(username);
    packet.push(b':');
    packet.extend_from_slice(hostname);
    packet.push(b':');
    packet.extend_from_slice(command.to_string().as_bytes());
    packet.push(b':');

    if packet.len() > MAX_PACKET_SIZE {
        return Err(EncodeError::PacketTooLarge);
    }
    Ok(packet)
}

/// Encode an IPMSG_ANSENTRY response (command 3).
pub fn encode_answer_entry(
    packet_number: u64,
    username: &[u8],
    hostname: &[u8],
) -> Result<Vec<u8>, EncodeError> {
    encode_identity_command(packet_number, username, hostname, 3)
}

/// Encode an IPMSG_BR_ENTRY announcement (command 1).
pub fn encode_broadcast_entry(
    packet_number: u64,
    username: &[u8],
    hostname: &[u8],
) -> Result<Vec<u8>, EncodeError> {
    encode_identity_command(packet_number, username, hostname, 1)
}

/// Encode an IPMSG_BR_EXIT announcement (command 2).
pub fn encode_broadcast_exit(
    packet_number: u64,
    username: &[u8],
    hostname: &[u8],
) -> Result<Vec<u8>, EncodeError> {
    encode_identity_command(packet_number, username, hostname, 2)
}

/// Encode an unflagged IPMSG_SENDMSG packet (command 32).
///
/// The initial interoperable subset is ASCII-only. ASCII is represented
/// identically by CP932 and UTF-8, so this does not require advertising or
/// assuming IPMSG_CAPUTF8OPT. NUL-separated attachment extensions and
/// SENDCHECK acknowledgements are deliberately not implemented.
pub fn encode_send_message(
    packet_number: u64,
    username: &[u8],
    hostname: &[u8],
    body: &[u8],
) -> Result<Vec<u8>, EncodeError> {
    if body.is_empty() {
        return Err(EncodeError::EmptyMessage);
    }
    if body.len() > MAX_MESSAGE_BODY_BYTES {
        return Err(EncodeError::MessageTooLong);
    }
    if !body.is_ascii() {
        return Err(EncodeError::NonAsciiMessage);
    }
    for byte in body {
        if *byte == 0 {
            return Err(EncodeError::ContainsNul);
        }
        if !(*byte == b'\n' || *byte == b'\\t' || (0x20..=0x7e).contains(byte)) {
            return Err(EncodeError::UnsupportedControl);
        }
    }

    let mut packet = encode_identity_command(packet_number, username, hostname, 32)?;
    if packet.len().saturating_add(body.len()) > MAX_PACKET_SIZE {
        return Err(EncodeError::PacketTooLarge);
    }
    packet.extend_from_slice(body);
    Ok(packet)
}

/// A deliberately small UDP socket adapter.
///
/// Binding address, interface selection, broadcast destination, and socket
/// lifetime are explicit caller responsibilities. No global/default interface
/// is selected implicitly.
#[derive(Debug)]
pub struct UdpTransport {
    socket: UdpSocket,
    next_reply_token: u64,
}

impl UdpTransport {
    pub fn bind(address: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(address)?;
        Ok(Self {
            socket,
            next_reply_token: 1,
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn set_broadcast(&self, enabled: bool) -> io::Result<()> {
        self.socket.set_broadcast(enabled)
    }

    pub fn set_read_timeout(&self, timeout: Option<std::time::Duration>) -> io::Result<()> {
        self.socket.set_read_timeout(timeout)
    }

    /// Receive one datagram, preserving its source address.
    ///
    /// The buffer must have exactly RECEIVE_BUFFER_SIZE bytes. If a datagram
    /// fills the extra sentinel byte, it is rejected as oversized; this works
    /// even on APIs that silently truncate a datagram to the supplied buffer.
    pub fn recv_from<'a>(
        &mut self,
        buffer: &'a mut [u8; RECEIVE_BUFFER_SIZE],
    ) -> Result<ReceivedDatagram<'a>, ReceiveError> {
        let (length, source) = self.socket.recv_from(buffer).map_err(ReceiveError::Io)?;
        if length > MAX_PACKET_SIZE {
            return Err(ReceiveError::PacketTooLarge {
                source,
                observed_at_least: length,
                maximum: MAX_PACKET_SIZE,
            });
        }

        let reply_target = ReplyTarget::new(self.next_reply_token);
        self.next_reply_token = self
            .next_reply_token
            .checked_add(1)
            .ok_or(ReceiveError::ReplyTokenExhausted)?;

        let datagram = BoundedDatagram::new(&buffer[..length])
            .expect("receive length was checked against MAX_PACKET_SIZE");

        Ok(ReceivedDatagram {
            datagram,
            source,
            reply_target,
        })
    }

    /// Translate the verified SendAnswerEntry action into a directed UDP send.
    ///
    /// The action target must match the token carried by received; the
    /// corresponding SocketAddr is retained by that same receive result.
    pub fn send_action(
        &self,
        received: &ReceivedDatagram<'_>,
        action: ProtocolAction,
        packet_number: u64,
        local_username: &[u8],
        local_hostname: &[u8],
    ) -> Result<usize, SendError> {
        match action {
            ProtocolAction::SendAnswerEntry { target } => {
                if target != received.reply_target {
                    return Err(SendError::TargetMismatch);
                }
                let payload = encode_answer_entry(packet_number, local_username, local_hostname)
                    .map_err(SendError::Encode)?;
                let sent = self
                    .socket
                    .send_to(&payload, received.source)
                    .map_err(SendError::Io)?;
                if sent != payload.len() {
                    return Err(SendError::ShortDatagramWrite {
                        expected: payload.len(),
                        actual: sent,
                    });
                }
                Ok(sent)
            }
        }
    }

    /// Send an explicitly addressed datagram (for a later broadcast policy).
    pub fn send_to(&self, target: SocketAddr, bytes: &[u8]) -> io::Result<usize> {
        self.socket.send_to(bytes, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipmsgsec_protocol_core::{CommandKind, PeerJoinKind};

    #[test]
    fn encodes_answer_entry_and_round_trips_through_core_parser() {
        let bytes = encode_answer_entry(123, b"alice", b"workstation").unwrap();
        let packet = parse_packet(&bytes).unwrap();
        assert_eq!(packet.version, 1);
        assert_eq!(packet.packet_number, 123);
        assert_eq!(packet.username, b"alice");
        assert_eq!(packet.hostname, b"workstation");
        assert_eq!(packet.command_word, 3);
        assert_eq!(packet.command, CommandKind::AnswerEntry);
        assert!(packet.additional_data.is_empty());
        let event = classify_packet(&packet).unwrap();
        assert!(matches!(
            event,
            ProtocolEvent::PeerJoined {
                kind: PeerJoinKind::AnswerEntry,
                ..
            }
        ));
    }

    #[test]
    fn encodes_entry_and_exit_commands_with_empty_extension() {
        let entry = encode_broadcast_entry(10, b"alice", b"host").unwrap();
        let exit = encode_broadcast_exit(11, b"alice", b"host").unwrap();
        let parsed_entry = parse_packet(&entry).unwrap();
        let parsed_exit = parse_packet(&exit).unwrap();
        assert_eq!(parsed_entry.command, CommandKind::BroadcastEntry);
        assert_eq!(parsed_exit.command, CommandKind::BroadcastExit);
        assert!(parsed_entry.additional_data.is_empty());
        assert!(parsed_exit.additional_data.is_empty());
        assert!(matches!(
            classify_packet(&parsed_entry),
            Ok(ProtocolEvent::PeerJoined {
                kind: PeerJoinKind::BroadcastEntry,
                ..
            })
        ));
        assert!(matches!(
            classify_packet(&parsed_exit),
            Ok(ProtocolEvent::PeerDeparted(_))
        ));
    }

    #[test]
    fn encodes_ascii_send_message_and_round_trips_through_core() {
        let bytes = encode_send_message(123, b"alice", b"workstation", b"hello: world\nnext").unwrap();
        let packet = parse_packet(&bytes).unwrap();
        assert_eq!(packet.packet_number, 123);
        assert_eq!(packet.username, b"alice");
        assert_eq!(packet.hostname, b"workstation");
        assert_eq!(packet.command, CommandKind::SendMessage);
        assert_eq!(packet.command_word, 32);
        assert_eq!(packet.option_flags, 0);
        assert_eq!(packet.additional_data, b"hello: world\nnext");
        assert!(matches!(
            classify_packet(&packet),
            Ok(ProtocolEvent::MessageReceived { body, .. })
                if body == b"hello: world\nnext"
        ));
    }

    #[test]
    fn rejects_empty_oversized_or_non_ascii_message() {
        assert_eq!(
            encode_send_message(1, b"alice", b"pc", b""),
            Err(EncodeError::EmptyMessage)
        );
        let oversized = vec![b'a'; MAX_MESSAGE_BODY_BYTES + 1];
        assert_eq!(
            encode_send_message(1, b"alice", b"pc", &oversized),
            Err(EncodeError::MessageTooLong)
        );
        assert_eq!(
            encode_send_message(1, b"alice", b"pc", "こんにちは".as_bytes()),
            Err(EncodeError::NonAsciiMessage)
        );
    }

    #[test]
    fn rejects_nul_and_terminal_control_bytes_in_message() {
        assert_eq!(
            encode_send_message(1, b"alice", b"pc", b"hello\0world"),
            Err(EncodeError::ContainsNul)
        );
        assert_eq!(
            encode_send_message(1, b"alice", b"pc", b"hello\x1b[31m"),
            Err(EncodeError::UnsupportedControl)
        );
    }

    #[test]
    fn rejects_message_that_would_exceed_datagram_limit() {
        let body = vec![b'a'; MAX_PACKET_SIZE];
        assert_eq!(
            encode_send_message(1, b"alice", b"pc", &body),
            Err(EncodeError::MessageTooLong)
        );
    }

    #[test]
    fn rejects_unrepresentable_local_identity() {
        assert_eq!(
            encode_answer_entry(1, b"", b"pc"),
            Err(EncodeError::EmptyIdentity)
        );
        assert_eq!(
            encode_answer_entry(1, b"alice:admin", b"pc"),
            Err(EncodeError::ContainsDelimiter)
        );
        let too_long = vec![b'x'; MAX_IDENTITY_BYTES + 1];
        assert_eq!(
            encode_answer_entry(1, &too_long, b"pc"),
            Err(EncodeError::IdentityTooLong)
        );
    }

    #[test]
    fn receives_and_replies_to_the_original_source() {
        let mut transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        transport
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender
            .send_to(b"1:5:bob:remote:1:", transport.local_addr().unwrap())
            .unwrap();

        let mut buffer = [0_u8; RECEIVE_BUFFER_SIZE];
        let received = transport.recv_from(&mut buffer).unwrap();
        let event = received.classify().unwrap();
        let target = received.reply_target();
        let transition = ipmsgsec_protocol_core::apply_discovery_event(
            &ipmsgsec_protocol_core::ProtocolState::empty(),
            &event,
            target,
            10,
        )
        .unwrap();
        let action = transition.actions.get(0).unwrap();

        transport
            .send_action(&received, action, 99, b"alice", b"local")
            .unwrap();

        let mut reply = [0_u8; 128];
        let (length, source) = sender.recv_from(&mut reply).unwrap();
        assert_eq!(source, transport.local_addr().unwrap());
        let parsed = parse_packet(&reply[..length]).unwrap();
        assert_eq!(parsed.command, CommandKind::AnswerEntry);
        assert_eq!(parsed.username, b"alice");
        assert_eq!(parsed.hostname, b"local");
    }

    #[test]
    fn rejects_datagrams_larger_than_the_core_limit() {
        let mut transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        transport
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let oversized = vec![b'x'; RECEIVE_BUFFER_SIZE];
        sender
            .send_to(&oversized, transport.local_addr().unwrap())
            .unwrap();

        let mut buffer = [0_u8; RECEIVE_BUFFER_SIZE];
        let error = transport.recv_from(&mut buffer).unwrap_err();
        assert!(matches!(
            error,
            ReceiveError::PacketTooLarge {
                observed_at_least: RECEIVE_BUFFER_SIZE,
                maximum: MAX_PACKET_SIZE,
                ..
            }
        ));
    }
}
