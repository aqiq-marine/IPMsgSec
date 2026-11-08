//! Bounded boundary types for untrusted IP Messenger datagrams.
//!
//! Parsing lives in the Verus-enabled parser module. The size wrapper is the
//! transport-facing boundary type; parse_packet also checks the size itself
//! so callers cannot bypass the limit by passing a raw slice.

/// A datagram that passed the application-level size bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundedDatagram<'a> {
    bytes: &'a [u8],
}

/// The input datagram exceeded MAX_PACKET_SIZE.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PacketTooLarge {
    pub actual: usize,
    pub maximum: usize,
}

impl<'a> BoundedDatagram<'a> {
    /// Validate size without allocating or copying.
    pub fn new(bytes: &'a [u8]) -> Result<Self, PacketTooLarge> {
        if bytes.len() <= MAX_PACKET_SIZE {
            Ok(Self { bytes })
        } else {
            Err(PacketTooLarge {
                actual: bytes.len(),
                maximum: MAX_PACKET_SIZE,
            })
        }
    }

    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

pub mod parser;

pub use parser::{CommandKind, MAX_PACKET_SIZE, Packet, ParseError, parse_packet};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_empty_datagram_at_boundary_layer() {
        let datagram = BoundedDatagram::new(&[]).expect("empty is within size bound");
        assert!(datagram.is_empty());
    }

    #[test]
    fn accepts_exact_limit() {
        let bytes = vec![0_u8; MAX_PACKET_SIZE];
        let datagram = BoundedDatagram::new(&bytes).expect("exact limit is accepted");
        assert_eq!(datagram.len(), MAX_PACKET_SIZE);
    }

    #[test]
    fn rejects_one_byte_over_limit() {
        let bytes = vec![0_u8; MAX_PACKET_SIZE + 1];
        let error = BoundedDatagram::new(&bytes).expect_err("oversized input is rejected");
        assert_eq!(error.actual, MAX_PACKET_SIZE + 1);
        assert_eq!(error.maximum, MAX_PACKET_SIZE);
    }
}
