use std::sync::Arc;

use anyhow::Result;
use bytes::{Buf, BytesMut};
use slog::{Logger, info, warn};

use crate::{
    backend::{
        config::Config,
        stream::{Stream, StreamBuilder},
    },
    opus::convert_slice_mut,
    ringbuf::RingBuf,
    stream::error::create_error_cb,
};

pub struct PlaybackStream {
    pub inner: Stream,
    config: Config,
    rb: Arc<RingBuf<u8>>,
    decoder: Option<(opus::Decoder, BytesMut)>,
}

impl PlaybackStream {
    pub fn write(&mut self, buf: &mut impl Buf) -> opus::Result<()> {
        let Self {
            config,
            rb,
            decoder,
            ..
        } = self;

        let src = if let Some((dec, tmp)) = decoder {
            let cnt = dec.decode(buf.chunk(), convert_slice_mut(tmp), false)?;
            let len = config.frame_count_to_bytes(cnt);
            &tmp[..len]
        } else {
            buf.chunk()
        };

        let mut off = 0;
        while off < src.len() && rb.available() > 0 {
            let mut dst = rb.write_chunks();
            let write = dst.write(&src[off..]);
            off += write;
        }
        buf.advance(buf.remaining());

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
    let stream_rb = Arc::clone(&rb);
    let stream_log = log.to_owned();
    let stream = StreamBuilder::new(config.clone())
        .write_cb(move |dst| {
            let (rb, log) = (&stream_rb, &stream_log);
            let mut src = rb.read_chunks();
            if src.remaining() >= dst.len() {
                src.read(dst);
                src.free();
            } else {
                warn!(
                    log, "Buffer underflow!";
                    "requested" => dst.len(),
                    "remaining" => src.remaining()
                );
                dst.fill(0);
            }
        })
        .error_cb(create_error_cb(log.clone()))
        .start(name, device)?;
    info!(
        log,
        "Using playback device: {}",
        stream.device_name().unwrap_or("unknown")
    );

    let decoder = if opus_enabled {
        let dec = opus::Decoder::new(config.sample_rate, config.opus_channels()).unwrap();
        let buf = BytesMut::zeroed(config.max_buffer_size());
        Some((dec, buf))
    } else {
        None
    };

    Ok(PlaybackStream {
        inner: stream,
        config: config.clone(),
        rb,
        decoder,
    })
}
