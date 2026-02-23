use std::collections::VecDeque;

use anyhow::Result;
use bytes::Bytes;
use slog::{Logger, info};
use tokio::sync::mpsc;

use crate::{
    backend::stream::{Stream, StreamBuilder},
    stream::error::create_error_cb,
};

pub struct PlaybackStream {
    pub inner: Stream,
    tx: mpsc::Sender<Bytes>,
}

impl PlaybackStream {
    pub async fn write(&self, buf: Bytes) -> Result<(), mpsc::error::SendError<Bytes>> {
        self.tx.send(buf).await
    }
}

pub fn handle_playback<N, D>(log: &Logger, name: N, device: Option<D>) -> Result<PlaybackStream>
where
    N: Into<Vec<u8>>,
    D: Into<Vec<u8>>,
{
    let mut buf = VecDeque::<u8>::with_capacity(65536);
    let (tx, mut rx) = mpsc::channel::<Bytes>(5);
    let stream = StreamBuilder::default()
        .write_cb(move |dst| {
            if let Ok(src) = rx.try_recv() {
                buf.extend(src.into_iter());
            }
            let requested = dst.len();
            if buf.len() >= requested {
                let slice = buf.make_contiguous();
                dst.copy_from_slice(&slice[..requested]);
                // TODO: Remove the head from VecDeque
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
    Ok(PlaybackStream { inner: stream, tx })
}
