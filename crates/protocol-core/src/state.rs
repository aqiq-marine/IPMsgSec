//! Bounded peer-table state and deterministic transitions.

use vstd::prelude::*;

verus! {

pub const MAX_PEERS: usize = 64;
pub const MAX_IDENTITY_BYTES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Peer {
    username: Vec<u8>,
    hostname: Vec<u8>,
    last_seen: u64,
}

impl Peer {
    pub fn username(&self) -> &[u8] {
        self.username.as_slice()
    }

    pub fn hostname(&self) -> &[u8] {
        self.hostname.as_slice()
    }

    pub fn last_seen(&self) -> u64 {
        self.last_seen
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerState {
    peers: Vec<Peer>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerEvent<'a> {
    Joined {
        username: &'a [u8],
        hostname: &'a [u8],
        observed_at: u64,
    },
    Departed {
        username: &'a [u8],
        hostname: &'a [u8],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateError {
    InvalidIdentity,
    TableFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionOutcome {
    Added,
    Refreshed,
    Removed,
    Unchanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionResult {
    pub state: PeerState,
    pub outcome: TransitionOutcome,
}

pub open spec fn peer_well_formed(peer: &Peer) -> bool {
    peer.username@.len() > 0
        && peer.username@.len() <= MAX_IDENTITY_BYTES as int
        && peer.hostname@.len() > 0
        && peer.hostname@.len() <= MAX_IDENTITY_BYTES as int
}

pub open spec fn same_identity(left: &Peer, right: &Peer) -> bool {
    left.username@ == right.username@
        && left.hostname@ == right.hostname@
}

pub open spec fn peer_state_well_formed(state: &PeerState) -> bool {
    state.peers@.len() <= MAX_PEERS as int
        && forall|i: int|
            0 <= i < state.peers@.len()
                ==> peer_well_formed(&state.peers@[i])
        && forall|i: int, j: int|
            0 <= i < j < state.peers@.len()
                ==> !same_identity(&state.peers@[i], &state.peers@[j])
}

/// Find the first peer with the requested byte-exact identity.
fn find_peer_index(
    peers: &Vec<Peer>,
    username: &[u8],
    hostname: &[u8],
) -> (result: Option<usize>)
    ensures
        match result {
            Some(index) =>
                index < peers.len()
                && peers@[index as int].username@ == username@
                && peers@[index as int].hostname@ == hostname@
                && forall|j: int| 0 <= j < index as int
                    ==> peers@[j].username@ != username@
                        || peers@[j].hostname@ != hostname@,
            None =>
                forall|j: int| 0 <= j < peers.len() as int
                    ==> peers@[j].username@ != username@
                        || peers@[j].hostname@ != hostname@,
        }
{
    let mut index: usize = 0;
    while index < peers.len()
        invariant
            index <= peers.len(),
            forall|j: int| 0 <= j < index as int
                ==> peers@[j].username@ != username@
                    || peers@[j].hostname@ != hostname@,
        decreases
            peers.len() - index,
    {
        if peers[index].username.as_slice() == username
            && peers[index].hostname.as_slice() == hostname
        {
            return Some(index);
        }
        index += 1;
    }
    None
}

impl PeerState {
    pub fn empty() -> (state: Self)
        ensures
            peer_state_well_formed(&state),
    {
        Self { peers: Vec::new() }
    }

    pub fn peers(&self) -> &[Peer] {
        self.peers.as_slice()
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

/// Apply one discovery event without performing I/O or reading the clock.
///
/// Identity bytes are copied only after their lengths have been validated.
/// Repeated joins refresh last_seen monotonically; stale timestamps cannot
/// move it backwards. Departures for unknown peers are harmless no-ops.
pub fn apply_event(
    state: &PeerState,
    event: PeerEvent<'_>,
) -> (result: Result<TransitionResult, StateError>)
    requires
        peer_state_well_formed(state),
    ensures
        match result {
            Ok(transition) => peer_state_well_formed(&transition.state),
            Err(_) => true,
        }
{
    let (username, hostname) = match event {
        PeerEvent::Joined { username, hostname, .. } =>
            (username, hostname),
        PeerEvent::Departed { username, hostname } =>
            (username, hostname),
    };

    if username.is_empty()
        || hostname.is_empty()
        || username.len() > MAX_IDENTITY_BYTES
        || hostname.len() > MAX_IDENTITY_BYTES
    {
        return Err(StateError::InvalidIdentity);
    }

    let found = find_peer_index(&state.peers, username, hostname);

    match event {
        PeerEvent::Joined { username, hostname, observed_at } => {
            match found {
                Some(index) => {
                    let mut next = state.clone();
                    let outcome = if observed_at > next.peers[index].last_seen {
                        next.peers[index].last_seen = observed_at;
                        TransitionOutcome::Refreshed
                    } else {
                        TransitionOutcome::Unchanged
                    };
                    Ok(TransitionResult { state: next, outcome })
                }
                None => {
                    if state.peers.len() >= MAX_PEERS {
                        return Err(StateError::TableFull);
                    }
                    let mut next = state.clone();
                    next.peers.push(Peer {
                        username: username.to_vec(),
                        hostname: hostname.to_vec(),
                        last_seen: observed_at,
                    });
                    Ok(TransitionResult {
                        state: next,
                        outcome: TransitionOutcome::Added,
                    })
                }
            }
        }
        PeerEvent::Departed { .. } => {
            match found {
                Some(index) => {
                    let mut next = state.clone();
                    next.peers.remove(index);
                    Ok(TransitionResult {
                        state: next,
                        outcome: TransitionOutcome::Removed,
                    })
                }
                None => Ok(TransitionResult {
                    state: state.clone(),
                    outcome: TransitionOutcome::Unchanged,
                }),
            }
        }
    }
}

}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined<'a>(username: &'a [u8], hostname: &'a [u8], observed_at: u64) -> PeerEvent<'a> {
        PeerEvent::Joined { username, hostname, observed_at }
    }

    fn departed<'a>(username: &'a [u8], hostname: &'a [u8]) -> PeerEvent<'a> {
        PeerEvent::Departed { username, hostname }
    }

    #[test]
    fn adds_peer_and_preserves_identity_bytes() {
        let state = PeerState::empty();
        let result = apply_event(&state, joined(b"alice", b"workstation", 10)).unwrap();
        assert_eq!(result.outcome, TransitionOutcome::Added);
        assert_eq!(result.state.len(), 1);
        assert_eq!(result.state.peers()[0].username(), b"alice");
        assert_eq!(result.state.peers()[0].hostname(), b"workstation");
        assert_eq!(result.state.peers()[0].last_seen(), 10);
        assert!(state.is_empty());
    }

    #[test]
    fn duplicate_join_refreshes_without_adding_a_second_peer() {
        let state = PeerState::empty();
        let first = apply_event(&state, joined(b"alice", b"pc", 10)).unwrap();
        let second = apply_event(&first.state, joined(b"alice", b"pc", 20)).unwrap();
        assert_eq!(second.outcome, TransitionOutcome::Refreshed);
        assert_eq!(second.state.len(), 1);
        assert_eq!(second.state.peers()[0].last_seen(), 20);
    }

    #[test]
    fn stale_join_does_not_move_timestamp_backwards() {
        let state = PeerState::empty();
        let first = apply_event(&state, joined(b"alice", b"pc", 20)).unwrap();
        let second = apply_event(&first.state, joined(b"alice", b"pc", 10)).unwrap();
        assert_eq!(second.outcome, TransitionOutcome::Unchanged);
        assert_eq!(second.state.peers()[0].last_seen(), 20);
    }

    #[test]
    fn departure_removes_matching_peer_only() {
        let state = PeerState::empty();
        let first = apply_event(&state, joined(b"alice", b"pc", 10)).unwrap();
        let second = apply_event(&first.state, joined(b"bob", b"pc", 11)).unwrap();
        let third = apply_event(&second.state, departed(b"alice", b"pc")).unwrap();
        assert_eq!(third.outcome, TransitionOutcome::Removed);
        assert_eq!(third.state.len(), 1);
        assert_eq!(third.state.peers()[0].username(), b"bob");
    }

    #[test]
    fn unknown_departure_is_a_noop() {
        let state = PeerState::empty();
        let result = apply_event(&state, departed(b"missing", b"pc")).unwrap();
        assert_eq!(result.outcome, TransitionOutcome::Unchanged);
        assert!(result.state.is_empty());
    }

    #[test]
    fn rejects_empty_and_oversized_identity_without_changing_state() {
        let state = PeerState::empty();
        assert_eq!(
            apply_event(&state, joined(b"", b"pc", 1)),
            Err(StateError::InvalidIdentity)
        );
        let long_identity = [b'x'; MAX_IDENTITY_BYTES + 1];
        assert_eq!(
            apply_event(&state, joined(&long_identity, b"pc", 1)),
            Err(StateError::InvalidIdentity)
        );
        assert!(state.is_empty());
    }

    #[test]
    fn rejects_capacity_overflow() {
        let mut state = PeerState::empty();
        for index in 0..MAX_PEERS {
            let username = format!("user-{index}");
            let result = apply_event(&state, joined(username.as_bytes(), b"pc", index as u64));
            state = result.unwrap().state;
        }
        assert_eq!(state.len(), MAX_PEERS);
        assert_eq!(
            apply_event(&state, joined(b"overflow", b"pc", 100)),
            Err(StateError::TableFull)
        );
        assert_eq!(state.len(), MAX_PEERS);
    }
}
