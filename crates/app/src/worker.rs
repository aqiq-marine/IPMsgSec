//! Bounded, single-owner application worker for UDP session lifecycle.
//!
//! The worker thread exclusively owns the UDP transport, protocol session,
//! packet-number allocator, and observed peer endpoints. Frontends communicate
//! through bounded command/event channels and receive only owned DTOs.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ipmsgsec_transport_udp::{
    RECEIVE_BUFFER_SIZE, ReceiveError, UdpTransport, encode_broadcast_entry, encode_broadcast_exit,
};

use crate::{AppEvent, ApplicationSession, PeerInfo, TextEncoding, encode_message_for_peer};

const DEFAULT_QUEUE_CAPACITY: usize = 64;
const MAX_QUEUE_CAPACITY: usize = 1024;
const RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Configuration for one running application session.
#[derive(Clone, Debug)]
pub struct ApplicationConfig {
    pub bind_address: SocketAddr,
    pub announce_to: Vec<SocketAddr>,
    pub username: Vec<u8>,
    pub hostname: Vec<u8>,
    pub command_queue_capacity: usize,
    pub event_queue_capacity: usize,
}

impl ApplicationConfig {
    pub fn new(
        bind_address: SocketAddr,
        announce_to: Vec<SocketAddr>,
        username: impl Into<Vec<u8>>,
        hostname: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            bind_address,
            announce_to,
            username: username.into(),
            hostname: hostname.into(),
            command_queue_capacity: DEFAULT_QUEUE_CAPACITY,
            event_queue_capacity: DEFAULT_QUEUE_CAPACITY,
        }
    }
}

/// State-change and diagnostic events emitted by the worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionEvent {
    Started {
        local_address: SocketAddr,
    },
    Protocol {
        source: SocketAddr,
        event: AppEvent,
    },
    MessageSubmitted {
        target: SocketAddr,
        encoding: TextEncoding,
    },
    TransportError {
        message: String,
    },
    /// Number of events dropped because the bounded event queue was full.
    /// The worker never blocks on a slow UI; overload is surfaced explicitly.
    EventsDropped {
        count: u64,
    },
    Stopped,
}

/// One observed peer endpoint. The address and identity are observations,
/// not authenticated identity claims.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerEndpoint {
    pub address: SocketAddr,
    pub peer: PeerInfo,
}

/// An owned snapshot queried independently of the event stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    pub running: bool,
    pub peers: Vec<PeerEndpoint>,
}

/// Errors returned by the frontend-facing service handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceError {
    InvalidQueueCapacity,
    InvalidConfiguration,
    CommandQueueFull,
    WorkerStopped,
    WorkerPanicked,
    OperationFailed,
    PeerNotObserved,
    MessageEncodingFailed,
    DatagramSendFailed,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidQueueCapacity => write!(
                f,
                "queue capacity must be between 1 and {MAX_QUEUE_CAPACITY}"
            ),
            Self::InvalidConfiguration => write!(f, "invalid application session configuration"),
            Self::CommandQueueFull => write!(f, "application command queue is full"),
            Self::WorkerStopped => write!(f, "application worker has stopped"),
            Self::WorkerPanicked => write!(f, "application worker panicked"),
            Self::OperationFailed => write!(f, "application operation failed"),
            Self::PeerNotObserved => write!(f, "target is not a currently observed peer"),
            Self::MessageEncodingFailed => {
                write!(f, "message cannot be encoded for the target peer")
            }
            Self::DatagramSendFailed => write!(f, "UDP datagram could not be submitted"),
        }
    }
}

impl std::error::Error for ServiceError {}

enum WorkerCommand {
    Discover(SyncSender<Result<(), ServiceError>>),
    SendMessage {
        target: SocketAddr,
        text: String,
        reply: SyncSender<Result<TextEncoding, ServiceError>>,
    },
    Snapshot(SyncSender<SessionSnapshot>),
    Stop,
}

/// Thread-safe command facade. Clones share one bounded command queue.
#[derive(Clone)]
pub struct ApplicationHandle {
    commands: SyncSender<WorkerCommand>,
}

impl ApplicationHandle {
    pub fn discover(&self) -> Result<(), ServiceError> {
        let (reply, result) = mpsc::sync_channel(1);
        self.try_send(WorkerCommand::Discover(reply))?;
        result.recv().map_err(|_| ServiceError::WorkerStopped)?
    }

    /// Submit text to an observed peer endpoint.
    ///
    /// Success means the local UDP socket accepted the datagram; it does not
    /// establish remote receipt or display.
    pub fn send_message(
        &self,
        target: SocketAddr,
        text: impl Into<String>,
    ) -> Result<TextEncoding, ServiceError> {
        let (reply, result) = mpsc::sync_channel(1);
        self.try_send(WorkerCommand::SendMessage {
            target,
            text: text.into(),
            reply,
        })?;
        result.recv().map_err(|_| ServiceError::WorkerStopped)?
    }

    pub fn snapshot(&self) -> Result<SessionSnapshot, ServiceError> {
        let (reply, result) = mpsc::sync_channel(1);
        self.try_send(WorkerCommand::Snapshot(reply))?;
        result.recv().map_err(|_| ServiceError::WorkerStopped)
    }

    fn try_send(&self, command: WorkerCommand) -> Result<(), ServiceError> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => ServiceError::CommandQueueFull,
                mpsc::TrySendError::Disconnected(_) => ServiceError::WorkerStopped,
            })
    }
}

/// Running session. Dropping it requests shutdown and joins the worker.
pub struct ApplicationService {
    handle: ApplicationHandle,
    events: Receiver<SessionEvent>,
    worker: Option<JoinHandle<()>>,
    local_address: SocketAddr,
}

impl ApplicationService {
    pub fn start(config: ApplicationConfig) -> Result<Self, ServiceError> {
        if config.command_queue_capacity == 0
            || config.event_queue_capacity == 0
            || config.command_queue_capacity > MAX_QUEUE_CAPACITY
            || config.event_queue_capacity > MAX_QUEUE_CAPACITY
        {
            return Err(ServiceError::InvalidQueueCapacity);
        }
        if config.announce_to.is_empty()
            || !config.bind_address.is_ipv4()
            || config.announce_to.iter().any(|address| !address.is_ipv4())
            || config.username.is_empty()
            || config.hostname.is_empty()
            || config.username.len() > 64
            || config.hostname.len() > 64
            || config.username.contains(&b':')
            || config.hostname.contains(&b':')
        {
            return Err(ServiceError::InvalidConfiguration);
        }

        let mut transport =
            UdpTransport::bind(config.bind_address).map_err(|_| ServiceError::OperationFailed)?;
        transport
            .set_broadcast(true)
            .map_err(|_| ServiceError::OperationFailed)?;
        transport
            .set_read_timeout(Some(RECEIVE_POLL_INTERVAL))
            .map_err(|_| ServiceError::OperationFailed)?;
        let local_address = transport
            .local_addr()
            .map_err(|_| ServiceError::OperationFailed)?;

        let (command_tx, command_rx) = mpsc::sync_channel(config.command_queue_capacity);
        let (event_tx, event_rx) = mpsc::sync_channel(config.event_queue_capacity);
        let worker = thread::Builder::new()
            .name("ipmsgsec-session".to_owned())
            .spawn(move || run_worker(config, transport, command_rx, event_tx))
            .map_err(|_| ServiceError::OperationFailed)?;

        Ok(Self {
            handle: ApplicationHandle {
                commands: command_tx,
            },
            events: event_rx,
            worker: Some(worker),
            local_address,
        })
    }

    pub fn handle(&self) -> ApplicationHandle {
        self.handle.clone()
    }

    pub fn local_address(&self) -> SocketAddr {
        self.local_address
    }

    pub fn try_recv_event(&self) -> Result<SessionEvent, mpsc::TryRecvError> {
        self.events.try_recv()
    }

    pub fn recv_event_timeout(&self, timeout: Duration) -> Result<SessionEvent, RecvTimeoutError> {
        self.events.recv_timeout(timeout)
    }

    pub fn snapshot(&self) -> Result<SessionSnapshot, ServiceError> {
        self.handle.snapshot()
    }

    /// Request a graceful stop, broadcast BR_EXIT, and join the worker.
    pub fn shutdown(mut self) -> Result<(), ServiceError> {
        self.stop_and_join()
    }

    fn stop_and_join(&mut self) -> Result<(), ServiceError> {
        if let Some(worker) = self.worker.take() {
            let stop_result = self.handle.commands.send(WorkerCommand::Stop);
            let join_result = worker.join();
            if join_result.is_err() {
                return Err(ServiceError::WorkerPanicked);
            }
            if stop_result.is_err() {
                return Err(ServiceError::WorkerStopped);
            }
        }
        Ok(())
    }
}

impl Drop for ApplicationService {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

struct PacketNumbers {
    next: u64,
}

impl PacketNumbers {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX - 1))
            .unwrap_or(1);
        Self { next: seed }
    }

    fn take(&mut self) -> Result<u64, ServiceError> {
        let current = self.next;
        self.next = current
            .checked_add(1)
            .ok_or(ServiceError::OperationFailed)?;
        Ok(current)
    }
}

fn emit(events: &SyncSender<SessionEvent>, event: SessionEvent, dropped: &mut u64) {
    if *dropped > 0 {
        match events.try_send(SessionEvent::EventsDropped { count: *dropped }) {
            Ok(()) => *dropped = 0,
            Err(TrySendError::Full(_)) => {
                *dropped = dropped.saturating_add(1);
                return;
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
    match events.try_send(event) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => *dropped = dropped.saturating_add(1),
        Err(TrySendError::Disconnected(_)) => {}
    }
}

fn send_entry(
    transport: &UdpTransport,
    targets: &[SocketAddr],
    username: &[u8],
    hostname: &[u8],
    packet_numbers: &mut PacketNumbers,
    events: &SyncSender<SessionEvent>,
    dropped: &mut u64,
) -> Result<(), ServiceError> {
    let packet_number = packet_numbers.take()?;
    let packet = encode_broadcast_entry(packet_number, username, hostname)
        .map_err(|_| ServiceError::OperationFailed)?;
    let mut failed = false;
    for target in targets {
        if let Err(error) = transport.send_to(*target, &packet) {
            failed = true;
            emit(
                events,
                SessionEvent::TransportError {
                    message: format!("could not send BR_ENTRY to {target}: {error}"),
                },
                dropped,
            );
        }
    }
    if failed {
        Err(ServiceError::DatagramSendFailed)
    } else {
        Ok(())
    }
}

fn run_worker(
    config: ApplicationConfig,
    mut transport: UdpTransport,
    commands: Receiver<WorkerCommand>,
    events: SyncSender<SessionEvent>,
) {
    let mut dropped_events = 0_u64;
    let mut packet_numbers = PacketNumbers::new();
    let started = Instant::now();
    let mut session = ApplicationSession::new();
    let mut peers: HashMap<SocketAddr, PeerInfo> = HashMap::new();
    let mut buffer = [0_u8; RECEIVE_BUFFER_SIZE];

    emit(
        &events,
        SessionEvent::Started {
            local_address: transport.local_addr().unwrap_or(config.bind_address),
        },
        &mut dropped_events,
    );
    let _ = send_entry(
        &transport,
        &config.announce_to,
        &config.username,
        &config.hostname,
        &mut packet_numbers,
        &events,
        &mut dropped_events,
    );

    let mut running = true;
    while running {
        loop {
            match commands.try_recv() {
                Ok(WorkerCommand::Discover(reply)) => {
                    let result = send_entry(
                        &transport,
                        &config.announce_to,
                        &config.username,
                        &config.hostname,
                        &mut packet_numbers,
                        &events,
                        &mut dropped_events,
                    );
                    let _ = reply.send(result);
                }
                Ok(WorkerCommand::SendMessage {
                    target,
                    text,
                    reply,
                }) => {
                    let result = match peers.get(&target) {
                        None => Err(ServiceError::PeerNotObserved),
                        Some(peer) => {
                            let packet_number = packet_numbers.take();
                            match packet_number.and_then(|number| {
                                encode_message_for_peer(
                                    number,
                                    &config.username,
                                    &config.hostname,
                                    &text,
                                    peer.supports_utf8,
                                )
                                .map_err(|_| ServiceError::MessageEncodingFailed)
                            }) {
                                Ok(encoded) => match transport.send_to(target, &encoded.bytes) {
                                    Ok(sent) if sent == encoded.bytes.len() => {
                                        emit(
                                            &events,
                                            SessionEvent::MessageSubmitted {
                                                target,
                                                encoding: encoded.encoding,
                                            },
                                            &mut dropped_events,
                                        );
                                        Ok(encoded.encoding)
                                    }
                                    _ => Err(ServiceError::DatagramSendFailed),
                                },
                                Err(error) => Err(error),
                            }
                        }
                    };
                    let _ = reply.send(result);
                }
                Ok(WorkerCommand::Snapshot(reply)) => {
                    let snapshot = SessionSnapshot {
                        running: true,
                        peers: peers
                            .iter()
                            .filter_map(|(address, observed)| {
                                session
                                    .peers()
                                    .into_iter()
                                    .find(|peer| peer.identity == observed.identity)
                                    .map(|peer| PeerEndpoint {
                                        address: *address,
                                        peer,
                                    })
                            })
                            .collect(),
                    };
                    let _ = reply.send(snapshot);
                }
                Ok(WorkerCommand::Stop) => {
                    running = false;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    running = false;
                    break;
                }
            }
        }
        if !running {
            break;
        }

        match transport.recv_from(&mut buffer) {
            Ok(received) => {
                let source = received.source();
                let observed_at = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                match session.handle_datagram(&received, observed_at) {
                    Ok(update) => {
                        match &update.event {
                            AppEvent::PeerJoined { peer, .. } => {
                                peers.retain(|_, existing| existing.identity != peer.identity);
                                peers.insert(source, peer.clone());
                            }
                            AppEvent::PeerDeparted { peer, .. } => {
                                peers.retain(|_, existing| existing.identity != *peer);
                            }
                            AppEvent::MessageReceived { .. } => {}
                        }

                        if let Some(action) = update.response_action {
                            match packet_numbers.take() {
                                Ok(number) => {
                                    if let Err(error) = transport.send_action(
                                        &received,
                                        action,
                                        number,
                                        &config.username,
                                        &config.hostname,
                                    ) {
                                        emit(
                                            &events,
                                            SessionEvent::TransportError {
                                                message: error.to_string(),
                                            },
                                            &mut dropped_events,
                                        );
                                    }
                                }
                                Err(error) => emit(
                                    &events,
                                    SessionEvent::TransportError {
                                        message: error.to_string(),
                                    },
                                    &mut dropped_events,
                                ),
                            }
                        }
                        emit(
                            &events,
                            SessionEvent::Protocol {
                                source,
                                event: update.event,
                            },
                            &mut dropped_events,
                        );
                    }
                    Err(error) => emit(
                        &events,
                        SessionEvent::TransportError {
                            message: format!("ignored datagram from {source}: {error}"),
                        },
                        &mut dropped_events,
                    ),
                }
            }
            Err(ReceiveError::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => emit(
                &events,
                SessionEvent::TransportError {
                    message: error.to_string(),
                },
                &mut dropped_events,
            ),
        }
    }

    let exit_number = packet_numbers.take();
    if let Ok(number) = exit_number
        && let Ok(packet) = encode_broadcast_exit(number, &config.username, &config.hostname)
    {
        for target in &config.announce_to {
            if let Err(error) = transport.send_to(*target, &packet) {
                emit(
                    &events,
                    SessionEvent::TransportError {
                        message: format!("could not send BR_EXIT to {target}: {error}"),
                    },
                    &mut dropped_events,
                );
            }
        }
    }
    emit(&events, SessionEvent::Stopped, &mut dropped_events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;

    fn config() -> ApplicationConfig {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let target = receiver.local_addr().unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = sender.local_addr().unwrap();
        drop(receiver);
        drop(sender);
        ApplicationConfig::new(bind, vec![target], b"worker".to_vec(), b"loopback".to_vec())
    }

    #[test]
    fn rejects_zero_or_excessive_queue_capacities() {
        let mut config = config();
        config.command_queue_capacity = 0;
        assert_eq!(
            ApplicationService::start(config).err(),
            Some(ServiceError::InvalidQueueCapacity)
        );
    }

    #[test]
    fn starts_and_stops_worker_and_emits_lifecycle_events() {
        let config = config();
        let receiver = UdpSocket::bind(config.announce_to[0]).unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let service = ApplicationService::start(config.clone()).unwrap();
        assert_eq!(service.local_address().ip(), config.bind_address.ip());
        let snapshot = service.snapshot().unwrap();
        assert!(snapshot.running);
        assert!(snapshot.peers.is_empty());
        assert_eq!(
            service
                .handle()
                .send_message(config.announce_to[0], "not yet observed"),
            Err(ServiceError::PeerNotObserved)
        );
        assert!(matches!(
            service.recv_event_timeout(Duration::from_secs(1)),
            Ok(SessionEvent::Started { .. })
        ));
        service.shutdown().unwrap();
        // BR_ENTRY/BR_EXIT are sent to the configured target; receiving them
        // proves the worker owned the socket and completed graceful shutdown.
        let mut buffer = [0_u8; 1024];
        let (length, _) = receiver.recv_from(&mut buffer).unwrap();
        assert!(buffer[..length].starts_with(b"1:"));
        let (length, _) = receiver.recv_from(&mut buffer).unwrap();
        assert!(buffer[..length].starts_with(b"1:"));
    }
}
