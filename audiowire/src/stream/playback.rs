use std::{
    io::Read,
    ops::{Deref, Neg},
    sync::Arc,
    time::SystemTime,
};

use anyhow::Result;
use audiowire_serde::Deserialize;
use slog::{Logger, info, o, trace};

use crate::{
    backend::{
        config::Config,
        stream::{Stream, StreamBuilder},
    },
    opus::convert_slice_mut,
    packet::{data::IncomingAudioData, time::NetworkTime},
    ringbuf::RingBuf,
    stream::error::create_error_cb,
};

const INTERNAL_BUFFER_SIZE: usize = 65536;

pub struct PlaybackStream {
    log: Logger,
    inner: Stream,
    config: Config,
    decoder: Option<opus::Decoder>,

    rb: Arc<RingBuf<u8>>,
    buf: [u8; INTERNAL_BUFFER_SIZE],

    rtt: i64,
    delta: i64,
    buffer_ms: i64,
    sequence: u64,
}

impl PlaybackStream {
    pub fn write<R: Read>(&mut self, data: IncomingAudioData<R>) -> Result<()> {
        let Self {
            ref log,
            ref config,
            ref mut decoder,
            ref rb,
            ref mut buf,
            rtt,
            delta,
            buffer_ms,
            ..
        } = *self;
        let IncomingAudioData {
            sequence,
            timestamp,
            mut reader,
            ..
        } = data;
        let log = log.new(o!("sequence" => sequence));

        // Drop packets that are out of sequence
        if sequence < self.sequence {
            trace!(log, "Dropping out of sequence packet");
            return Ok(());
        } else {
            self.sequence = sequence;
        }

        // Determine if we should silently drop packets that are way outside
        // of the buffer duration
        let delay = time_delta(timestamp, SystemTime::now()) - delta - (rtt / 2);
        if delay > buffer_ms {
            trace!(log, "Dropping out of buffer window packet");
            return Ok(());
        }

        let len = usize::deserialize(&mut reader)?;
        reader.read_exact(&mut buf[..len])?;
        if let Some(dec) = decoder {
            let (src, buf) = buf.split_at_mut(len);
            let cnt = dec.decode(&src, convert_slice_mut(buf), false)?;
            let len = config.frames_to_bytes(cnt);
            write_to_ringbuf(&buf[..len], rb);
        } else {
            write_to_ringbuf(&buf[..len], rb);
        }

        Ok(())
    }
}

fn write_to_ringbuf(src: &[u8], rb: &RingBuf<u8>) {
    rb.reserve(src.len());
    rb.write_chunks().write(src);
}

impl AsRef<Stream> for PlaybackStream {
    fn as_ref(&self) -> &Stream {
        &self.inner
    }
}

impl Deref for PlaybackStream {
    type Target = Stream;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

unsafe impl Send for PlaybackStream {}
unsafe impl Sync for PlaybackStream {}

pub fn handle_playback<N, D>(
    log: &Logger,
    config: &Config,
    name: N,
    device: Option<D>,
    time: &NetworkTime,
    org_timestamp: SystemTime,
    rec_timestamp: SystemTime,
    opus_enabled: bool,
) -> Result<PlaybackStream>
where
    N: Into<Vec<u8>>,
    D: Into<Vec<u8>>,
{
    let local_dur = rec_timestamp.duration_since(org_timestamp)?;
    let remote_dur = time.xmt_timestamp.duration_since(time.rec_timestamp)?;
    let rtt = local_dur.abs_diff(remote_dur).as_millis() as i64;
    let delta = time_delta(org_timestamp, time.rec_timestamp)
        + time_delta(time.xmt_timestamp, rec_timestamp)
        - rtt;

    let rb = Arc::new(RingBuf::new(config.max_buffer_size()));
    let stream = {
        let rb = Arc::clone(&rb);
        let log = log.clone();
        let error_log = log.clone();

        let frames = config.max_buffer_frames;
        let bufsize = config.max_buffer_size();
        info!(
            log, "Using buffer size {bufsize} bytes";
            "frames" => frames, "rtt" => rtt,
        );

        StreamBuilder::new(config.clone())
            .write_cb(move |dst| {
                let mut src = rb.read_chunks();
                let remaining = src.remaining();
                let available = dst.len();
                if remaining < available {
                    dst.fill(0);
                    return;
                } else if remaining > bufsize {
                    let off = remaining - bufsize;
                    src.advance(off);
                    trace!(log, "Advanced ring buffer reader"; "offset" => off);
                }
                src.read(dst);
            })
            .error_cb(create_error_cb(error_log))
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
        log: log.to_owned(),
        inner: stream,
        config: config.to_owned(),
        decoder,

        rb,
        buf: [0u8; 65536],

        rtt,
        delta,
        buffer_ms: config.buffer_duration().as_millis() as i64,
        sequence: 0,
    })
}

fn time_delta(earlier: SystemTime, later: SystemTime) -> i64 {
    match later.duration_since(earlier) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => (e.duration().as_millis() as i64).neg(),
    }
}
