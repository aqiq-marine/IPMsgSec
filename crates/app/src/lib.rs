//! UI-independent application API shared by the CLI and future desktop frontends.
//!
//! This crate owns application-level interpretation and orchestration of
//! protocol events. It deliberately exposes owned DTOs rather than borrowed
//! parser slices, socket handles, or protocol-core state.
//!
//! The session is currently synchronous and single-owner. Background worker
//! lifecycle, command queues, and event distribution are a separate milestone.

use ipmsgsec_protocol_core::{
    DiscoveryError, MessageEncoding, Peer, PeerIdentity, PeerJoinKind, ProtocolAction,
    ProtocolEvent, ProtocolState, TransitionOutcome, apply_discovery_event,
};
use ipmsgsec_transport_udp::{
    DecodeError, EncodeError, InboundError, ReceivedDatagram, decode_text_body,
    encode_send_message_with_encoding, encode_text_body,
};
use serde::{Deserialize, Serialize};

/// A byte-preserving peer identity suitable for crossing an application/UI boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerIdentityDto {
    pub username: Vec<u8>,
    pub hostname: Vec<u8>,
}

impl PeerIdentityDto {
    fn from_borrowed(identity: PeerIdentity<'_>) -> Self {
        Self {
            username: identity.username().to_vec(),
            hostname: identity.hostname().to_vec(),
        }
    }
}

/// A snapshot of one observed peer. The identity and capabilities are not authenticated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub identity: PeerIdentityDto,
    pub first_seen_at: u64,
    pub supports_utf8: bool,
}

impl PeerInfo {
    fn from_peer(peer: &Peer) -> Self {
        Self {
            identity: PeerIdentityDto {
                username: peer.username().to_vec(),
                hostname: peer.hostname().to_vec(),
            },
            first_seen_at: peer.first_seen_at(),
            supports_utf8: peer.supports_utf8(),
        }
    }
}

/// Stable application-level representation of the selected wire text encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEncoding {
    Cp932,
    Utf8,
}

impl From<MessageEncoding> for TextEncoding {
    fn from(value: MessageEncoding) -> Self {
        match value {
            MessageEncoding::Cp932 => Self::Cp932,
            MessageEncoding::Utf8 => Self::Utf8,
        }
    }
}

impl From<TextEncoding> for MessageEncoding {
    fn from(value: TextEncoding) -> Self {
        match value {
            TextEncoding::Cp932 => Self::Cp932,
            TextEncoding::Utf8 => Self::Utf8,
        }
    }
}

/// Why a received message could not be decoded without loss.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextDecodeFailure {
    InvalidUtf8,
    InvalidCp932,
}

impl From<DecodeError> for TextDecodeFailure {
    fn from(value: DecodeError) -> Self {
        match value {
            DecodeError::InvalidUtf8 => Self::InvalidUtf8,
            DecodeError::InvalidCp932 => Self::InvalidCp932,
        }
    }
}

/// Application-level peer-table mutation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerChange {
    Added,
    Removed,
    Unchanged,
    Updated,
}

impl From<TransitionOutcome> for PeerChange {
    fn from(value: TransitionOutcome) -> Self {
        match value {
            TransitionOutcome::Added => Self::Added,
            TransitionOutcome::Removed => Self::Removed,
            TransitionOutcome::Unchanged => Self::Unchanged,
            TransitionOutcome::Updated => Self::Updated,
        }
    }
}

/// Application-level event. All fields own their data and are serializable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    PeerJoined {
        peer: PeerInfo,
        kind: PeerJoinKindDto,
        change: PeerChange,
    },
    PeerDeparted {
        peer: PeerIdentityDto,
        change: PeerChange,
    },
    MessageReceived {
        sender: PeerIdentityDto,
        body: Vec<u8>,
        encoding: TextEncoding,
        decoded_text: Option<String>,
        decode_error: Option<TextDecodeFailure>,
    },
}

/// Application-level form of the discovery origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerJoinKindDto {
    BroadcastEntry,
    AnswerEntry,
}

impl From<PeerJoinKind> for PeerJoinKindDto {
    fn from(value: PeerJoinKind) -> Self {
        match value {
            PeerJoinKind::BroadcastEntry => Self::BroadcastEntry,
            PeerJoinKind::AnswerEntry => Self::AnswerEntry,
        }
    }
}

/// One processed datagram, including any abstract response for the transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionUpdate {
    pub event: AppEvent,
    pub response_action: Option<ProtocolAction>,
    pub peers_changed: bool,
}

/// Errors at the application boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationError {
    Inbound(InboundError),
    Discovery(DiscoveryError),
    Encode(EncodeError),
}

impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inbound(error) => write!(f, "invalid inbound datagram: {error:?}"),
            Self::Discovery(error) => write!(f, "discovery transition failed: {error:?}"),
            Self::Encode(error) => write!(f, "cannot encode application message: {error}"),
        }
    }
}

impl std::error::Error for ApplicationError {}

/// The application-owned view of the current verified protocol state.
///
/// A single caller should own this value. The next lifecycle milestone will
/// put it behind one background worker and expose commands/events.
#[derive(Debug)]
pub struct ApplicationSession {
    protocol_state: ProtocolState,
}

impl Default for ApplicationSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationSession {
    pub fn new() -> Self {
        Self {
            protocol_state: ProtocolState::empty(),
        }
    }

    /// Return an owned snapshot; no references into protocol-core escape.
    pub fn peers(&self) -> Vec<PeerInfo> {
        self.protocol_state
            .peers()
            .peers()
            .iter()
            .map(PeerInfo::from_peer)
            .collect()
    }

    /// Parse, classify, and apply one received datagram at the application layer.
    ///
    /// The transport remains responsible for I/O and source-address correlation.
    /// A broadcast-entry response is returned as an abstract action so the
    /// caller can send it through the matching received datagram.
    pub fn handle_datagram(
        &mut self,
        received: &ReceivedDatagram<'_>,
        observed_at: u64,
    ) -> Result<SessionUpdate, ApplicationError> {
        let event = received.classify().map_err(ApplicationError::Inbound)?;

        match event {
            ProtocolEvent::MessageReceived {
                sender,
                body,
                encoding,
            } => {
                let decoded = decode_text_body(body, encoding);
                let (decoded_text, decode_error) = match decoded {
                    Ok(text) => (Some(text), None),
                    Err(error) => (None, Some(error.into())),
                };
                Ok(SessionUpdate {
                    event: AppEvent::MessageReceived {
                        sender: PeerIdentityDto::from_borrowed(sender),
                        body: body.to_vec(),
                        encoding: encoding.into(),
                        decoded_text,
                        decode_error,
                    },
                    response_action: None,
                    peers_changed: false,
                })
            }
            ProtocolEvent::PeerJoined { identity, kind, .. } => {
                let transition = apply_discovery_event(
                    &self.protocol_state,
                    &event,
                    received.reply_target(),
                    observed_at,
                )
                .map_err(ApplicationError::Discovery)?;
                let change = PeerChange::from(transition.peer_outcome);
                self.protocol_state = transition.state;
                let peer = self
                    .protocol_state
                    .peers()
                    .peers()
                    .iter()
                    .find(|peer| {
                        peer.username() == identity.username()
                            && peer.hostname() == identity.hostname()
                    })
                    .map(PeerInfo::from_peer)
                    .expect("successful join transition retains the joined peer");
                Ok(SessionUpdate {
                    event: AppEvent::PeerJoined {
                        peer,
                        kind: kind.into(),
                        change,
                    },
                    response_action: transition.actions.get(0),
                    peers_changed: change != PeerChange::Unchanged,
                })
            }
            ProtocolEvent::PeerDeparted(identity) => {
                let transition = apply_discovery_event(
                    &self.protocol_state,
                    &event,
                    received.reply_target(),
                    observed_at,
                )
                .map_err(ApplicationError::Discovery)?;
                let change = PeerChange::from(transition.peer_outcome);
                self.protocol_state = transition.state;
                Ok(SessionUpdate {
                    event: AppEvent::PeerDeparted {
                        peer: PeerIdentityDto::from_borrowed(identity),
                        change,
                    },
                    response_action: transition.actions.get(0),
                    peers_changed: change != PeerChange::Unchanged,
                })
            }
        }
    }
}

/// Encoded outbound message, ready for the UDP transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedMessage {
    pub bytes: Vec<u8>,
    pub encoding: TextEncoding,
}

/// Encode text according to a previously observed peer capability.
///
/// This function does not imply delivery; the caller still has to submit the
/// returned datagram to the OS socket.
pub fn encode_message_for_peer(
    packet_number: u64,
    local_username: &[u8],
    local_hostname: &[u8],
    text: &str,
    peer_supports_utf8: bool,
) -> Result<EncodedMessage, ApplicationError> {
    let encoding = if peer_supports_utf8 {
        MessageEncoding::Utf8
    } else {
        MessageEncoding::Cp932
    };
    let body = encode_text_body(text, encoding).map_err(ApplicationError::Encode)?;
    let bytes = encode_send_message_with_encoding(
        packet_number,
        local_username,
        local_hostname,
        &body,
        encoding,
    )
    .map_err(ApplicationError::Encode)?;
    Ok(EncodedMessage {
        bytes,
        encoding: encoding.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipmsgsec_transport_udp::{
        RECEIVE_BUFFER_SIZE, UdpTransport, encode_broadcast_entry, encode_send_message_with_encoding,
        encode_text_body,
    };
    use std::net::UdpSocket;
    use std::time::Duration;

    fn receive_one(transport: &mut UdpTransport) -> ReceivedDatagram<'static> {
        unreachable!("helper is intentionally not used; datagrams borrow the receive buffer")
    }

    #[test]
    fn encodes_utf8_for_capable_peer_and_cp932_for_legacy_peer() {
        let utf8 = encode_message_for_peer(1, b"alice", b"host", "こんにちは", true).unwrap();
        assert_eq!(utf8.encoding, TextEncoding::Utf8);

        let cp932 = encode_message_for_peer(2, b"alice", b"host", "こんにちは", false).unwrap();
        assert_eq!(cp932.encoding, TextEncoding::Cp932);

        assert!(matches!(
            encode_message_for_peer(3, b"alice", b"host", "emoji 😀", false),
            Err(ApplicationError::Encode(EncodeError::UnrepresentableCp932))
        ));
    }

    #[test]
    fn exposes_owned_discovery_and_message_dtos() {
        let mut transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        transport
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let entry = encode_broadcast_entry(10, b"bob", b"remote").unwrap();
        sender.send_to(&entry, transport.local_addr().unwrap()).unwrap();

        let mut buffer = [0_u8; RECEIVE_BUFFER_SIZE];
        let received = transport.recv_from(&mut buffer).unwrap();
        let mut session = ApplicationSession::new();
        let update = session.handle_datagram(&received, 100).unwrap();

        assert!(matches!(
            update.event,
            AppEvent::PeerJoined {
                peer: PeerInfo { supports_utf8: true, .. },
                kind: PeerJoinKindDto::BroadcastEntry,
                change: PeerChange::Added,
            }
        ));
        assert!(update.response_action.is_some());
        assert_eq!(session.peers().len(), 1);
        assert_eq!(session.peers()[0].identity.username, b"bob");
    }

    #[test]
    fn message_event_contains_raw_bytes_and_decoded_text() {
        let mut transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        transport
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let body = encode_text_body("こんにちは", MessageEncoding::Utf8).unwrap();
        let packet = encode_send_message_with_encoding(
            11,
            b"bob",
            b"remote",
            &body,
            MessageEncoding::Utf8,
        )
        .unwrap();
        sender.send_to(&packet, transport.local_addr().unwrap()).unwrap();

        let mut buffer = [0_u8; RECEIVE_BUFFER_SIZE];
        let received = transport.recv_from(&mut buffer).unwrap();
        let mut session = ApplicationSession::new();
        let update = session.handle_datagram(&received, 100).unwrap();

        assert!(matches!(
            update.event,
            AppEvent::MessageReceived {
                encoding: TextEncoding::Utf8,
                decoded_text: Some(text),
                decode_error: None,
                ..
            } if text == "こんにちは"
        ));
    }
}
