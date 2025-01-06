use std::{env, error::Error};

use audiowire::{
    convert_slice, logging, opus::ChannelsParser, PlaybackStream, Stream, DEFAULT_CONFIG,
};
use slog::{error, info, o, Logger};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    let root_logger = logging::term_logger();
    let name = env::args().nth(1);

    audiowire::initialize()?;

    let listener = TcpListener::bind("0.0.0.0:8760").await?;
    info!(
        root_logger,
        "Server listening at {}",
        listener.local_addr()?
    );
    loop {
        let (socket, addr) = listener.accept().await?;
        let logger = root_logger.new(o!("addr" => addr));
        info!(logger, "Client connected");
        let name_clone = name.clone();
        tokio::spawn(async move {
            handle_client(name_clone, logger.clone(), socket)
                .await
                .map_err(|err| error!(logger, "Client error: {}", err))
                .unwrap_or_default()
        });
    }
}

async fn handle_client(name: Option<String>, root_logger: Logger, socket: TcpStream) -> Result<()> {
    let config = DEFAULT_CONFIG;
    let name_str = name.as_ref().map(|s| s.as_str());

    let channels = config.channels;
    let decoder = opus::Decoder::new(
        config.sample_rate,
        opus::Channels::from_u8(config.channels)?,
    )?;
    let mut stream = audiowire::start_playback(name_str, config)?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device)),
        None => root_logger,
    };
    info!(logger, "Playback started");

    handle_stream(&mut stream, channels as usize, socket, decoder)
        .await
        .map_err(|err| error!(logger, "Client stream error: {}", err))
        .unwrap_or_default();

    stream.stop()?;
    info!(logger, "Playback stopped");
    Ok(())
}

async fn handle_stream(
    stream: &mut PlaybackStream,
    channels: usize,
    mut socket: TcpStream,
    mut decoder: opus::Decoder,
) -> Result<()> {
    let mut buf = [0i16; 65536];
    let mut tmp = [0u8; 8192];
    let (head, tail) = tmp.split_at_mut(2);
    loop {
        socket.read_exact(head).await?;
        let length = u16::from_be_bytes(head.try_into()?) as usize;
        socket.read_exact(&mut tail[..length]).await?;
        let fcount = decoder.decode(&tail[..length], &mut buf, false)?;
        stream.write(convert_slice(&buf, channels * fcount));
    }
}
