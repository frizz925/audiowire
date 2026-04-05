use std::{
    io::Read,
    net::SocketAddr,
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};

use slog::{Logger, error, info};

use crate::{
    packet::{data::IncomingAudioData, stream::StreamFlags},
    stream::Peer,
};

pub enum Client {
    Handshake(ClientHandshake),
    Running(ClientRunning),
}

impl From<ClientHandshake> for Client {
    fn from(value: ClientHandshake) -> Self {
        Self::Handshake(value)
    }
}

impl From<ClientRunning> for Client {
    fn from(value: ClientRunning) -> Self {
        Self::Running(value)
    }
}

pub struct ClientHandshake {
    pub flags: StreamFlags,
    pub org_timestamp: Instant,
    pub last_handshake: Instant,
}

pub struct ClientRunning {
    inner: Peer,
    pub addr: SocketAddr,
    pub last_heartbeat: Instant,
    pub local_epoch: Instant,
}

impl ClientRunning {
    pub fn new(peer: Peer, addr: SocketAddr, local_epoch: Instant) -> Self {
        Self {
            inner: peer,
            addr,
            last_heartbeat: Instant::now(),
            local_epoch,
        }
    }

    pub fn handle_data<R: Read>(&mut self, log: &Logger, reader: R) -> std::io::Result<()> {
        self.write(IncomingAudioData::deserialize(reader)?)
            .map_err(
                |e| error!(log, "Failed to write incoming audio data"; "error" => e.to_string()),
            )
            .ok();
        Ok(())
    }

    pub fn maybe_update_addr(&mut self, log: &Logger, addr: &SocketAddr) {
        if self.addr.eq(addr) {
            return;
        }
        let addr = addr.to_owned();
        info!(log, "Client address changed"; "old_addr" => self.addr);
        if let Some(record) = self.record.as_ref() {
            record.update_addr(addr);
            self.addr = addr;
        }
    }

    pub fn maybe_update_remote_epoch(&mut self, timestamp: Duration) {
        if let Some(playback) = self.playback.as_mut() {
            playback.update_remote_epoch(timestamp);
        }
    }
}

impl Deref for ClientRunning {
    type Target = Peer;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ClientRunning {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
