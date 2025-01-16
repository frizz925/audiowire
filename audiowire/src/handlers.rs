use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{error::Error, time::Duration};

use slog::{error, info, o, Logger};
use tokio::time::sleep;

use crate::peer::PeerWriteHalf;

use super::audiowire::{
    start_playback, start_record, Config, PlaybackStream, RecordStream, Stream,
};
use super::opus::ChannelsParser;
use super::peer::PeerReadHalf;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn error_cb(err: i32, message: &str, userdata: *mut c_void) {
    let logger = unsafe { &ptr::read(userdata as *mut Logger) };
    error!(logger, "Error {}: {}", err, message);
}

pub fn handle_signal() -> Result<Arc<AtomicBool>> {
    let term = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&term))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;
    Ok(term)
}

pub async fn handle_playback<P: PeerReadHalf>(
    term: Arc<AtomicBool>,
    config: Config,
    device: Option<String>,
    name: String,
    root_logger: &Logger,
    peer: P,
) -> Result<()> {
    let channels = config.channels;
    let decoder = opus::Decoder::new(
        config.sample_rate,
        opus::Channels::from_u8(config.channels)?,
    )?;

    let mut stream = start_playback(
        device.as_deref(),
        &name,
        config,
        Some(error_cb),
        Some(root_logger.clone()),
    )?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device.to_owned())),
        None => root_logger.new(o!()),
    };
    info!(
        logger,
        "Playback started, buffer samples: {}", config.max_buffer_frames
    );

    handle_playback_stream(term, &mut stream, channels as usize, peer, decoder)
        .await
        .map_err(|err| error!(logger, "Peer stream error: {}", err))
        .unwrap_or_default();

    stream.stop()?;
    info!(logger, "Playback stopped");
    Ok(())
}

async fn handle_playback_stream<P: PeerReadHalf>(
    term: Arc<AtomicBool>,
    stream: &mut PlaybackStream,
    channels: usize,
    mut peer: P,
    mut decoder: opus::Decoder,
) -> Result<()> {
    let mut buf = [0i16; 65536];
    let mut tmp = [0u8; 8192];
    while !term.load(Ordering::Relaxed) {
        let (head, tail) = tmp.split_at_mut(size_of::<u16>());
        peer.read_exact(head).await?;
        let length = u16::from_be_bytes(head.try_into().unwrap()) as usize;
        let data = &mut tail[..length];
        peer.read_exact(data).await?;
        let fcount = decoder.decode(data, &mut buf, false)?;

        let data = convert_slice(&buf, channels * fcount);
        if stream.peek() >= data.len() {
            stream.write(data);
        }
    }
    Ok(())
}

pub async fn handle_record<P: PeerWriteHalf>(
    term: Arc<AtomicBool>,
    config: Config,
    device: Option<String>,
    name: String,
    root_logger: &Logger,
    peer: P,
) -> Result<()> {
    let bufsize = config.buffer_size();
    let interval = config.buffer_duration();
    let encoder = opus::Encoder::new(
        config.sample_rate,
        opus::Channels::from_u8(config.channels)?,
        opus::Application::Audio,
    )?;

    let mut stream = start_record(
        device.as_deref(),
        &name,
        config,
        Some(error_cb),
        Some(root_logger.clone()),
    )?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device.to_owned())),
        None => root_logger.new(o!()),
    };
    info!(
        logger,
        "Record started, buffer samples: {}", config.max_buffer_frames
    );

    handle_record_stream(term, &mut stream, bufsize, interval, peer, encoder)
        .await
        .map_err(|err| error!(logger, "Peer stream error: {}", err))
        .unwrap_or_default();

    stream.stop()?;
    info!(logger, "Record stopped");
    Ok(())
}

async fn handle_record_stream<P: PeerWriteHalf>(
    term: Arc<AtomicBool>,
    stream: &mut RecordStream,
    bufsize: usize,
    interval: Duration,
    mut peer: P,
    mut encoder: opus::Encoder,
) -> Result<()> {
    let mut tmp = [0u8; 65536];
    let mut buf = [0u8; 8192];
    while !term.load(Ordering::Relaxed) {
        while stream.peek() >= bufsize {
            let (head, tail) = buf.split_at_mut(size_of::<u16>());
            let read = stream.read(&mut tmp[..bufsize]);
            let size = encoder.encode(convert_slice(&tmp, read), tail)?;
            let size_buf = (size as u16).to_be_bytes();
            let end = size_buf.len() + size;
            head.clone_from_slice(size_buf.as_slice());
            peer.write_all(&buf[..end]).await?;
        }
        sleep(interval).await;
    }
    Ok(())
}

fn convert_slice<S: Sized, T: Sized>(buf: &[S], len: usize) -> &[T] {
    let src_size = std::mem::size_of::<S>();
    let dst_size = std::mem::size_of::<T>();
    unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const T, len * src_size / dst_size) }
}
