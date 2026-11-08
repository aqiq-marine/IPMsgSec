use std::net::SocketAddr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ipmsgsec_app::{
    AppEvent, ApplicationConfig, ApplicationHandle, ApplicationService, SessionEvent, TextEncoding,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum DesktopSessionEvent {
    Started { local_address: String },
    Protocol { source: String, event: AppEvent },
    MessageSubmitted { target: String, encoding: TextEncoding },
    TransportError { message: String },
    EventsDropped { count: u64 },
    Stopped,
}

impl From<SessionEvent> for DesktopSessionEvent {
    fn from(event: SessionEvent) -> Self {
        match event {
            SessionEvent::Started { local_address } => Self::Started {
                local_address: local_address.to_string(),
            },
            SessionEvent::Protocol { source, event } => Self::Protocol {
                source: source.to_string(),
                event,
            },
            SessionEvent::MessageSubmitted { target, encoding } => Self::MessageSubmitted {
                target: target.to_string(),
                encoding,
            },
            SessionEvent::TransportError { message } => Self::TransportError { message },
            SessionEvent::EventsDropped { count } => Self::EventsDropped { count },
            SessionEvent::Stopped => Self::Stopped,
        }
    }
}

#[derive(Default)]
pub struct DesktopState {
    service: Arc<Mutex<Option<ApplicationService>>>,
    handle: Mutex<Option<ApplicationHandle>>,
    event_pump_stop: Arc<AtomicBool>,
    event_pump: Mutex<Option<JoinHandle<()>>>,
}

impl DesktopState {
    pub fn start(
        &self,
        app: AppHandle,
        config: ApplicationConfig,
    ) -> Result<SocketAddr, String> {
        let mut service_slot = self
            .service
            .lock()
            .map_err(|_| "desktop session lock is poisoned".to_owned())?;
        if service_slot.is_some() {
            return Err("a session is already running".to_owned());
        }

        let service = ApplicationService::start(config)
            .map_err(|error| format!("could not start session: {error}"))?;
        let local_address = service.local_address();
        let handle = service.handle();
        *service_slot = Some(service);
        *self
            .handle
            .lock()
            .map_err(|_| "desktop handle lock is poisoned".to_owned())? = Some(handle);

        self.event_pump_stop.store(false, Ordering::Release);
        let service = Arc::clone(&self.service);
        let stop = Arc::clone(&self.event_pump_stop);
        let pump = match thread::Builder::new()
            .name("ipmsgsec-desktop-events".to_owned())
            .spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let next_event = match service.lock() {
                        Ok(guard) => guard.as_ref().map(ApplicationService::try_recv_event),
                        Err(_) => break,
                    };
                    match next_event {
                        Some(Ok(event)) => {
                            let _ = app.emit("session-event", DesktopSessionEvent::from(event));
                        }
                        Some(Err(std::sync::mpsc::TryRecvError::Empty)) | None => {
                            thread::sleep(Duration::from_millis(20));
                        }
                        Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => break,
                    }
                }
            }) {
            Ok(pump) => pump,
            Err(error) => {
                self.handle
                    .lock()
                    .map_err(|_| "desktop handle lock is poisoned".to_owned())?
                    .take();
                if let Some(service) = service_slot.take() {
                    let _ = service.shutdown();
                }
                return Err(format!("could not start event forwarding: {error}"));
            }
        };
        *self
            .event_pump
            .lock()
            .map_err(|_| "desktop event thread lock is poisoned".to_owned())? = Some(pump);

        Ok(local_address)
    }

    pub fn handle(&self) -> Result<ApplicationHandle, String> {
        self.handle
            .lock()
            .map_err(|_| "desktop handle lock is poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "no application session is running".to_owned())
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.event_pump_stop.store(true, Ordering::Release);
        if let Some(pump) = self
            .event_pump
            .lock()
            .map_err(|_| "desktop event thread lock is poisoned".to_owned())?
            .take()
        {
            pump.join()
                .map_err(|_| "desktop event forwarding thread panicked".to_owned())?;
        }

        *self
            .handle
            .lock()
            .map_err(|_| "desktop handle lock is poisoned".to_owned())? = None;

        let service = self
            .service
            .lock()
            .map_err(|_| "desktop session lock is poisoned".to_owned())?
            .take();
        if let Some(service) = service {
            service
                .shutdown()
                .map_err(|error| format!("session shutdown failed: {error}"))?;
        }
        Ok(())
    }
}

impl Drop for DesktopState {
    fn drop(&mut self) {
        self.event_pump_stop.store(true, Ordering::Release);
        if let Ok(pump) = self.event_pump.get_mut()
            && let Some(pump) = pump.take()
        {
            let _ = pump.join();
        }
        if let Ok(handle) = self.handle.get_mut() {
            handle.take();
        }
        if let Ok(service) = self.service.get_mut()
            && let Some(service) = service.take()
        {
            let _ = service.shutdown();
        }
    }
}
