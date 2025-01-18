use std::{
    env,
    error::Error,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use audiowire::{
    handlers::{handle_playback, handle_record, handle_signal},
    logging,
    peer::PeerWriteHalf,
    Config, StreamFlags, StreamType, DEFAULT_CONFIG,
};
use slog::{error, info, o, Logger};
use tokio::{io::AsyncReadExt, net::TcpListener, time::timeout};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    audiowire::initialize()?;
    let result = run().await;
    audiowire::terminate()?;
    result
}

async fn run() -> Result<()> {
    let config = DEFAULT_CONFIG;
    let mut args = env::args();
    let output = args.nth(1);
    let input = args.next();

    let logger = logging::term_logger();
    let term = handle_signal()?;
    listen_tcp(term, config, &logger, output, input)
        .await
        .map_err(|e| error!(logger, "Listener error: {}", e))
        .unwrap_or_default();

    info!(logger, "Server terminated");
    Ok(())
}

async fn listen_tcp(
    term: Arc<AtomicBool>,
    config: Config,
    root_logger: &Logger,
    output_name: Option<String>,
    input_name: Option<String>,
) -> Result<()> {
    let server_type = StreamType::new(
        input_name.as_ref().map(|s| s != "null").unwrap_or(true),
        output_name.as_ref().map(|s| s != "null").unwrap_or(true),
    );
    let mut buf = [0u8; 16];
    let listener = TcpListener::bind("0.0.0.0:8760").await?;
    info!(
        root_logger,
        "Server listening at {}",
        listener.local_addr()?
    );
    while !term.load(Ordering::Relaxed) {
        let listener_ref = &listener;
        let future = timeout(Duration::from_secs(1), async move {
            listener_ref.accept().await
        });
        let (socket, addr) = match future.await {
            Ok(v) => v?,
            Err(_) => continue,
        };
        let client_logger = root_logger.new(o!("addr" => addr));
        info!(client_logger, "Client connected");

        let (mut input, mut output) = socket.into_split();
        output.write_all(server_type.to_bytes().as_slice()).await?;
        input.read_exact(&mut buf[..1]).await?;

        let flags = StreamFlags::from(buf.as_slice());
        let client_type = flags.stream_type();
        let opus_enabled = flags.opus_enabled();
        let stream_logger = client_logger.new(o!("opus" => opus_enabled));

        if server_type.is_sink() && client_type.is_source() {
            handle_playback(
                Arc::clone(&term),
                config,
                output_name.clone(),
                addr.to_string(),
                stream_logger.new(o!("stream" => "playback")),
                input,
                opus_enabled,
            )?;
        }

        if server_type.is_source() && client_type.is_sink() {
            handle_record(
                Arc::clone(&term),
                config,
                input_name.clone(),
                addr.to_string(),
                stream_logger.new(o!("stream" => "record")),
                output,
                opus_enabled,
            )?;
        }
    }
    Ok(())
}
