use std::net::{SocketAddr, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

fn reserve_addresses(count: usize) -> Vec<SocketAddr> {
    let sockets: Vec<UdpSocket> = (0..count)
        .map(|_| UdpSocket::bind("127.0.0.1:0").unwrap())
        .collect();
    let addresses = sockets
        .iter()
        .map(|socket| socket.local_addr().unwrap())
        .collect();
    drop(sockets);
    addresses
}

fn spawn_instance(
    name: &str,
    bind: SocketAddr,
    announce_to: &[SocketAddr],
    duration_secs: u64,
) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ipmsgsec-cli"));
    command
        .arg("--bind")
        .arg(bind.to_string())
        .arg("--username")
        .arg(name)
        .arg("--hostname")
        .arg("loopback")
        .arg("--duration-secs")
        .arg(duration_secs.to_string());

    for target in announce_to {
        command.arg("--announce-to").arg(target.to_string());
    }

    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn spawn_message_sender(
    bind: SocketAddr,
    announce_to: SocketAddr,
    send_to: SocketAddr,
    body: &str,
    duration_secs: u64,
) -> Child {
    Command::new(env!("CARGO_BIN_EXE_ipmsgsec-cli"))
        .arg("--bind")
        .arg(bind.to_string())
        .arg("--announce-to")
        .arg(announce_to.to_string())
        .arg("--send-to")
        .arg(send_to.to_string())
        .arg("--message")
        .arg(body)
        .arg("--username")
        .arg("sender")
        .arg("--hostname")
        .arg("loopback")
        .arg("--duration-secs")
        .arg(duration_secs.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn finish(child: Child) -> String {
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn two_instances_discover_each_other_and_process_departure() {
    let addresses = reserve_addresses(2);
    let alice = spawn_instance("alice", addresses[0], &[addresses[1]], 2);
    thread::sleep(Duration::from_millis(150));
    let bob = spawn_instance("bob", addresses[1], &[addresses[0]], 4);

    let alice_output = finish(alice);
    let bob_output = finish(bob);

    assert!(
        alice_output.contains("Peer joined"),
        "alice did not observe bob:\n{alice_output}"
    );
    assert!(
        bob_output.contains("Peer joined"),
        "bob did not observe alice:\n{bob_output}"
    );
    assert!(
        bob_output.contains("Peer departed"),
        "bob did not observe alice's normal-shutdown BR_EXIT:\n{bob_output}"
    );
}

#[test]
fn three_instances_discover_the_other_two_peers() {
    let addresses = reserve_addresses(3);
    let alice = spawn_instance("alice", addresses[0], &[addresses[1], addresses[2]], 3);
    thread::sleep(Duration::from_millis(100));
    let bob = spawn_instance("bob", addresses[1], &[addresses[0], addresses[2]], 3);
    thread::sleep(Duration::from_millis(100));
    let carol = spawn_instance("carol", addresses[2], &[addresses[0], addresses[1]], 3);

    for (name, output) in [
        ("alice", finish(alice)),
        ("bob", finish(bob)),
        ("carol", finish(carol)),
    ] {
        let joined_count = output.matches("Peer joined").count();
        assert!(
            joined_count >= 2,
            "{name} observed only {joined_count} joins:\n{output}"
        );
    }
}

#[test]
fn sender_negotiates_utf8_and_delivers_unicode_message() {
    let addresses = reserve_addresses(2);
    let receiver = spawn_instance("receiver", addresses[0], &[addresses[1]], 3);
    thread::sleep(Duration::from_millis(150));
    let sender = spawn_message_sender(addresses[1], addresses[0], addresses[0], "こんにちは", 2);

    let receiver_output = finish(receiver);
    let sender_output = finish(sender);

    assert!(
        sender_output.contains("using Utf8"),
        "sender did not select negotiated UTF-8:\n{sender_output}"
    );
    assert!(
        receiver_output.contains("こんにちは"),
        "receiver did not decode the UTF-8 message:\n{receiver_output}"
    );
}

#[test]
fn sender_delivers_message_to_another_cli_instance() {
    let addresses = reserve_addresses(2);
    let receiver = spawn_instance("receiver", addresses[0], &[addresses[1]], 3);
    thread::sleep(Duration::from_millis(150));
    let sender = spawn_message_sender(
        addresses[1],
        addresses[0],
        addresses[0],
        "hello: from cli",
        2,
    );

    let receiver_output = finish(receiver);
    let sender_output = finish(sender);

    assert!(
        sender_output.contains("Sent SENDMSG"),
        "sender did not report successful UDP send:\n{sender_output}"
    );
    assert!(
        receiver_output.contains("Message from sender@loopback"),
        "receiver did not display sender identity:\n{receiver_output}"
    );
    assert!(
        receiver_output.contains("hello: from cli"),
        "receiver did not preserve message body:\n{receiver_output}"
    );
}
