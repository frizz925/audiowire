use std::{
    io::Cursor,
    net::{SocketAddr, UdpSocket},
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use slog::{Logger, error, info, trace};

use crate::{
    backend::{
        config::{Config, SampleFormat},
        result::Result,
        stream::{Stream, StreamBuilder},
    },
    opus::{ChannelsParser, convert_slice},
    stream::error::create_error_cb,
};

const INTERNAL_BUFFER_SIZE: usize = 65536;

pub trait SerializeFn: FnMut(&[u8], &mut Cursor<&mut [u8]>) -> std::io::Result<()> {}

impl<F: FnMut(&[u8], &mut Cursor<&mut [u8]>) -> std::io::Result<()>> SerializeFn for F {}

pub struct RecordStream {
    inner: Stream,

    updated: Arc<AtomicBool>,
    new_addr: Arc<Mutex<SocketAddr>>,
}

impl RecordStream {
    pub fn update_addr(&self, addr: SocketAddr) {
        let mut new_addr = self.new_addr.lock().unwrap();
        *new_addr = addr;
        self.updated.store(true, Ordering::Release);
    }
}

impl AsRef<Stream> for RecordStream {
    fn as_ref(&self) -> &Stream {
        &self.inner
    }
}

impl Deref for RecordStream {
    type Target = Stream;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

struct RecordProducer {
    log: Logger,
    config: Config,

    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    encoder: Option<opus::Encoder>,
    buf: [u8; INTERNAL_BUFFER_SIZE],

    updated: Arc<AtomicBool>,
    new_addr: Arc<Mutex<SocketAddr>>,
}

impl RecordProducer {
    fn maybe_update_socket(&mut self) {
        if self
            .updated
            .compare_exchange_weak(true, false, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            self.addr = *self.new_addr.lock().unwrap();
        }
    }

    fn write(&mut self, src: &[u8], mut serialize: impl SerializeFn) {
        let (start, end) = {
            let len = if let Some(enc) = &mut self.encoder {
                match self.config.sample_format {
                    SampleFormat::S16 => enc.encode(convert_slice(src), &mut self.buf),
                    SampleFormat::F32 => enc.encode_float(convert_slice(src), &mut self.buf),
                }
                .unwrap()
            } else {
                let len = src.len();
                self.buf[..len].copy_from_slice(src);
                len
            };
            let (src, buf) = self.buf.split_at_mut(len);
            let mut cur = Cursor::new(buf);
            serialize(src, &mut cur).unwrap();
            (len, len + cur.position() as usize)
        };
        let buf = &self.buf[start..end];
        self.sock
            .send_to(buf, self.addr)
            .map(|len| trace!(self.log, "Sent data {} bytes", len))
            .map_err(|e| error!(self.log, "Failed to send data packet"; "error" => e))
            .ok();
    }
}

pub fn handle_record<N, D>(
    log: &Logger,
    config: &Config,
    name: N,
    device: Option<D>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    mut serialize: impl SerializeFn + Send + Sync + 'static,
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
        Some(enc)
    } else {
        None
    };

    let updated = Arc::new(AtomicBool::new(false));
    let new_addr = Arc::new(Mutex::new(addr));
    let stream = {
        let mut producer = RecordProducer {
            log: log.clone(),
            config: config.clone(),

            sock,
            addr,
            encoder,
            buf: [0u8; 65536],

            updated: Arc::clone(&updated),
            new_addr: Arc::clone(&new_addr),
        };
        StreamBuilder::new(config.to_owned())
            .read_cb(move |src| {
                producer.maybe_update_socket();
                producer.write(src, &mut serialize);
            })
            .error_cb(create_error_cb(log.clone()))
            .start(name, device)?
    };
    info!(
        log,
        "Using record device: {}",
        stream.device_name().unwrap_or("unknown")
    );

    Ok(RecordStream {
        inner: stream,
        updated,
        new_addr,
    })
}
