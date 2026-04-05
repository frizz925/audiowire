pub mod handshake;
pub mod heartbeat;
pub mod time_sync;

use std::{
    io::Read,
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock},
    time::Instant,
};

use anyhow::Result;
use slog::{Logger, debug, error, info};

use crate::{
    packet::{
        command::server::{ServerCommand, ServerTimeSync},
        data::{IncomingAudioData, IncomingServerData},
        message::IncomingMessage,
    },
    stream::Peer,
};

pub struct Client {
    inner: Peer,
    log: Logger,
    last_heartbeat: Arc<RwLock<Instant>>,
    on_close: Box<dyn FnMut()>,
}

impl Client {
    pub fn new(
        peer: Peer,
        log: Logger,
        last_heartbeat: Arc<RwLock<Instant>>,
        on_close: impl FnMut() + 'static,
    ) -> Self {
        Self {
            inner: peer,
            log,
            last_heartbeat,
            on_close: Box::new(on_close),
        }
    }

    pub fn handle_packet(&mut self, mut buf: &[u8]) -> Result<()> {
        match IncomingMessage::deserialize(&mut buf)? {
            IncomingMessage::Data(buf) => {
                let data = IncomingAudioData::deserialize(IncomingServerData::deserialize(buf)?)?;
                self.handle_data(data);
            }
            IncomingMessage::ServerCommand(cmd) => self.handle_command(cmd),
            _ => (),
        }
        Ok(())
    }

    fn handle_data<R: Read>(&mut self, data: IncomingAudioData<R>) {
        self.write(data)
            .map_err(|e| error!(self.log, "Failed to write incoming audio data"; "error" => e.to_string()))
            .ok();
    }

    fn handle_command(&mut self, cmd: ServerCommand) {
        match cmd {
            ServerCommand::Heartbeat(_) => {
                debug!(self.log, "Received server heartbeat");
                let mut value = self.last_heartbeat.write().unwrap();
                *value = Instant::now();
            }
            ServerCommand::TimeSync(ServerTimeSync(timestamp)) => {
                debug!(self.log, "Received server time sync");
                if let Some(playback) = self.inner.playback.as_mut() {
                    playback.update_remote_epoch(timestamp);
                }
            }
            ServerCommand::Close(_) => {
                info!(self.log, "Server closed");
                (self.on_close)();
            }
            _ => (),
        }
    }
}

impl Deref for Client {
    type Target = Peer;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Client {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
