use std::env;
use std::net::SocketAddr;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ipmsgsec_protocol_core::{
    ProtocolEvent, ProtocolState, TransitionOutcome, apply_discovery_event,
};
use ipmsgsec_transport_udp::{
    RECEIVE_BUFFER_SIZE, UdpTransport, encode_broadcast_entry, encode_broadcast_exit,
};

const DEFAULT_DURATION_SECS: u64 = 30;
const RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug)]
struct Config {
    bind: SocketAddr,
    announce_to: Vec<SocketAddr>,
    username: String,
    hostname: String,
    duration: Duration,
}

fn usage() -> &'static str {
    r#"Usage: ipmsgsec-cli --bind <IPv4:port> --announce-to <IPv4:port>...
       --username <ASCII-name> --hostname <ASCII-host> [--duration-secs <seconds>]

Example (LAN broadcast):
  ipmsgsec-cli --bind 0.0.0.0:2425 --announce-to 255.255.255.255:2425 \
    --username alice --hostname workstation

Each --announce-to may be repeated. For same-host tests, bind each instance
to a different 127.0.0.1 port and announce to the other instances' addresses."#
}

fn parse_config<I>(args: I) -> Result<Config, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut bind = None;
    let mut announce_to = Vec::new();
    let mut username = None;
    let mut hostname = None;
    let mut duration_secs = DEFAULT_DURATION_SECS;

    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            return Err(usage().to_owned());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}\n{}", usage()))?;
        match flag.as_str() {
            "--bind" => {
                bind = Some(SocketAddr::from_str(&value).map_err(|e| format!("invalid --bind: {e}"))?);
            }
            "--announce-to" => {
                announce_to.push(
                    SocketAddr::from_str(&value)
                        .map_err(|e| format!("invalid --announce-to: {e}"))?,
                );
            }
            "--username" => username = Some(value),
            "--hostname" => hostname = Some(value),
            "--duration-secs" => {
                duration_secs = value
                    .parse::<u64>()
                    .map_err(|e| format!("invalid --duration-secs: {e}"))?;
                if duration_secs == 0 {
                    return Err("--duration-secs must be greater than zero".to_owned());
                }
            }
            _ => return Err(format!("unknown argument: {flag}\n{}", usage())),
        }
    }

    let bind = bind.ok_or_else(|| format!("--bind is required\n{}", usage()))?;
    if !bind.is_ipv4() {
        return Err("this CLI milestone supports IPv4 only".to_owned());
    }
    if announce_to.is_empty() {
        return Err("at least one --announce-to is required".to_owned());
    }
    if announce_to.iter().any(|address| !address.is_ipv4()) {
        return Err("all --announce-to addresses must be IPv4".to_owned());
    }

    let username = username.ok_or_else(|| "--username is required".to_owned())?;
    let hostname = hostname.ok_or_else(|| "--hostname is required".to_owned())?;
    for (label, value) in [("--username", &username), ("--hostname", &hostname)] {
        if value.is_empty() || !value.is_ascii() || value.contains(':') {
            return Err(format!("{label} must be non-empty ASCII and must not contain ':'"));
        }
    }

    Ok(Config {
        bind,
        announce_to,
        username,
        hostname,
        duration: Duration::from_secs(duration_secs),
    })
}

struct PacketNumbers {
    next: u64,
}

impl PacketNumbers {
    fn from_system_clock() -> Result<Self, String> {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?;
        let seed = u64::try_from(elapsed.as_nanos())
            .unwrap_or(u64::MAX.saturating_sub(1));
        Ok(Self { next: seed })
    }

    fn take(&mut self) -> Result<u64, String> {
        let value = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| "packet-number space exhausted".to_owned())?;
        Ok(value)
    }
}

fn display_peers(state: &ProtocolState) {
    println!("Peers ({}):", state.peers().len());
    for peer in state.peers().peers() {
        println!(
            "  {}@{}",
            String::from_utf8_lossy(peer.username()),
            String::from_utf8_lossy(peer.hostname())
        );
    }
}

fn run(config: Config) -> Result<(), String> {
    let mut transport = UdpTransport::bind(config.bind)
        .map_err(|error| format!("cannot bind {}: {error}", config.bind))?;
    transport
        .set_broadcast(true)
        .map_err(|error| format!("cannot enable IPv4 broadcast: {error}"))?;
    transport
        .set_read_timeout(Some(RECEIVE_POLL_INTERVAL))
        .map_err(|error| format!("cannot set receive timeout: {error}"))?;

    let mut packet_numbers = PacketNumbers::from_system_clock()?;
    let entry_number = packet_numbers.take()?;
    let entry = encode_broadcast_entry(
        entry_number,
        config.username.as_bytes(),
        config.hostname.as_bytes(),
    )
    .map_err(|error| format!("cannot encode BR_ENTRY: {error}"))?;

    for target in &config.announce_to {
        transport
            .send_to(*target, &entry)
            .map_err(|error| format!("cannot send BR_ENTRY to {target}: {error}"))?;
        println!("Sent BR_ENTRY to {target}");
    }

    println!(
        "Listening on {} for {} seconds (Ctrl+C terminates without a guaranteed BR_EXIT).",
        transport.local_addr().map_err(|e| e.to_string())?,
        config.duration.as_secs()
    );

    let started = Instant::now();
    let mut state = ProtocolState::empty();
    let mut buffer = [0_u8; RECEIVE_BUFFER_SIZE];

    while started.elapsed() < config.duration {
        let received = match transport.recv_from(&mut buffer) {
            Ok(datagram) => datagram,
            Err(ipmsgsec_transport_udp::ReceiveError::Io(error))
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(error) => {
                eprintln!("receive error: {error}");
                continue;
            }
        };

        let event = match received.classify() {
            Ok(event) => event,
            Err(error) => {
                eprintln!("ignored malformed/unsupported datagram from {}: {error:?}", received.source());
                continue;
            }
        };

        let transition = match apply_discovery_event(
            &state,
            &event,
            received.reply_target(),
            started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        ) {
            Ok(transition) => transition,
            Err(error) => {
                eprintln!("ignored event from {}: {error:?}", received.source());
                continue;
            }
        };

        let changed = transition.peer_outcome != TransitionOutcome::Unchanged;
        state = transition.state;

        if let Some(action) = transition.actions.get(0) {
            let packet_number = packet_numbers.take()?;
            if let Err(error) = transport.send_action(
                &received,
                action,
                packet_number,
                config.username.as_bytes(),
                config.hostname.as_bytes(),
            ) {
                eprintln!("could not send ANSENTRY to {}: {error}", received.source());
            }
        }

        if changed {
            match event {
                ProtocolEvent::PeerJoined { .. } => println!("Peer joined from {}", received.source()),
                ProtocolEvent::PeerDeparted(_) => println!("Peer departed from {}", received.source()),
                ProtocolEvent::MessageReceived { .. } => {}
            }
            display_peers(&state);
        }
    }

    let exit_number = packet_numbers.take()?;
    let exit = encode_broadcast_exit(
        exit_number,
        config.username.as_bytes(),
        config.hostname.as_bytes(),
    )
    .map_err(|error| format!("cannot encode BR_EXIT: {error}"))?;
    for target in &config.announce_to {
        if let Err(error) = transport.send_to(*target, &exit) {
            eprintln!("could not send BR_EXIT to {target}: {error}");
        } else {
            println!("Sent BR_EXIT to {target}");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let config = match parse_config(env::args().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    match run(config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> Vec<String> {
        [
            "--bind", "127.0.0.1:2425",
            "--announce-to", "127.0.0.1:2426",
            "--username", "alice",
            "--hostname", "workstation",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    #[test]
    fn parses_explicit_local_configuration() {
        let config = parse_config(args()).unwrap();
        assert_eq!(config.bind, "127.0.0.1:2425".parse().unwrap());
        assert_eq!(config.announce_to, vec!["127.0.0.1:2426".parse().unwrap()]);
        assert_eq!(config.username, "alice");
        assert_eq!(config.hostname, "workstation");
        assert_eq!(config.duration, Duration::from_secs(DEFAULT_DURATION_SECS));
    }

    #[test]
    fn requires_explicit_bind_and_announcement_target() {
        let mut values = args();
        values.drain(0..2);
        assert!(parse_config(values).is_err());

        let mut values = args();
        values.drain(2..4);
        assert!(parse_config(values).is_err());
    }

    #[test]
    fn rejects_non_ascii_or_delimiter_identity() {
        let mut values = args();
        let index = values.iter().position(|value| value == "alice").unwrap();
        values[index] = "ユーザー".to_owned();
        assert!(parse_config(values).is_err());

        let mut values = args();
        let index = values.iter().position(|value| value == "alice").unwrap();
        values[index] = "alice:admin".to_owned();
        assert!(parse_config(values).is_err());
    }

    #[test]
    fn accepts_multiple_announcement_targets() {
        let mut values = args();
        values.extend(
            ["--announce-to", "127.0.0.1:2427"]
                .into_iter()
                .map(str::to_owned),
        );
        let config = parse_config(values).unwrap();
        assert_eq!(config.announce_to.len(), 2);
    }
}
