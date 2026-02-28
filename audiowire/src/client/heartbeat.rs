use std::{
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use slog::{Logger, error, info};

use crate::{
    HEARTBEAT_GRACE_PERIOD, HEARTBEAT_INTERVAL,
    packet::{
        command::client::{ClientCommand, ClientHeartbeat},
        socket::{SharedUdpWrapper, UdpWrapper, wrap_udp},
        stream::StreamId,
    },
};

pub struct HeartbeatWorker {
    log: Logger,
    stream_id: StreamId,
    sock: SharedUdpWrapper<Arc<UdpSocket>>,
    addr: SocketAddr,
    notify: Arc<(Mutex<()>, Condvar, AtomicBool)>,
    last_heartbeat: Arc<RwLock<Instant>>,
}

impl HeartbeatWorker {
    pub fn new(
        log: Logger,
        stream_id: StreamId,
        sock: Arc<UdpSocket>,
        addr: SocketAddr,
        notify: Arc<(Mutex<()>, Condvar, AtomicBool)>,
        last_heartbeat: Arc<RwLock<Instant>>,
    ) -> Self {
        Self {
            log,
            stream_id,
            sock: wrap_udp(sock),
            addr,
            notify,
            last_heartbeat,
        }
    }

    pub fn run(self) {
        let Self {
            log,
            stream_id,
            mut sock,
            addr,
            notify,
            last_heartbeat,
        } = self;
        let (lock, cvar, running) = &*notify;
        let mut guard = lock.lock().unwrap();
        while running.load(Ordering::Acquire) {
            let (update, _) = cvar.wait_timeout(guard, HEARTBEAT_INTERVAL).unwrap();
            guard = update;
            if !Self::check(&log, &last_heartbeat, running) {
                break;
            }
            Self::pulse(&log, &mut sock, stream_id, &addr);
        }
    }

    fn check(log: &Logger, last_heartbeat: &Arc<RwLock<Instant>>, running: &AtomicBool) -> bool {
        let elapsed = last_heartbeat.read().unwrap().elapsed();
        if elapsed > HEARTBEAT_GRACE_PERIOD {
            info!(log, "Closing due to server inactivity");
            running.store(false, Ordering::Release);
            false
        } else {
            true
        }
    }

    fn pulse<S: AsRef<UdpSocket>>(
        log: &Logger,
        sock: &mut SharedUdpWrapper<S>,
        stream_id: StreamId,
        addr: &SocketAddr,
    ) {
        let cmd: ClientCommand = ClientHeartbeat(stream_id).into();
        sock.send_message_to(cmd, addr)
            .map_err(|e| error!(log, "Failed to send heartbeat"; "error" => e))
            .ok();
    }
}
