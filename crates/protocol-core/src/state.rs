//! Bounded peer-table state and deterministic transitions.

use vstd::prelude::*;

verus! {

pub const MAX_PEERS: usize = 64;
pub const MAX_IDENTITY_BYTES: usize = 64;

#[derive(Debug, Eq, PartialEq)]
pub struct Peer {
    username: Vec<u8>,
    hostname: Vec<u8>,
    first_seen_at: u64,
    supports_utf8: bool,
}

impl Peer {
    pub fn username(&self) -> &[u8] {
        self.username.as_slice()
    }

    pub fn hostname(&self) -> &[u8] {
        self.hostname.as_slice()
    }

    pub fn first_seen_at(&self) -> u64 {
        self.first_seen_at
    }

    pub fn supports_utf8(&self) -> bool {
        self.supports_utf8
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PeerState {
    peers: Vec<Peer>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeerEvent<'a> {
    Joined {
        username: &'a [u8],
        hostname: &'a [u8],
        observed_at: u64,
        supports_utf8: bool,
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
    Removed,
    Unchanged,
    Updated,
}

#[derive(Debug, Eq, PartialEq)]
pub struct TransitionResult {
    pub state: PeerState,
    pub outcome: TransitionOutcome,
}

closed spec fn peer_well_formed(peer: &Peer) -> bool {
    peer.username@.len() > 0
        && peer.username@.len() <= MAX_IDENTITY_BYTES as int
        && peer.hostname@.len() > 0
        && peer.hostname@.len() <= MAX_IDENTITY_BYTES as int
}

closed spec fn same_identity(left: &Peer, right: &Peer) -> bool {
    left.username@ == right.username@
        && left.hostname@ == right.hostname@
}

pub closed spec fn peer_state_well_formed(state: &PeerState) -> bool {
    state.peers@.len() <= MAX_PEERS as int
        && forall|i: int|
            0 <= i < state.peers@.len()
                ==> peer_well_formed(&state.peers@[i])
        && forall|i: int, j: int|
            0 <= i < j < state.peers@.len()
                ==> !same_identity(&state.peers@[i], &state.peers@[j])
}

closed spec fn no_peer_with_identity(
    peers: &Vec<Peer>,
    username: &[u8],
    hostname: &[u8],
) -> bool {
    forall|j: int| 0 <= j < peers@.len()
        ==> peers@[j].username@ != username@
            || peers@[j].hostname@ != hostname@
}

/// Find the first peer with the requested byte-exact identity.
#[allow(clippy::ptr_arg)]
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
                no_peer_with_identity(peers, username, hostname),
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


/// Copy a bounded byte slice without relying on an unmodelled allocation helper.
fn copy_bytes(bytes: &[u8]) -> (result: Vec<u8>)
    ensures
        result@ == bytes@,
{
    let mut result = Vec::new();
    let mut index: usize = 0;
    while index < bytes.len()
        invariant
            index <= bytes.len(),
            result@ == bytes@.take(index as int),
        decreases
            bytes.len() - index,
    {
        result.push(bytes[index]);
        index += 1;
    }
    result
}

fn copy_peer(peer: &Peer) -> (result: Peer)
    ensures
        result.username@ == peer.username@,
        result.hostname@ == peer.hostname@,
        result.first_seen_at == peer.first_seen_at,
        result.supports_utf8 == peer.supports_utf8,
{
    Peer {
        username: copy_bytes(peer.username.as_slice()),
        hostname: copy_bytes(peer.hostname.as_slice()),
        first_seen_at: peer.first_seen_at,
        supports_utf8: peer.supports_utf8,
    }
}

fn copy_state(state: &PeerState) -> (result: PeerState)
    requires
        peer_state_well_formed(state),
    ensures
        result.peers@.len() == state.peers@.len(),
        forall|i: int| 0 <= i < state.peers@.len() ==>
            result.peers@[i].username@ == state.peers@[i].username@
            && result.peers@[i].hostname@ == state.peers@[i].hostname@
            && result.peers@[i].first_seen_at == state.peers@[i].first_seen_at
            && result.peers@[i].supports_utf8 == state.peers@[i].supports_utf8,
        peer_state_well_formed(&result),
{
    let mut peers: Vec<Peer> = Vec::new();
    let mut index: usize = 0;
    while index < state.peers.len()
        invariant
            index <= state.peers.len(),
            peers@.len() == index as int,
            forall|j: int| 0 <= j < index as int ==>
                peers@[j].username@ == state.peers@[j].username@
                && peers@[j].hostname@ == state.peers@[j].hostname@
                && peers@[j].first_seen_at == state.peers@[j].first_seen_at
                && peers@[j].supports_utf8 == state.peers@[j].supports_utf8,
        decreases
            state.peers.len() - index,
    {
        let copied = copy_peer(&state.peers[index]);
        peers.push(copied);
        index += 1;
    }
    let result = PeerState { peers };
    reveal(peer_state_well_formed);
    assert(result.peers@.len() == state.peers@.len());
    assert forall|i: int| 0 <= i < state.peers@.len() implies
        result.peers@[i].username@ == state.peers@[i].username@
        && result.peers@[i].hostname@ == state.peers@[i].hostname@
        && result.peers@[i].first_seen_at == state.peers@[i].first_seen_at
        && result.peers@[i].supports_utf8 == state.peers@[i].supports_utf8
    by {
        assert(i < result.peers@.len());
    }
    reveal(peer_well_formed);
    assert forall|i: int| 0 <= i < result.peers@.len()
        implies peer_well_formed(&result.peers@[i])
    by {
        assert(peer_well_formed(&state.peers@[i]));
        assert(result.peers@[i].username@ == state.peers@[i].username@);
        assert(result.peers@[i].hostname@ == state.peers@[i].hostname@);
    }
    reveal(same_identity);
    assert forall|i: int, j: int| 0 <= i < j < result.peers@.len()
        implies !same_identity(&result.peers@[i], &result.peers@[j])
    by {
        assert(!same_identity(&state.peers@[i], &state.peers@[j]));
        assert(result.peers@[i].username@ == state.peers@[i].username@);
        assert(result.peers@[i].hostname@ == state.peers@[i].hostname@);
        assert(result.peers@[j].username@ == state.peers@[j].username@);
        assert(result.peers@[j].hostname@ == state.peers@[j].hostname@);
    }
    assert(peer_state_well_formed(&result));
    result
}


fn append_peer(
    state: &PeerState,
    username: &[u8],
    hostname: &[u8],
    observed_at: u64,
    supports_utf8: bool,
) -> (result: PeerState)
    requires
        peer_state_well_formed(state),
        username.len() > 0 && username.len() <= MAX_IDENTITY_BYTES,
        hostname.len() > 0 && hostname.len() <= MAX_IDENTITY_BYTES,
        state.peers.len() < MAX_PEERS,
        no_peer_with_identity(&state.peers, username, hostname),
    ensures
        peer_state_well_formed(&result),
{
    let mut next = copy_state(state);
    let ghost old_len = next.peers.len();
    let copied_username = copy_bytes(username);
    let copied_hostname = copy_bytes(hostname);
    assert(copied_username@ == username@);
    assert(copied_hostname@ == hostname@);
    next.peers.push(Peer {
        username: copied_username,
        hostname: copied_hostname,
        first_seen_at: observed_at,
        supports_utf8,
    });

    reveal(peer_state_well_formed);
    reveal(peer_well_formed);
    reveal(same_identity);
    assert(next.peers@.len() == old_len as int + 1);
    assert forall|i: int| 0 <= i < next.peers@.len()
        implies peer_well_formed(&next.peers@[i])
    by {
        if i < old_len as int {
            assert(next.peers@[i].username@ == state.peers@[i].username@);
            assert(next.peers@[i].hostname@ == state.peers@[i].hostname@);
            assert(peer_well_formed(&state.peers@[i]));
        } else {
            assert(i == old_len as int);
            assert(next.peers@[i].username@ == username@);
            assert(next.peers@[i].hostname@ == hostname@);
        }
    }
    assert forall|i: int, j: int| 0 <= i < j < next.peers@.len()
        implies !same_identity(&next.peers@[i], &next.peers@[j])
    by {
        if j < old_len as int {
            assert(next.peers@[i].username@ == state.peers@[i].username@);
            assert(next.peers@[i].hostname@ == state.peers@[i].hostname@);
            assert(next.peers@[j].username@ == state.peers@[j].username@);
            assert(next.peers@[j].hostname@ == state.peers@[j].hostname@);
            assert(!same_identity(&state.peers@[i], &state.peers@[j]));
        } else {
            assert(j == old_len as int);
            assert(i < old_len as int);
            assert(next.peers@[i].username@ == state.peers@[i].username@);
            assert(next.peers@[i].hostname@ == state.peers@[i].hostname@);
            assert(next.peers@[j].username@ == username@);
            assert(next.peers@[j].hostname@ == hostname@);
            assert(no_peer_with_identity(&state.peers, username, hostname));
        }
    }
    assert(next.peers@.len() <= MAX_PEERS as int);
    assert(peer_state_well_formed(&next));
    next
}

fn remove_peer_at(state: &PeerState, index: usize) -> (result: PeerState)
    requires
        peer_state_well_formed(state),
        index < state.peers.len(),
    ensures
        peer_state_well_formed(&result),
{
    let mut next = copy_state(state);
    let ghost old_peers = next.peers@;
    next.peers.remove(index);
    assert(next.peers@ == old_peers.remove(index as int));

    reveal(peer_state_well_formed);
    reveal(peer_well_formed);
    reveal(same_identity);
    assert(next.peers@.len() + 1 == old_peers.len());
    assert forall|i: int| 0 <= i < next.peers@.len()
        implies peer_well_formed(&next.peers@[i])
    by {
        if i < index as int {
            assert(next.peers@[i].username@ == old_peers[i].username@);
            assert(next.peers@[i].hostname@ == old_peers[i].hostname@);
        } else {
            assert(next.peers@[i].username@ == old_peers[i + 1].username@);
            assert(next.peers@[i].hostname@ == old_peers[i + 1].hostname@);
        }
        if i < index as int {
            assert(peer_well_formed(&old_peers[i]));
        } else {
            assert(peer_well_formed(&old_peers[i + 1]));
        }
    }
    assert forall|i: int, j: int| 0 <= i < j < next.peers@.len()
        implies !same_identity(&next.peers@[i], &next.peers@[j])
    by {
        if i < index as int {
            if j < index as int {
                assert(!same_identity(&old_peers[i], &old_peers[j]));
            } else {
                assert(!same_identity(&old_peers[i], &old_peers[j + 1]));
            }
        } else {
            assert(!same_identity(&old_peers[i + 1], &old_peers[j + 1]));
        }
    }
    assert(next.peers@.len() <= MAX_PEERS as int);
    assert(peer_state_well_formed(&next));
    next
}

/// Apply one discovery event without performing I/O or reading the clock.
///
/// Identity bytes are copied only after their lengths have been validated.
/// Repeated joins are idempotent no-ops and preserve the original
/// first_seen_at timestamp. Departures for unknown peers are harmless no-ops.
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
        PeerEvent::Joined {
            username,
            hostname,
            observed_at,
            supports_utf8,
        } => {
            match found {
                Some(index) => {
                    let mut next = copy_state(state);
                    let changed = next.peers[index].supports_utf8 != supports_utf8;
                    next.peers[index].supports_utf8 = supports_utf8;
                    Ok(TransitionResult {
                        state: next,
                        outcome: if changed {
                            TransitionOutcome::Updated
                        } else {
                            TransitionOutcome::Unchanged
                        },
                    })
                }
                None => {
                    if state.peers.len() >= MAX_PEERS {
                        return Err(StateError::TableFull);
                    }
                    let next = append_peer(state, username, hostname, observed_at, supports_utf8);
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
                    let next = remove_peer_at(state, index);
                    Ok(TransitionResult {
                        state: next,
                        outcome: TransitionOutcome::Removed,
                    })
                }
                None => Ok(TransitionResult {
                    state: copy_state(state),
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
        PeerEvent::Joined {
            username,
            hostname,
            observed_at,
            supports_utf8: false,
        }
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
        assert_eq!(result.state.peers()[0].first_seen_at(), 10);
        assert!(state.is_empty());
    }

    #[test]
    fn duplicate_join_is_idempotent_without_adding_a_second_peer() {
        let state = PeerState::empty();
        let first = apply_event(&state, joined(b"alice", b"pc", 10)).unwrap();
        let second = apply_event(&first.state, joined(b"alice", b"pc", 20)).unwrap();
        assert_eq!(second.outcome, TransitionOutcome::Unchanged);
        assert_eq!(second.state.len(), 1);
        assert_eq!(second.state.peers()[0].first_seen_at(), 10);
    }

    #[test]
    fn duplicate_join_does_not_change_the_first_observation_timestamp() {
        let state = PeerState::empty();
        let first = apply_event(&state, joined(b"alice", b"pc", 20)).unwrap();
        let second = apply_event(&first.state, joined(b"alice", b"pc", 10)).unwrap();
        assert_eq!(second.outcome, TransitionOutcome::Unchanged);
        assert_eq!(second.state.peers()[0].first_seen_at(), 20);
    }

    #[test]
    fn duplicate_join_updates_capability_without_resetting_first_seen() {
        let state = PeerState::empty();
        let first = apply_event(
            &state,
            PeerEvent::Joined {
                username: b"alice",
                hostname: b"pc",
                observed_at: 10,
                supports_utf8: false,
            },
        )
        .unwrap();
        let updated = apply_event(
            &first.state,
            PeerEvent::Joined {
                username: b"alice",
                hostname: b"pc",
                observed_at: 20,
                supports_utf8: true,
            },
        ).unwrap();

        assert_eq!(updated.outcome, TransitionOutcome::Updated);
        assert_eq!(updated.state.peers()[0].first_seen_at(), 10);
        assert!(updated.state.peers()[0].supports_utf8());
        assert!(!first.state.peers()[0].supports_utf8());
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
