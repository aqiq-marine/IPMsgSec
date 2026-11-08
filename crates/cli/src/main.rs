use std::env;
use std::net::SocketAddr;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::{Duration, Instant};

use ipmsgsec_app::{AppEvent, PeerIdentityDto, PeerInfo, TextDecodeFailure};

const DEFAULT_DURATION_SECS: u64 = 30;
const RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug)]
struct Config {
    bind: SocketAddr,
    announce_to: Vec<SocketAddr>,
    send_to: Vec<SocketAddr>,
    message: Option<String>,
    username: String,
    hostname: String,
    duration: Duration,
}

fn usage() -> &'static str {
    r#"Usage: ipmsgsec-cli --bind <IPv4:port> --announce-to <IPv4:port>...
       --username <ASCII-name> --hostname <ASCII-host>
       [--send-to <IPv4:port>... --message <text>]
       [--duration-secs <seconds>]

Example (LAN broadcast and a direct text message):
  ipmsgsec-cli --bind 0.0.0.0:2425 --announce-to 255.255.255.255:2425 \
    --send-to 192.0.2.20:2425 --message "Hello from IPMsgSec" \
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
    let mut send_to = Vec::new();
    let mut message = None;
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
                bind =
                    Some(SocketAddr::from_str(&value).map_err(|e| format!("invalid --bind: {e}"))?);
            }
            "--announce-to" => {
                announce_to.push(
                    SocketAddr::from_str(&value)
                        .map_err(|e| format!("invalid --announce-to: {e}"))?,
                );
            }
            "--send-to" => {
                send_to.push(
                    SocketAddr::from_str(&value).map_err(|e| format!("invalid --send-to: {e}"))?,
                );
            }
            "--message" => message = Some(value),
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
    if send_to.iter().any(|address| !address.is_ipv4()) {
        return Err("all --send-to addresses must be IPv4".to_owned());
    }
    if send_to.is_empty() != message.is_none() {
        return Err("--send-to and --message must be supplied together".to_owned());
    }
    if let Some(body) = &message {
        if body.is_empty() {
            return Err("--message must not be empty".to_owned());
        }
        if body
            .chars()
            .any(|ch| ch == '\0' || (ch.is_control() && ch != '\n' && ch != '\t'))
        {
            return Err(
                "--message may contain only text, TAB, and LF; NUL and other controls are rejected"
                    .to_owned(),
            );
        }
    }

    let username = username.ok_or_else(|| "--username is required".to_owned())?;
    let hostname = hostname.ok_or_else(|| "--hostname is required".to_owned())?;
    for (label, value) in [("--username", &username), ("--hostname", &hostname)] {
        if value.is_empty() || !value.is_ascii() || value.contains(':') {
            return Err(format!(
                "{label} must be non-empty ASCII and must not contain ':'"
            ));
        }
    }

    Ok(Config {
        bind,
        announce_to,
        send_to,
        message,
        username,
        hostname,
        duration: Duration::from_secs(duration_secs),
    })
}

fn display_peers(peers: &[PeerInfo]) {
    println!("Peers ({}):", peers.len());
    for peer in peers {
        println!(
            "  {}@{}",
            peer.identity.username.escape_ascii(),
            peer.identity.hostname.escape_ascii()
        );
        println!("    UTF-8 capability: {}", peer.supports_utf8);
    }
}

fn display_message(
    sender: &PeerIdentityDto,
    source: SocketAddr,
    body: &[u8],
    encoding: ipmsgsec_app::TextEncoding,
    decoded_text: Option<&str>,
    decode_error: Option<TextDecodeFailure>,
) {
    if let Some(text) = decoded_text {
        println!(
            "Message from {}@{} via {} ({encoding:?}): {}",
            sender.username.escape_ascii(),
            sender.hostname.escape_ascii(),
            source,
            text.escape_debug()
        );
    } else {
        println!(
            "Message from {}@{} via {} ({encoding:?}, decode error {decode_error:?}): \\x{}",
            sender.username.escape_ascii(),
            sender.hostname.escape_ascii(),
            source,
            body.iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join("")
        );
    }
}

fn run(config: Config) -> Result<(), String> {
    let mut app_config = ipmsgsec_app::ApplicationConfig::new(
        config.bind,
        config.announce_to.clone(),
        config.username.as_bytes().to_vec(),
        config.hostname.as_bytes().to_vec(),
    );
    app_config.command_queue_capacity = 64;
    app_config.event_queue_capacity = 128;
    let service = ipmsgsec_app::ApplicationService::start(app_config)
        .map_err(|error| format!("cannot start application session: {error}"))?;
    let handle = service.handle();

    let mut pending_send_to = config.send_to.clone();
    if !pending_send_to.is_empty() {
        println!(
            "Waiting for peer capability advertisements before sending to: {}",
            pending_send_to
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    println!(
        "Listening on {} for {} seconds (Ctrl+C terminates without a guaranteed BR_EXIT).",
        service.local_address(),
        config.duration.as_secs()
    );

    let started = Instant::now();
    while started.elapsed() < config.duration {
        let remaining = config.duration.saturating_sub(started.elapsed());
        match service.recv_event_timeout(remaining.min(RECEIVE_POLL_INTERVAL)) {
            Ok(ipmsgsec_app::SessionEvent::Started { local_address }) => {
                println!("Session started on {local_address}");
            }
            Ok(ipmsgsec_app::SessionEvent::Protocol { source, event }) => match event {
                AppEvent::MessageReceived {
                    sender,
                    body,
                    encoding,
                    decoded_text,
                    decode_error,
                } => display_message(
                    &sender,
                    source,
                    &body,
                    encoding,
                    decoded_text.as_deref(),
                    decode_error,
                ),
                AppEvent::PeerJoined { change, .. } => {
                    println!("Peer joined from {source} ({change:?})");
                    if let Some(index) = pending_send_to.iter().position(|target| *target == source)
                    {
                        let target = pending_send_to.remove(index);
                        if let Some(body) = &config.message {
                            match handle.send_message(target, body.clone()) {
                                Ok(encoding) => {
                                    println!("Sent SENDMSG to {target} using {encoding:?}")
                                }
                                Err(error) => {
                                    eprintln!("could not send SENDMSG to {target}: {error}")
                                }
                            }
                        }
                    }
                    if let Ok(snapshot) = service.snapshot() {
                        let peers: Vec<_> = snapshot
                            .peers
                            .into_iter()
                            .map(|endpoint| endpoint.peer)
                            .collect();
                        display_peers(&peers);
                    }
                }
                AppEvent::PeerDeparted { change, .. } => {
                    println!("Peer departed from {source} ({change:?})");
                    if let Ok(snapshot) = service.snapshot() {
                        let peers: Vec<_> = snapshot
                            .peers
                            .into_iter()
                            .map(|endpoint| endpoint.peer)
                            .collect();
                        display_peers(&peers);
                    }
                }
            },
            Ok(ipmsgsec_app::SessionEvent::MessageSubmitted { .. }) => {}
            Ok(ipmsgsec_app::SessionEvent::TransportError { message }) => {
                eprintln!("transport/application error: {message}");
            }
            Ok(ipmsgsec_app::SessionEvent::EventsDropped { count }) => {
                eprintln!("warning: application event queue dropped {count} event(s)");
            }
            Ok(ipmsgsec_app::SessionEvent::Stopped) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("application event stream disconnected".to_owned());
            }
        }
    }

    if !pending_send_to.is_empty() {
        eprintln!(
            "message was not sent because no capability advertisement was received from: {}",
            pending_send_to
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    service
        .shutdown()
        .map_err(|error| format!("application session shutdown failed: {error}"))?;
    Ok(())
}

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", usage());
        return ExitCode::SUCCESS;
    }

    let config = match parse_config(arguments) {
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
            "--bind",
            "127.0.0.1:2425",
            "--announce-to",
            "127.0.0.1:2426",
            "--username",
            "alice",
            "--hostname",
            "workstation",
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
    fn requires_send_target_and_message_as_a_pair() {
        let mut values = args();
        values.extend(["--message", "hello"].into_iter().map(str::to_owned));
        assert!(parse_config(values).is_err());

        let mut values = args();
        values.extend(
            ["--send-to", "127.0.0.1:2427"]
                .into_iter()
                .map(str::to_owned),
        );
        assert!(parse_config(values).is_err());
    }

    #[test]
    fn accepts_explicit_ascii_message_targets() {
        let mut values = args();
        values.extend(
            ["--send-to", "127.0.0.1:2427", "--message", "hello: world"]
                .into_iter()
                .map(str::to_owned),
        );
        let config = parse_config(values).unwrap();
        assert_eq!(config.send_to, vec!["127.0.0.1:2427".parse().unwrap()]);
        assert_eq!(config.message.as_deref(), Some("hello: world"));
    }

    #[test]
    fn accepts_unicode_message_and_rejects_terminal_controls() {
        let mut values = args();
        values.extend(
            ["--send-to", "127.0.0.1:2427", "--message", "こんにちは"]
                .into_iter()
                .map(str::to_owned),
        );
        assert_eq!(
            parse_config(values).unwrap().message.as_deref(),
            Some("こんにちは")
        );

        for body in ["hello\0world", "hello\u{1b}[31m"] {
            let mut values = args();
            values.extend(
                ["--send-to", "127.0.0.1:2427", "--message", body]
                    .into_iter()
                    .map(str::to_owned),
            );
            assert!(parse_config(values).is_err(), "accepted {body:?}");
        }
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
