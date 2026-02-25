use std::{net::SocketAddr, sync::Arc};

use bytes::{Buf, BufMut, BytesMut};
use slog::{Logger, debug, error, info, warn};
use tokio::{net::UdpSocket, sync::Notify, task::JoinHandle};

use crate::{
    backend::{
        config::Config,
        result::Result,
        stream::{Stream, StreamBuilder},
    },
    opus::{ChannelsParser, convert_slice},
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

    rb: Arc<RingBuf<u8>>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    encoder: Option<(opus::Encoder, Vec<u8>)>,

    data_notify: Arc<Notify>,
    cancel_notify: Arc<Notify>,
}

impl RecordWorker {
    async fn run(mut self, serialize: impl SerializeFn<BytesMut>) {
        let mut buf = BytesMut::with_capacity(65536);
        loop {
            tokio::select! {
                _ = self.data_notify.notified() => (),
                _ = self.cancel_notify.notified() => break
            }
            self.read(&mut buf, &serialize).await;
        }
    }

    async fn read(&mut self, buf: &mut BytesMut, serialize: impl SerializeFn<BytesMut>) {
        let bufsize = self.config.buffer_size();
        let mut src = self.rb.read_chunks();
        while src.remaining() >= bufsize {
            buf.clear();
            buf.put_bytes(0, bufsize);
            let read = src.read(buf);

            let src = &buf[..read];
            if let Some((enc, tmp)) = &mut self.encoder {
                let len = enc.encode(convert_slice(src), tmp).unwrap();
                buf.advance(buf.remaining());
                buf.put_slice(&tmp[..len]);
            }

            let src = buf.split();
            serialize(&src, buf);
            self.send(&buf).await;
            buf.advance(buf.len());
            buf.unsplit(src);
        }
    }

    async fn send(&self, buf: &[u8]) {
        match self.sock.send_to(buf, self.addr).await {
            Ok(send) => debug!(self.log, "Sent data {send} bytes"),
            Err(e) => error!(self.log, "Failed to send packet"; "error" => e),
        }
    }
}

unsafe impl Sync for RecordWorker {}
unsafe impl Send for RecordWorker {}

struct RecordProducer {
    log: Logger,
    rb: Arc<RingBuf<u8>>,
    notify: Arc<Notify>,
}

impl RecordProducer {
    fn write(&self, src: &[u8]) {
        let Self { log, rb, notify } = self;
        let mut dst = rb.write_chunks();
        if dst.available() >= src.len() {
            dst.write(src);
            notify.notify_one();
        } else {
            warn!(
                log, "Buffer underflow!";
                "requested" => src.len(),
                "available" => dst.available()
            );
        }
    }
}

pub fn handle_record<N, D>(
    log: &Logger,
    config: &Config,
    name: N,
    device: Option<D>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    serialize: impl SerializeFn<BytesMut> + Send + Sync + 'static,
    opus_enabled: bool,
) -> Result<RecordStream>
where
    N: Into<Vec<u8>>,
    D: Into<Vec<u8>>,
{
    let encoder = if opus_enabled {
        let enc = opus::Encoder::new(
            config.sample_rate,
            opus::Channels::from_u8(config.channels),
            opus::Application::LowDelay,
        )
        .unwrap();
        let buf = vec![0u8; 65536];
        Some((enc, buf))
    } else {
        None
    };

    let rb = Arc::new(RingBuf::new(config.max_buffer_size()));
    let notify = Arc::new(Notify::new());
    let worker = RecordWorker {
        log: log.clone(),
        config: config.clone(),

        rb: Arc::clone(&rb),
        sock,
        addr,
        encoder,

        data_notify: Arc::clone(&notify),
        cancel_notify: Arc::new(Notify::new()),
    };

    let stream = {
        let producer = RecordProducer {
            log: log.clone(),
            rb,
            notify,
        };
        StreamBuilder::new(config.clone())
            .read_cb(move |src| producer.write(src))
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
