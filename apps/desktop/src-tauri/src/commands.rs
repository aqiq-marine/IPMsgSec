use std::net::SocketAddr;

use ipmsgsec_app::{ApplicationConfig, PeerInfo};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::state::DesktopState;

#[derive(Serialize)]
pub struct DesktopPeerEndpoint {
    pub address: String,
    pub peer: PeerInfo,
}

#[derive(Serialize)]
pub struct DesktopSessionSnapshot {
    pub running: bool,
    pub peers: Vec<DesktopPeerEndpoint>,
}

#[tauri::command]
pub fn start_session(
    app: AppHandle,
    state: State<'_, DesktopState>,
    bind_address: String,
    announce_to: Vec<String>,
    username: String,
    hostname: String,
) -> Result<String, String> {
    let bind_address: SocketAddr = bind_address
        .parse()
        .map_err(|_| "bind address must be an IPv4 socket address".to_owned())?;
    if !bind_address.is_ipv4() {
        return Err("bind address must be IPv4".to_owned());
    }

    if announce_to.is_empty() {
        return Err("at least one discovery target is required".to_owned());
    }
    let targets = announce_to
        .into_iter()
        .map(|value| {
            let address: SocketAddr = value
                .parse()
                .map_err(|_| format!("invalid discovery target: {value}"))?;
            if !address.is_ipv4() {
                return Err(format!("discovery target must be IPv4: {value}"));
            }
            Ok(address)
        })
        .collect::<Result<Vec<_>, String>>()?;

    let config = ApplicationConfig::new(
        bind_address,
        targets,
        username.into_bytes(),
        hostname.into_bytes(),
    );
    state.start(app, config).map(|address| address.to_string())
}

#[tauri::command]
pub fn discover(state: State<'_, DesktopState>) -> Result<(), String> {
    state.handle()?.discover().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, DesktopState>) -> Result<DesktopSessionSnapshot, String> {
    let snapshot = state
        .handle()?
        .snapshot()
        .map_err(|error| error.to_string())?;
    Ok(DesktopSessionSnapshot {
        running: snapshot.running,
        peers: snapshot
            .peers
            .into_iter()
            .map(|endpoint| DesktopPeerEndpoint {
                address: endpoint.address.to_string(),
                peer: endpoint.peer,
            })
            .collect(),
    })
}

#[tauri::command]
pub fn send_message(
    state: State<'_, DesktopState>,
    target: String,
    text: String,
) -> Result<String, String> {
    let target: SocketAddr = target
        .parse()
        .map_err(|_| "target must be an IPv4 socket address".to_owned())?;
    if !target.is_ipv4() {
        return Err("target must be IPv4".to_owned());
    }
    state
        .handle()?
        .send_message(target, text)
        .map(|encoding| match encoding {
            ipmsgsec_app::TextEncoding::Cp932 => "cp932".to_owned(),
            ipmsgsec_app::TextEncoding::Utf8 => "utf8".to_owned(),
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn stop_session(state: State<'_, DesktopState>) -> Result<(), String> {
    state.shutdown()
}
