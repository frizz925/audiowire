use std::{
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use slog::{Logger, error};

use crate::{
    TIME_SYNC_INTERVAL,
    packet::{
        command::client::{ClientCommand, ClientTimeSync},
        socket::{SharedUdpWrapper, UdpWrapper, wrap_udp},
        stream::StreamId,
    },
};

pub struct TimeSyncWorker {
    log: Logger,
    stream_id: StreamId,
    sock: SharedUdpWrapper<Arc<UdpSocket>>,
    addr: SocketAddr,
    notify: Arc<(Mutex<()>, Condvar, AtomicBool)>,
    local_epoch: Instant,
}

impl TimeSyncWorker {
    pub fn new(
        log: Logger,
        stream_id: StreamId,
        sock: Arc<UdpSocket>,
        addr: SocketAddr,
        notify: Arc<(Mutex<()>, Condvar, AtomicBool)>,
        local_epoch: Instant,
    ) -> Self {
        Self {
            log,
            stream_id,
            sock: wrap_udp(sock),
            addr,
            notify,
            local_epoch,
        }
    }

    pub fn run(self) {
        let Self {
            log,
            stream_id,
            mut sock,
            addr,
            notify,
            local_epoch,
        } = self;
        let (lock, cvar, running) = &*notify;
        let mut guard = lock.lock().unwrap();
        loop {
            let (update, _) = cvar.wait_timeout(guard, TIME_SYNC_INTERVAL).unwrap();
            guard = update;
            if !running.load(Ordering::Acquire) {
                break;
            }
            Self::sync(&log, &mut sock, stream_id, &addr, local_epoch);
        }
    }

    fn sync<S: AsRef<UdpSocket>>(
        log: &Logger,
        sock: &mut SharedUdpWrapper<S>,
        stream_id: StreamId,
        addr: &SocketAddr,
        local_epoch: Instant,
    ) {
        let timestamp = local_epoch.elapsed();
        let cmd: ClientCommand = ClientTimeSync(stream_id, timestamp).into();
        sock.send_message_to(cmd, addr)
            .map_err(|e| error!(log, "Failed to send time sync"; "error" => e))
            .ok();
    }
}
