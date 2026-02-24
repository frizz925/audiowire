use std::sync::Arc;

use anyhow::Result;
use bytes::Buf;
use slog::{Logger, debug, info, warn};

use crate::{
    backend::{
        config::Config,
        stream::{Stream, StreamBuilder},
    },
    ringbuf::RingBuf,
    stream::error::create_error_cb,
};

pub struct PlaybackStream {
    pub inner: Stream,
    log: Logger,
    rb: Arc<RingBuf<u8>>,
}

impl PlaybackStream {
    pub fn write(&self, buf: &mut impl Buf) {
        debug!(self.log, "Received data {} bytes", buf.remaining());
        let Self { rb, .. } = self;
        while buf.remaining() > 0 && rb.available() > 0 {
            let mut dst = rb.write_chunks();
            buf.advance(dst.write(buf.chunk()));
        }
    }
}

pub fn handle_playback<N, D>(
    log: &Logger,
    config: Config,
    name: N,
    device: Option<D>,
) -> Result<PlaybackStream>
where
    N: Into<Vec<u8>>,
    D: Into<Vec<u8>>,
{
    let rb = Arc::new(RingBuf::new(config.max_buffer_size()));
    let stream_rb = Arc::clone(&rb);
    let stream_log = log.to_owned();
    let stream = StreamBuilder::new(config)
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
    Ok(PlaybackStream {
        inner: stream,
        log: log.to_owned(),
        rb,
    })
}
