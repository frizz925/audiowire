use std::{
    net::{ToSocketAddrs, UdpSocket},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use slog::{Logger, error, info};

use crate::{
    HEARTBEAT_GRACE_PERIOD, HEARTBEAT_INTERVAL,
    packet::{
        command::server::SERVER_HEARTBEAT,
        socket::{SharedUdpWrapper, UdpWrapper, wrap_udp},
        stream::StreamId,
    },
    server::{SharedClientMap, client::Client},
};

pub struct HeartbeatWorker {
    log: Logger,
    sock: SharedUdpWrapper<Arc<UdpSocket>>,
    clients: SharedClientMap,
    notify: Arc<(Mutex<()>, Condvar, AtomicBool)>,
}

impl HeartbeatWorker {
    pub fn new(
        log: Logger,
        sock: Arc<UdpSocket>,
        clients: SharedClientMap,
        notify: Arc<(Mutex<()>, Condvar, AtomicBool)>,
    ) -> Self {
        Self {
            log,
            sock: wrap_udp(sock),
            clients,
            notify,
        }
    }

    pub fn run(self) {
        let Self {
            log,
            clients,
            mut sock,
            notify,
            ..
        } = self;
        let (lock, cvar, running) = &*notify;
        let mut guard = lock.lock().unwrap();
        while running.load(Ordering::Acquire) {
            let (update, _) = cvar.wait_timeout(guard, HEARTBEAT_INTERVAL).unwrap();
            guard = update;
            Self::check(&log, &clients, &mut sock);
        }
    }

    fn check(log: &Logger, clients: &SharedClientMap, sock: &mut SharedUdpWrapper<Arc<UdpSocket>>) {
        let mut dead_clients = Vec::new();
        for (stream_id, client) in clients.read().unwrap().iter() {
            let last_instant = match client {
                Client::Handshake(c) => c.last_handshake,
                Client::Running(c) => {
                    Self::pulse(log, sock, c.addr, *stream_id);
                    c.last_heartbeat
                }
            };
            if last_instant.elapsed() > HEARTBEAT_GRACE_PERIOD {
                dead_clients.push(*stream_id);
            }
        }
        let mut clients = clients.write().unwrap();
        for stream_id in dead_clients {
            clients.remove(&stream_id);
            info!(
                log, "Removed client due to inactivity";
                "stream_id" => stream_id
            );
        }
    }

    fn pulse<A: ToSocketAddrs>(
        log: &Logger,
        sock: &mut SharedUdpWrapper<Arc<UdpSocket>>,
        addr: A,
        stream_id: StreamId,
    ) {
        sock.send_message_to(SERVER_HEARTBEAT, addr)
            .map_err(|e| {
                error!(
                    log, "Failed to send heartbeat";
                    "stream_id" => stream_id,
                    "error" => e
                )
            })
            .ok();
    }
}
