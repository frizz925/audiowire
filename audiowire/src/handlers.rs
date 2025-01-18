use std::error::Error;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use slog::{error, info, o, Logger};
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::peer::PeerWriteHalf;
use crate::StreamBuilder;

use super::audiowire::{Config, PlaybackStream, RecordStream, Stream};
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

pub fn check_audio(
    logger: &Logger,
    config: Config,
    output: Option<&str>,
    input: Option<&str>,
) -> Result<()> {
    info!(logger, "Running audio system check");
    if output.map(|s| s != "null").unwrap_or(true) {
        let mut stream = PlaybackStream::start("playback-test", output, config)?;
        stream
            .device_name()
            .map(|s| info!(logger, "Using playback device: {}", s))
            .unwrap_or_else(|| info!(logger, "Using playback device"));
        stream.stop()?;
    }
    if input.map(|s| s != "null").unwrap_or(true) {
        let mut stream = RecordStream::start("record-test", input, config)?;
        stream
            .device_name()
            .map(|s| info!(logger, "Using record device: {}", s))
            .unwrap_or_else(|| info!(logger, "Using record device"));
        stream.stop()?;
    }
    info!(logger, "Audio system check completed");
    Ok(())
}

pub fn handle_playback<P: PeerReadHalf + Send + 'static>(
    term: Arc<AtomicBool>,
    config: Config,
    device: Option<String>,
    name: String,
    root_logger: Logger,
    peer: P,
    opus_enabled: bool,
) -> Result<JoinHandle<()>> {
    let mut stream = StreamBuilder::new(config)
        .error_cb(error_cb, Some(root_logger.clone()))
        .start_playback(&name, device.as_deref())?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device.to_owned())),
        None => root_logger.new(o!()),
    };
    info!(
        logger,
        "Playback started, buffer samples: {}", config.max_buffer_frames
    );

    let handle = tokio::spawn(async move {
        let result = if opus_enabled {
            handle_opus_playback_stream(term, &mut stream, config, peer).await
        } else {
            handle_raw_playback_stream(term, &mut stream, config, peer).await
        };

        result
            .map_err(|err| error!(logger, "Playback error: {}", err))
            .unwrap_or_default();

        if let Err(err) = stream.stop() {
            error!(logger, "Failed to stop playback stream: {}", err);
        } else {
            info!(logger, "Playback stopped");
        }
    });

    Ok(handle)
}

async fn handle_raw_playback_stream<P: PeerReadHalf>(
    term: Arc<AtomicBool>,
    stream: &mut PlaybackStream,
    config: Config,
    mut peer: P,
) -> Result<()> {
    let bufsize = config.buffer_size();
    let mut base = [0u8; 65536];
    let buf = &mut base[..bufsize];
    while !term.load(Ordering::Relaxed) {
        if stream.peek() >= bufsize {
            peer.read_exact(buf).await?;
            stream.write(buf);
        }
    }
    Ok(())
}

async fn handle_opus_playback_stream<P: PeerReadHalf>(
    term: Arc<AtomicBool>,
    stream: &mut PlaybackStream,
    config: Config,
    mut peer: P,
) -> Result<()> {
    let channels = config.channels as usize;
    let mut decoder =
        opus::Decoder::new(config.sample_rate, opus::Channels::from_u8(config.channels))?;

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

pub fn handle_record<P: PeerWriteHalf + Send + 'static>(
    term: Arc<AtomicBool>,
    config: Config,
    device: Option<String>,
    name: String,
    root_logger: Logger,
    peer: P,
    opus_enabled: bool,
) -> Result<JoinHandle<()>> {
    let mut stream = StreamBuilder::new(config)
        .error_cb(error_cb, Some(root_logger.clone()))
        .start_record(&name, device.as_deref())?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device.to_owned())),
        None => root_logger.new(o!()),
    };
    info!(
        logger,
        "Record started, buffer samples: {}", config.max_buffer_frames
    );

    let handle = tokio::spawn(async move {
        let result = if opus_enabled {
            handle_opus_record_stream(term, &mut stream, config, peer).await
        } else {
            handle_raw_record_stream(term, &mut stream, config, peer).await
        };

        result
            .map_err(|err| error!(logger, "Record error: {}", err))
            .unwrap_or_default();

        if let Err(err) = stream.stop() {
            error!(logger, "Failed to stop record stream: {}", err);
        } else {
            info!(logger, "Record stopped");
        }
    });

    Ok(handle)
}

async fn handle_raw_record_stream<P: PeerWriteHalf>(
    term: Arc<AtomicBool>,
    stream: &mut RecordStream,
    config: Config,
    mut peer: P,
) -> Result<()> {
    let bufsize = config.buffer_size();
    let interval = config.buffer_duration();
    let mut base = [0u8; 65536];
    let buf = &mut base[..bufsize];
    while !term.load(Ordering::Relaxed) {
        while stream.peek() >= bufsize {
            let read = stream.read(buf);
            peer.write_all(&buf[..read]).await?;
        }
        sleep(interval).await;
    }
    Ok(())
}

async fn handle_opus_record_stream<P: PeerWriteHalf>(
    term: Arc<AtomicBool>,
    stream: &mut RecordStream,
    config: Config,
    mut peer: P,
) -> Result<()> {
    let bufsize = config.buffer_size();
    let interval = config.buffer_duration();
    let mut encoder = opus::Encoder::new(
        config.sample_rate,
        opus::Channels::from_u8(config.channels),
        opus::Application::Audio,
    )?;

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
