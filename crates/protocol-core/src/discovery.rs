//! Pure discovery-state transitions and abstract network actions.
//!
//! The transport supplies an opaque reply target for each received datagram.
//! The verified core never interprets it as an IP address or socket address.

use crate::protocol::{PeerIdentity, PeerJoinKind, ProtocolEvent};
use crate::state::{PeerEvent, PeerState, StateError, TransitionOutcome, apply_event};
use vstd::prelude::*;

verus! {

/// Current maximum number of abstract actions emitted by one transition.
pub const MAX_ACTIONS: usize = 1;

/// Opaque token that the unverified transport maps to a datagram source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplyTarget(u64);

impl ReplyTarget {
    pub fn new(token: u64) -> Self {
        Self(token)
    }

    pub fn token(&self) -> u64 {
        self.0
    }
}

/// An abstract request for the transport to send a discovery response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolAction {
    SendAnswerEntry { target: ReplyTarget },
}

/// A fixed-capacity, ordered action list.
///
/// PR2 currently needs at most one action. A one-slot representation makes
/// that bound structural rather than relying on a runtime Vec length check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionList {
    first: Option<ProtocolAction>,
}

impl ActionList {
    pub fn len(&self) -> usize {
        if self.first.is_some() { 1 } else { 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.first.is_none()
    }

    pub fn get(&self, index: usize) -> Option<ProtocolAction> {
        if index == 0 { self.first } else { None }
    }
}

/// The discovery state contains only deterministic, bounded peer data.
#[derive(Debug, Eq, PartialEq)]
pub struct ProtocolState {
    peers: PeerState,
}

pub closed spec fn protocol_state_well_formed(state: &ProtocolState) -> bool {
    crate::state::peer_state_well_formed(&state.peers)
}

impl ProtocolState {
    pub fn empty() -> (state: Self)
        ensures
            protocol_state_well_formed(&state),
    {
        let state = Self {
            peers: PeerState::empty(),
        };
        reveal(protocol_state_well_formed);
        assert(protocol_state_well_formed(&state));
        state
    }

    pub fn peers(&self) -> &PeerState {
        &self.peers
    }
}

/// Errors are returned without a partially updated state or action list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    NonDiscoveryEvent,
    State(StateError),
}

/// Result of one discovery transition.
#[derive(Debug, Eq, PartialEq)]
pub struct DiscoveryTransition {
    pub state: ProtocolState,
    pub actions: ActionList,
    pub peer_outcome: TransitionOutcome,
}

pub closed spec fn action_list_well_formed(actions: &ActionList) -> bool {
    match actions.first {
        Some(_) => MAX_ACTIONS >= 1,
        None => true,
    }
}

pub closed spec fn actions_match_event(
    event: &ProtocolEvent,
    target: ReplyTarget,
    actions: &ActionList,
) -> bool {
    match event {
        ProtocolEvent::PeerJoined {
            kind: PeerJoinKind::BroadcastEntry,
            ..
        } => actions.first == Some(ProtocolAction::SendAnswerEntry { target }),
        _ => actions.first.is_none(),
    }
}

/// Apply one protocol event to discovery state and return abstract actions.
///
/// A broadcast entry always requests an answer-entry response, including for
/// duplicate announcements (the prior response may have been lost). An
/// answer-entry never causes a response. Message events are outside discovery
/// scope and are rejected without changing the input state.
pub fn apply_discovery_event(
    state: &ProtocolState,
    event: &ProtocolEvent<'_>,
    reply_target: ReplyTarget,
    observed_at: u64,
) -> (result: Result<DiscoveryTransition, DiscoveryError>)
    requires
        protocol_state_well_formed(state),
    ensures
        match result {
            Ok(transition) =>
                protocol_state_well_formed(&transition.state)
                && action_list_well_formed(&transition.actions)
                && actions_match_event(event, reply_target, &transition.actions),
            Err(_) => true,
        }
{
    let peer_event = match event {
        ProtocolEvent::PeerJoined { identity, .. } =>
            PeerEvent::Joined {
                username: identity.username(),
                hostname: identity.hostname(),
                observed_at,
            },
        ProtocolEvent::PeerDeparted(identity) =>
            PeerEvent::Departed {
                username: identity.username(),
                hostname: identity.hostname(),
            },
        ProtocolEvent::MessageReceived { .. } =>
            return Err(DiscoveryError::NonDiscoveryEvent),
    };

    let applied = match apply_event(&state.peers, peer_event) {
        Ok(transition) => transition,
        Err(error) => return Err(DiscoveryError::State(error)),
    };

    let actions = match event {
        ProtocolEvent::PeerJoined {
            kind: PeerJoinKind::BroadcastEntry,
            ..
        } => ActionList {
            first: Some(ProtocolAction::SendAnswerEntry {
                target: reply_target,
            }),
        },
        _ => ActionList { first: None },
    };

    let next_state = ProtocolState {
        peers: applied.state,
    };
    reveal(protocol_state_well_formed);
    assert(crate::state::peer_state_well_formed(&next_state.peers));
    assert(protocol_state_well_formed(&next_state));
    reveal(action_list_well_formed);
    assert(action_list_well_formed(&actions));
    reveal(actions_match_event);
    assert(actions_match_event(event, reply_target, &actions));

    Ok(DiscoveryTransition {
        state: next_state,
        actions,
        peer_outcome: applied.outcome,
    })
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> PeerIdentity<'static> {
        PeerIdentity::new(b"alice", b"workstation").unwrap()
    }

    fn joined(kind: PeerJoinKind) -> ProtocolEvent<'static> {
        ProtocolEvent::PeerJoined {
            identity: identity(),
            kind,
        }
    }

    #[test]
    fn broadcast_entry_adds_peer_and_requests_answer() {
        let state = ProtocolState::empty();
        let target = ReplyTarget::new(42);
        let result =
            apply_discovery_event(&state, &joined(PeerJoinKind::BroadcastEntry), target, 100)
                .unwrap();

        assert_eq!(result.peer_outcome, TransitionOutcome::Added);
        assert_eq!(result.state.peers().len(), 1);
        assert_eq!(result.state.peers().peers()[0].first_seen_at(), 100);
        assert_eq!(
            result.actions.get(0),
            Some(ProtocolAction::SendAnswerEntry { target })
        );
        assert_eq!(result.actions.len(), 1);
        assert!(state.peers().is_empty());
    }

    #[test]
    fn answer_entry_adds_peer_without_reply() {
        let state = ProtocolState::empty();
        let result = apply_discovery_event(
            &state,
            &joined(PeerJoinKind::AnswerEntry),
            ReplyTarget::new(42),
            100,
        )
        .unwrap();

        assert_eq!(result.peer_outcome, TransitionOutcome::Added);
        assert_eq!(result.state.peers().len(), 1);
        assert!(result.actions.is_empty());
    }

    #[test]
    fn duplicate_broadcast_entry_preserves_peer_and_still_replies() {
        let state = ProtocolState::empty();
        let target = ReplyTarget::new(7);
        let first =
            apply_discovery_event(&state, &joined(PeerJoinKind::BroadcastEntry), target, 10)
                .unwrap();
        let second = apply_discovery_event(
            &first.state,
            &joined(PeerJoinKind::BroadcastEntry),
            target,
            20,
        )
        .unwrap();

        assert_eq!(second.peer_outcome, TransitionOutcome::Unchanged);
        assert_eq!(second.state.peers().len(), 1);
        assert_eq!(second.state.peers().peers()[0].first_seen_at(), 10);
        assert_eq!(
            second.actions.get(0),
            Some(ProtocolAction::SendAnswerEntry { target })
        );
    }

    #[test]
    fn duplicate_answer_entry_is_a_noop_without_reply() {
        let state = ProtocolState::empty();
        let first = apply_discovery_event(
            &state,
            &joined(PeerJoinKind::AnswerEntry),
            ReplyTarget::new(7),
            10,
        )
        .unwrap();
        let second = apply_discovery_event(
            &first.state,
            &joined(PeerJoinKind::AnswerEntry),
            ReplyTarget::new(8),
            20,
        )
        .unwrap();

        assert_eq!(second.peer_outcome, TransitionOutcome::Unchanged);
        assert!(second.actions.is_empty());
        assert_eq!(second.state.peers().peers()[0].first_seen_at(), 10);
    }

    #[test]
    fn departure_removes_peer_without_actions() {
        let state = ProtocolState::empty();
        let first = apply_discovery_event(
            &state,
            &joined(PeerJoinKind::AnswerEntry),
            ReplyTarget::new(1),
            10,
        )
        .unwrap();
        let event = ProtocolEvent::PeerDeparted(identity());
        let second = apply_discovery_event(&first.state, &event, ReplyTarget::new(2), 20).unwrap();

        assert_eq!(second.peer_outcome, TransitionOutcome::Removed);
        assert!(second.state.peers().is_empty());
        assert!(second.actions.is_empty());
    }

    #[test]
    fn unknown_departure_is_noop_without_actions() {
        let state = ProtocolState::empty();
        let event = ProtocolEvent::PeerDeparted(identity());
        let result = apply_discovery_event(&state, &event, ReplyTarget::new(2), 20).unwrap();

        assert_eq!(result.peer_outcome, TransitionOutcome::Unchanged);
        assert!(result.state.peers().is_empty());
        assert!(result.actions.is_empty());
    }

    #[test]
    fn message_event_is_rejected_without_changing_input_state() {
        let state = ProtocolState::empty();
        let event = ProtocolEvent::MessageReceived {
            sender: identity(),
            body: b"hello",
        };

        assert_eq!(
            apply_discovery_event(&state, &event, ReplyTarget::new(1), 10),
            Err(DiscoveryError::NonDiscoveryEvent)
        );
        assert!(state.peers().is_empty());
    }

    #[test]
    fn full_peer_table_returns_error_without_partial_transition() {
        let mut state = ProtocolState::empty();
        for index in 0..crate::state::MAX_PEERS {
            let username = format!("user-{index}");
            let event = ProtocolEvent::PeerJoined {
                identity: PeerIdentity::new(username.as_bytes(), b"pc").unwrap(),
                kind: PeerJoinKind::AnswerEntry,
            };
            state =
                apply_discovery_event(&state, &event, ReplyTarget::new(index as u64), index as u64)
                    .unwrap()
                    .state;
        }

        let event = joined(PeerJoinKind::BroadcastEntry);
        assert_eq!(
            apply_discovery_event(&state, &event, ReplyTarget::new(99), 99),
            Err(DiscoveryError::State(StateError::TableFull))
        );
        assert_eq!(state.peers().len(), crate::state::MAX_PEERS);
    }
}
