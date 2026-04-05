use std::{
    net::UdpSocket,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use slog::{Logger, error};

use crate::{
    TIME_SYNC_INTERVAL,
    packet::{
        command::server::{ServerCommand, ServerTimeSync},
        socket::{SharedUdpWrapper, UdpWrapper, wrap_udp},
        stream::StreamId,
    },
    server::{
        SharedClientMap,
        client::{Client, ClientRunning},
    },
};

pub struct TimeSyncWorker {
    log: Logger,
    sock: SharedUdpWrapper<Arc<UdpSocket>>,
    clients: SharedClientMap,
    notify: Arc<(Mutex<()>, Condvar, AtomicBool)>,
}

impl TimeSyncWorker {
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
        loop {
            let (update, _) = cvar.wait_timeout(guard, TIME_SYNC_INTERVAL).unwrap();
            guard = update;
            if !running.load(Ordering::Acquire) {
                break;
            }
            Self::sync_all(&log, &clients, &mut sock);
        }
    }

    fn sync_all(
        log: &Logger,
        clients: &SharedClientMap,
        sock: &mut SharedUdpWrapper<Arc<UdpSocket>>,
    ) {
        for (stream_id, client) in clients.read().unwrap().iter() {
            let client = if let Client::Running(c) = client {
                c
            } else {
                continue;
            };
            Self::sync(log, sock, *stream_id, client);
        }
    }

    fn sync(
        log: &Logger,
        sock: &mut SharedUdpWrapper<Arc<UdpSocket>>,
        stream_id: StreamId,
        client: &ClientRunning,
    ) {
        let timestamp = client.local_epoch.elapsed();
        let command = ServerCommand::TimeSync(ServerTimeSync(timestamp));
        sock.send_message_to(command, client.addr)
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
