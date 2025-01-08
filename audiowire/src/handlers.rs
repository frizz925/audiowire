use std::{error::Error, time::Duration};

use slog::{error, info, o, Logger};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    time::sleep,
};

use super::audiowire::{
    start_playback, start_record, Config, PlaybackStream, RecordStream, Stream,
};
use super::opus::ChannelsParser;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub async fn handle_playback(
    config: Config,
    name: Option<String>,
    root_logger: &Logger,
    socket: OwnedReadHalf,
) -> Result<()> {
    let name_str = name.as_ref().map(|s| s.as_str());
    let channels = config.channels;
    let decoder = opus::Decoder::new(
        config.sample_rate,
        opus::Channels::from_u8(config.channels)?,
    )?;

    let mut stream = start_playback(name_str, config)?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device.to_owned())),
        None => root_logger.new(o!()),
    };
    info!(
        logger,
        "Playback started, buffer capacity: {}",
        stream.capacity(),
    );

    handle_playback_stream(&mut stream, channels as usize, socket, decoder)
        .await
        .map_err(|err| error!(logger, "Peer stream error: {}", err))
        .unwrap_or_default();

    stream.stop()?;
    info!(logger, "Playback stopped");
    Ok(())
}

async fn handle_playback_stream(
    stream: &mut PlaybackStream,
    channels: usize,
    mut socket: OwnedReadHalf,
    mut decoder: opus::Decoder,
) -> Result<()> {
    let mut buf = [0i16; 65536];
    let mut tmp = [0u8; 8192];
    loop {
        let (head, tail) = tmp.split_at_mut(size_of::<u16>());
        socket.read_exact(head).await?;
        let length = u16::from_be_bytes(head.try_into().unwrap()) as usize;
        let data = &mut tail[..length];
        socket.read_exact(data).await?;
        let fcount = decoder.decode(data, &mut buf, false)?;

        let data = convert_slice(&buf, channels * fcount);
        if stream.peek() >= data.len() {
            stream.write(data);
        }
    }
}

pub async fn handle_record(
    config: Config,
    name: Option<String>,
    root_logger: &Logger,
    socket: OwnedWriteHalf,
) -> Result<()> {
    let name_str = name.as_ref().map(|s| s.as_str());
    let bufsize = config.frame_buffer_size();
    let interval = config.buffer_duration;
    let encoder = opus::Encoder::new(
        config.sample_rate,
        opus::Channels::from_u8(config.channels)?,
        opus::Application::Audio,
    )?;

    let mut stream = start_record(name_str, config)?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device.to_owned())),
        None => root_logger.new(o!()),
    };
    info!(
        logger,
        "Record started, buffer capacity: {}",
        stream.capacity()
    );

    handle_record_stream(&mut stream, bufsize, interval, socket, encoder)
        .await
        .map_err(|err| error!(logger, "Peer stream error: {}", err))
        .unwrap_or_default();

    stream.stop()?;
    info!(logger, "Record stopped");
    Ok(())
}

async fn handle_record_stream(
    stream: &mut RecordStream,
    bufsize: usize,
    interval: Duration,
    mut socket: OwnedWriteHalf,
    mut encoder: opus::Encoder,
) -> Result<()> {
    let mut tmp = [0u8; 65536];
    let mut buf = [0u8; 8192];
    loop {
        while stream.peek() >= bufsize {
            let (head, tail) = buf.split_at_mut(size_of::<u16>());
            let read = stream.read(&mut tmp[..bufsize]);
            let size = encoder.encode(convert_slice(&tmp, read), tail)?;
            let size_buf = (size as u16).to_be_bytes();
            let end = size_buf.len() + size;
            head.clone_from_slice(size_buf.as_slice());
            socket.write_all(&buf[..end]).await?;
        }
        sleep(interval).await;
    }
}

fn convert_slice<S: Sized, T: Sized>(buf: &[S], len: usize) -> &[T] {
    let src_size = std::mem::size_of::<S>();
    let dst_size = std::mem::size_of::<T>();
    unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const T, len * src_size / dst_size) }
}
