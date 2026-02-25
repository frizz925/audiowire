use std::sync::Arc;

use anyhow::Result;
use slog::{Logger, info};

use crate::{
    backend::{
        config::Config,
        stream::{Stream, StreamBuilder},
    },
    opus::convert_slice_mut,
    ringbuf::RingBuf,
    stream::error::create_error_cb,
};

const INTERNAL_BUFFER_SIZE: usize = 65536;

pub struct PlaybackStream {
    pub inner: Stream,
    config: Config,
    rb: Arc<RingBuf<u8>>,
    decoder: Option<opus::Decoder>,
    buf: [u8; INTERNAL_BUFFER_SIZE],
}

impl PlaybackStream {
    pub fn write(&mut self, src: &[u8]) -> opus::Result<()> {
        let Self {
            config,
            rb,
            decoder,
            buf,
            ..
        } = self;

        let buf = if let Some(dec) = decoder {
            let cnt = dec.decode(src, convert_slice_mut(buf), false)?;
            let len = config.frame_count_to_bytes(cnt);
            &buf[..len]
        } else {
            src
        };

        let mut off = 0;
        while off < buf.len() && rb.available() > 0 {
            let mut dst = rb.write_chunks();
            let write = dst.write(&buf[off..]);
            off += write;
        }

        Ok(())
    }
}

unsafe impl Send for PlaybackStream {}
unsafe impl Sync for PlaybackStream {}

pub fn handle_playback<N, D>(
    log: &Logger,
    config: &Config,
    name: N,
    device: Option<D>,
    opus_enabled: bool,
) -> Result<PlaybackStream>
where
    N: Into<Vec<u8>>,
    D: Into<Vec<u8>>,
{
    let rb = Arc::new(RingBuf::new(config.max_buffer_size()));
    let stream = {
        let rb = Arc::clone(&rb);
        StreamBuilder::new(config.clone())
            .write_cb(move |dst| {
                let mut src = rb.read_chunks();
                let len = usize::min(src.remaining(), dst.len());
                src.read(&mut dst[..len]);
                if len < dst.len() {
                    dst[len..].fill(0);
                }
            })
            .error_cb(create_error_cb(log.clone()))
            .start(name, device)?
    };
    info!(
        log,
        "Using playback device: {}",
        stream.device_name().unwrap_or("unknown")
    );

    let decoder = if opus_enabled {
        let dec = opus::Decoder::new(config.sample_rate, config.opus_channels()).unwrap();
        Some(dec)
    } else {
        None
    };

    Ok(PlaybackStream {
        inner: stream,
        config: config.clone(),
        rb,
        decoder,
        buf: [0u8; 65536],
    })
}
