use std::{net::SocketAddr, sync::Arc};

use bytes::Bytes;
use slog::{Logger, debug, error, info};
use tokio::{
    net::UdpSocket,
    sync::{Notify, mpsc},
    task::JoinHandle,
};

use crate::{
    backend::{
        result::Result,
        stream::{Stream, StreamBuilder},
    },
    stream::error::create_error_cb,
};

pub trait SerializeFn: Fn(Bytes) -> Bytes {}

impl<F: Fn(Bytes) -> Bytes> SerializeFn for F {}

pub struct RecordStream {
    pub inner: Stream,

    notify: Arc<Notify>,
    handle: JoinHandle<()>,
}

impl Drop for RecordStream {
    fn drop(&mut self) {
        self.notify.notify_waiters();
        self.handle.abort();
    }
}

struct RecordWorker {
    log: Logger,
    notify: Arc<Notify>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    rx: mpsc::Receiver<Bytes>,
}

impl RecordWorker {
    async fn start(&mut self, serialize: impl SerializeFn) -> std::io::Result<()> {
        while self.run(&serialize).await? {}
        Ok(())
    }

    async fn run(&mut self, serialize: impl SerializeFn) -> std::io::Result<bool> {
        tokio::select! {
            opt = self.rx.recv() => {
                if let Some(buf) = opt {
                    self.send_stream(serialize, buf).await?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ = self.notify.notified() => {
                Ok(false)
            }
        }
    }

    async fn send_stream(
        &mut self,
        serialize: impl SerializeFn,
        buf: Bytes,
    ) -> std::io::Result<()> {
        let send = self
            .sock
            .send_to(serialize(buf).as_ref(), &self.addr)
            .await?;
        debug!(self.log, "Sent data {} bytes", send);
        Ok(())
    }
}

pub fn handle_record<N, D>(
    log: &Logger,
    name: N,
    device: Option<D>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    serialize: impl SerializeFn + Send + Sync + 'static,
) -> Result<RecordStream>
where
    N: Into<Vec<u8>>,
    D: Into<Vec<u8>>,
{
    let (tx, rx) = mpsc::channel::<Bytes>(512);
    let stream = StreamBuilder::default()
        .read_cb(move |buf: &[u8]| {
            tx.blocking_send(Bytes::copy_from_slice(buf)).ok();
        })
        .error_cb(create_error_cb(log.clone()))
        .start(name, device)?;
    info!(
        log,
        "Using record device: {}",
        stream.device_name().unwrap_or("unknown")
    );

    let notify = Arc::new(Notify::new());
    let mut worker = RecordWorker {
        log: log.to_owned(),
        notify: Arc::clone(&notify),
        sock,
        addr,
        rx,
    };
    let handle = tokio::spawn(async move {
        if let Err(e) = worker.start(serialize).await {
            error!(worker.log, "Worker error"; "error" => e);
        }
    });

    Ok(RecordStream {
        inner: stream,
        notify,
        handle,
    })
}
