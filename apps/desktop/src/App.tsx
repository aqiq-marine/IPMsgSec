import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Peer = {
  address: string;
  peer: {
    identity: { username: number[]; hostname: number[] };
    supports_utf8: boolean;
    first_seen_at: number;
  };
};
type Snapshot = { running: boolean; peers: Peer[] };
type ProtocolEvent = {
  type: string;
  peer?: Peer["peer"];
  sender?: { username: number[]; hostname: number[] };
  decoded_text?: string | null;
  decode_error?: string | null;
  change?: string;
};
type SessionEvent =
  | { type: "started"; local_address: string }
  | { type: "protocol"; source: string; event: ProtocolEvent }
  | { type: "message_submitted"; target: string; encoding: string }
  | { type: "transport_error"; message: string }
  | { type: "events_dropped"; count: number }
  | { type: "stopped" };

function decodeIdentity(bytes: number[]) {
  return new TextDecoder().decode(new Uint8Array(bytes));
}
function identityLabel(peer: Peer["peer"]) {
  return decodeIdentity(peer.identity.username) + "@" + decodeIdentity(peer.identity.hostname);
}

function App() {
  const [bindAddress, setBindAddress] = useState("0.0.0.0:2425");
  const [announceTargets, setAnnounceTargets] = useState("");
  const [username, setUsername] = useState("");
  const [hostname, setHostname] = useState("");
  const [localAddress, setLocalAddress] = useState<string | null>(null);
  const [peers, setPeers] = useState<Peer[]>([]);
  const [selectedTarget, setSelectedTarget] = useState("");
  const [message, setMessage] = useState("");
  const [activity, setActivity] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const running = localAddress !== null;
  const selectedPeer = useMemo(
    () => peers.find((peer) => peer.address === selectedTarget),
    [peers, selectedTarget],
  );

  async function refreshSnapshot() {
    try {
      const snapshot = await invoke<Snapshot>("get_snapshot");
      setPeers(snapshot.peers);
      setSelectedTarget((current) =>
        snapshot.peers.some((peer) => peer.address === current)
          ? current
          : snapshot.peers[0]?.address ?? "",
      );
    } catch (cause) {
      setError(String(cause));
    }
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<SessionEvent>("session-event", ({ payload }) => {
      if (payload.type === "started") {
        setLocalAddress(payload.local_address);
        setActivity((items) => [ "Session started on " + payload.local_address, ...items].slice(0, 30));
      } else if (payload.type === "protocol") {
        const event = payload.event;
        if (event.type === "peer_joined" && event.peer) {
          setActivity((items) => [ "Peer discovered: " + identityLabel(event.peer!) + " (" + payload.source + ")", ...items].slice(0, 30));
          void refreshSnapshot();
        } else if (event.type === "peer_departed") {
          setActivity((items) => [ "Peer departed: " + payload.source, ...items].slice(0, 30));
          void refreshSnapshot();
        } else if (event.type === "message_received") {
          const sender = event.sender
            ? decodeIdentity(event.sender.username) + "@" + decodeIdentity(event.sender.hostname)
            : "Unknown sender";
          setActivity((items) => [ "Message from " + sender + ": " + (event.decoded_text ?? "[decode failed]"), ...items].slice(0, 30));
        }
      } else if (payload.type === "message_submitted") {
        setActivity((items) => [ "UDP message submitted to " + payload.target + " (" + payload.encoding + ")", ...items].slice(0, 30));
      } else if (payload.type === "transport_error") {
        setActivity((items) => [ "Transport: " + payload.message, ...items].slice(0, 30));
      } else if (payload.type === "events_dropped") {
        setActivity((items) => [ "Warning: " + payload.count + " UI events were dropped; refresh the peer snapshot", ...items].slice(0, 30));
        void refreshSnapshot();
      } else if (payload.type === "stopped") {
        setLocalAddress(null);
        setPeers([]);
        setActivity((items) => ["Session stopped", ...items].slice(0, 30));
      }
    }).then((stopListening) => {
      unlisten = stopListening;
    }).catch((cause) => setError(String(cause)));
    return () => unlisten?.();
  }, []);

  async function startSession(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const targets = announceTargets.split(",").map((value) => value.trim()).filter(Boolean);
      const address = await invoke<string>("start_session", {
        bindAddress,
        announceTo: targets,
        username,
        hostname,
      });
      setLocalAddress(address);
      setActivity((items) => ["Session started on " + address, ...items].slice(0, 30));
      await refreshSnapshot();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function discover() {
    setBusy(true);
    setError(null);
    try {
      await invoke("discover");
      await refreshSnapshot();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function stopSession() {
    setBusy(true);
    setError(null);
    try {
      await invoke("stop_session");
      setLocalAddress(null);
      setPeers([]);
      setActivity((items) => ["Session stopped", ...items].slice(0, 30));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function sendMessage(event: React.FormEvent) {
    event.preventDefault();
    if (!selectedTarget || !message.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const encoding = await invoke<string>("send_message", { target: selectedTarget, text: message });
      setActivity((items) => ["Message submitted to " + selectedTarget + " (" + encoding + ")", ...items].slice(0, 30));
      setMessage("");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="shell">
      <header className="titlebar">
        <div className="brand"><span className="brand-mark">IP</span><span>IPMsgSec</span></div>
        <span className={"status " + (running ? "status-online" : "")}>
          <span className="status-dot" />{running ? "Session running" : "Not connected"}
        </span>
      </header>

      <section className="workspace">
        {!running ? (
          <form className="panel setup-panel" onSubmit={startSession}>
            <p className="eyebrow">NETWORK SESSION</p>
            <h1>Start peer discovery</h1>
            <p className="description">
              Configure the IPv4 socket and one or more explicit discovery targets.
              The session keeps running when this window is hidden to the tray.
            </p>
            <label>Bind address
              <input value={bindAddress} onChange={(event) => setBindAddress(event.target.value)} required />
            </label>
            <label>Discovery target(s)
              <input value={announceTargets} onChange={(event) => setAnnounceTargets(event.target.value)}
                placeholder="192.168.1.255:2425 or 255.255.255.255:2425" required />
              <small>Enter comma-separated IPv4 socket addresses. Broadcast destinations are not selected automatically.</small>
            </label>
            <div className="two-columns">
              <label>Username
                <input value={username} onChange={(event) => setUsername(event.target.value)} required maxLength={64} />
              </label>
              <label>Hostname
                <input value={hostname} onChange={(event) => setHostname(event.target.value)} required maxLength={64} />
              </label>
            </div>
            <button type="submit" disabled={busy}>Start session</button>
          </form>
        ) : (
          <>
            <section className="panel session-panel">
              <div className="section-heading">
                <div>
                  <p className="eyebrow">NETWORK SESSION</p>
                  <h1>Peer discovery</h1>
                  <p className="description">Listening on {localAddress}. The worker remains active while the window is in the tray.</p>
                </div>
                <div className="actions">
                  <button className="secondary" onClick={discover} disabled={busy}>Discover</button>
                  <button className="quiet" onClick={stopSession} disabled={busy}>Stop session</button>
                </div>
              </div>
              <div className="peer-heading"><h2>Peers</h2><span>{peers.length} observed</span></div>
              {peers.length === 0 ? (
                <div className="empty-state">No peers discovered yet. Check the discovery target and network firewall.</div>
              ) : (
                <ul className="peer-list">
                  {peers.map((peer) => (
                    <li key={peer.address} className={selectedTarget === peer.address ? "selected" : ""}>
                      <button className="peer-select" onClick={() => setSelectedTarget(peer.address)}>
                        <span className="peer-avatar">{decodeIdentity(peer.peer.identity.username).slice(0, 2).toUpperCase()}</span>
                        <span className="peer-meta">
                          <strong>{identityLabel(peer.peer)}</strong>
                          <small>{peer.address} · {peer.peer.supports_utf8 ? "UTF-8" : "CP932"}</small>
                        </span>
                        <span className="peer-check">{selectedTarget === peer.address ? "●" : "○"}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <form className="composer" onSubmit={sendMessage}>
                <label htmlFor="message">Message {selectedPeer ? "to " + identityLabel(selectedPeer.peer) : ""}</label>
                <div className="composer-row">
                  <input id="message" value={message} onChange={(event) => setMessage(event.target.value)}
                    placeholder={selectedPeer ? "Write a message…" : "Select an observed peer first"} disabled={!selectedPeer} />
                  <button type="submit" disabled={busy || !selectedPeer || !message.trim()}>Send</button>
                </div>
                <small>UDP submission is not a delivery acknowledgement.</small>
              </form>
            </section>
            <section className="panel activity-panel">
              <div className="peer-heading"><h2>Activity</h2><span>Latest events</span></div>
              {activity.length === 0 ? <div className="empty-state">Session events will appear here.</div> : (
                <ul className="activity-list">
                  {activity.map((item, index) => <li key={String(index) + "-" + item}>{item}</li>)}
                </ul>
              )}
            </section>
          </>
        )}
        {error && <div className="error-banner">{error}</div>}
      </section>
      <footer><span>IPMsgSec 0.1.0</span><span>Protocol core · Verus proof boundary unchanged</span></footer>
    </main>
  );
}

export default App;
