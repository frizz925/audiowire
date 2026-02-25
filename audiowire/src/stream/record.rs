use std::{
    io::Cursor,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
};

use slog::{Logger, debug, error, info};

use crate::{
    backend::{
        config::Config,
        result::Result,
        stream::{Stream, StreamBuilder},
    },
    opus::{ChannelsParser, convert_slice},
    stream::error::create_error_cb,
};

const INTERNAL_BUFFER_SIZE: usize = 65536;

pub trait SerializeFn: Fn(&[u8], &mut Cursor<&mut [u8]>) -> std::io::Result<()> {}

impl<F: Fn(&[u8], &mut Cursor<&mut [u8]>) -> std::io::Result<()>> SerializeFn for F {}

pub struct RecordStream {
    pub inner: Stream,
}

impl AsRef<Stream> for RecordStream {
    fn as_ref(&self) -> &Stream {
        &self.inner
    }
}

struct RecordProducer {
    log: Logger,

    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    encoder: Option<opus::Encoder>,
    buf: [u8; INTERNAL_BUFFER_SIZE],
}

impl RecordProducer {
    fn write(&mut self, src: &[u8], serialize: impl SerializeFn) {
        let (start, end) = {
            let len = if let Some(enc) = &mut self.encoder {
                enc.encode(convert_slice(src), &mut self.buf).unwrap()
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
            .map(|len| debug!(self.log, "Sent data {} bytes", len))
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
    serialize: impl SerializeFn + Send + Sync + 'static,
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

    let stream = {
        let mut producer = RecordProducer {
            log: log.clone(),
            sock,
            addr,
            encoder,
            buf: [0u8; 65536],
        };
        StreamBuilder::new(config.to_owned())
            .read_cb(move |src| producer.write(src, &serialize))
            .error_cb(create_error_cb(log.clone()))
            .start(name, device)?
    };
    info!(
        log,
        "Using record device: {}",
        stream.device_name().unwrap_or("unknown")
    );

    Ok(RecordStream { inner: stream })
}
