use std::{net::SocketAddr, sync::Arc};

use bytes::{BufMut, BytesMut};
use slog::{Logger, debug, error, info, warn};
use tokio::{net::UdpSocket, sync::Notify, task::JoinHandle};

use crate::{
    backend::{
        config::Config,
        result::Result,
        stream::{Stream, StreamBuilder},
    },
    ringbuf::RingBuf,
    stream::error::create_error_cb,
};

pub trait SerializeFn<B: BufMut>: Fn(&[u8], &mut B) {}

impl<B: BufMut, F: Fn(&[u8], &mut B)> SerializeFn<B> for F {}

pub struct RecordStream {
    pub inner: Stream,
    notify: Arc<Notify>,
    handle: JoinHandle<()>,
}

impl AsRef<Stream> for RecordStream {
    fn as_ref(&self) -> &Stream {
        &self.inner
    }
}

impl Drop for RecordStream {
    fn drop(&mut self) {
        self.notify.notify_waiters();
        self.handle.abort();
    }
}

struct RecordWorker {
    log: Logger,
    config: Config,

    rb: RingBuf<u8>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,

    data_notify: Notify,
    cancel_notify: Arc<Notify>,
}

impl RecordWorker {
    fn write(&self, src: &[u8]) {
        let mut dst = self.rb.write_chunks();
        if dst.available() >= src.len() {
            dst.write(src);
            self.data_notify.notify_one();
        } else {
            warn!(
                self.log, "Buffer underflow!";
                "requested" => src.len(),
                "available" => dst.available()
            );
        }
    }

    async fn run(&self, serialize: impl SerializeFn<BytesMut>) {
        let Self { config, rb, .. } = self;
        let mut buf = BytesMut::with_capacity(65536);
        let bufsize = config.buffer_size();
        loop {
            tokio::select! {
                _ = self.data_notify.notified() => (),
                _ = self.cancel_notify.notified() => break
            }
            let mut src = rb.read_chunks();
            while src.remaining() >= bufsize {
                buf.put_bytes(0, bufsize);
                src.read(&mut buf);

                let src = buf.split();
                serialize(&src, &mut buf);
                self.send(&buf).await;
                buf.unsplit(src);
                buf.clear();
            }
        }
    }

    async fn send(&self, buf: &[u8]) {
        match self.sock.send_to(buf, self.addr).await {
            Ok(send) => debug!(self.log, "Sent data {send} bytes"),
            Err(e) => error!(self.log, "Failed to send packet"; "error" => e),
        }
    }
}

pub fn handle_record<N, D>(
    log: &Logger,
    config: Config,
    name: N,
    device: Option<D>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    serialize: impl SerializeFn<BytesMut> + Send + Sync + 'static,
) -> Result<RecordStream>
where
    N: Into<Vec<u8>>,
    D: Into<Vec<u8>>,
{
    let worker = Arc::new(RecordWorker {
        log: log.clone(),
        config: config.clone(),

        rb: RingBuf::new(config.max_buffer_size()),
        sock,
        addr,

        data_notify: Notify::new(),
        cancel_notify: Arc::new(Notify::new()),
    });

    let stream = {
        let worker = Arc::clone(&worker);
        StreamBuilder::new(config)
            .read_cb(move |src| worker.write(src))
            .error_cb(create_error_cb(log.clone()))
            .start(name, device)?
    };
    info!(
        log,
        "Using record device: {}",
        stream.device_name().unwrap_or("unknown")
    );

    let notify = Arc::clone(&worker.cancel_notify);
    let handle = tokio::spawn(async move { worker.run(serialize).await });
    Ok(RecordStream {
        inner: stream,
        notify,
        handle,
    })
}
