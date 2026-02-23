use std::sync::Arc;

use anyhow::Result;
use bytes::Buf;
use slog::{Logger, debug, info};

use crate::{
    backend::stream::{Stream, StreamBuilder},
    ringbuf::RingBuf,
    stream::error::create_error_cb,
};

pub struct PlaybackStream {
    pub inner: Stream,
    log: Logger,
    rb: Arc<RingBuf>,
}

impl PlaybackStream {
    pub fn write(&self, buf: &mut impl Buf) {
        debug!(self.log, "Received data {} bytes", buf.remaining());
        let Self { rb, .. } = self;
        while buf.remaining() > 0 && rb.available() > 0 {
            let src = buf.chunk();
            let dst = rb.write_chunk();
            let write = usize::min(src.len(), dst.len());
            dst[..write].copy_from_slice(&src[..write]);

            buf.advance(write);
            rb.advance_write(write);
        }
    }
}

pub fn handle_playback<N, D>(log: &Logger, name: N, device: Option<D>) -> Result<PlaybackStream>
where
    N: Into<Vec<u8>>,
    D: Into<Vec<u8>>,
{
    let rb = Arc::new(RingBuf::new(65536));
    let stream_rb = Arc::clone(&rb);
    let stream = StreamBuilder::default()
        .write_cb(move |dst| {
            let rb = &stream_rb;
            let src = rb.read_chunk();
            let read = usize::min(src.len(), dst.len());

            if read >= dst.len() {
                dst[..read].copy_from_slice(&src[..read]);
                rb.advance_read(read);
            } else {
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
