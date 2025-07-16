use std::{
    env,
    error::Error,
    net::SocketAddr,
    os::fd::FromRawFd,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use audiowire::{
    handlers::{check_audio, handle_playback, handle_record, handle_signal},
    logging,
    peer::PeerWriteHalf,
    Config, StreamFlags, StreamType, DEFAULT_CONFIG,
};
use slog::{error, info, o, Logger};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    time::timeout,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const SYSTEMD_SOCKET_FD: i32 = 3;

#[tokio::main]
async fn main() -> ExitCode {
    let logger = logging::term_logger();
    if let Err(e) = audiowire::initialize() {
        error!(logger, "Failed to initialize audio: {e}");
        return ExitCode::FAILURE;
    }
    run(&logger).await;
    if let Err(e) = audiowire::terminate() {
        error!(logger, "Failed to terminate audio: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn run(logger: &Logger) {
    let config = DEFAULT_CONFIG;
    let mut args = env::args();
    let output = args.nth(1);
    let input = args.next();

    if let Err(e) = check_audio(logger, config.clone(), input.as_deref(), output.as_deref()) {
        error!(logger, "Check audio error: {e}");
        return;
    }

    listen_tcp(logger, config, input, output)
        .await
        .map_err(|e| error!(logger, "Listener error: {e}"))
        .unwrap_or_default();
}

async fn listen_tcp(
    root_logger: &Logger,
    config: Config,
    input_name: Option<String>,
    output_name: Option<String>,
) -> Result<()> {
    let use_sd_socket = env::var("SYSTEMD_SOCKET")
        .map(|s| s == "true")
        .unwrap_or(false);
    let server_type = StreamType::new(
        input_name.as_ref().map(|s| s != "null").unwrap_or(true),
        output_name.as_ref().map(|s| s != "null").unwrap_or(true),
    );

    info!(root_logger, "Starting server");
    let listener = if use_sd_socket {
        info!(root_logger, "Using systemd activation socket");
        let std_listener = unsafe { std::net::TcpListener::from_raw_fd(SYSTEMD_SOCKET_FD) };
        TcpListener::from_std(std_listener)?
    } else {
        let addr = env::var("LISTENER_ADDR").unwrap_or("0.0.0.0:8760".to_owned());
        TcpListener::bind(addr).await?
    };
    info!(
        root_logger,
        "Server listening at {}",
        listener.local_addr()?
    );

    let term = handle_signal()?;
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
        handle_client(
            config,
            &client_logger,
            input_name.clone(),
            output_name.clone(),
            server_type,
            &term,
            socket,
            addr,
        )
        .await
        .map_err(|e| error!(client_logger, "Client error: {}", e))
        .unwrap_or_default();
    }

    info!(root_logger, "Server terminated");
    Ok(())
}

async fn handle_client(
    config: Config,
    client_logger: &Logger,
    input_name: Option<String>,
    output_name: Option<String>,
    server_type: StreamType,
    term: &Arc<AtomicBool>,
    socket: TcpStream,
    addr: SocketAddr,
) -> Result<()> {
    let mut buf = [0u8; 16];
    let (mut input, mut output) = socket.into_split();
    output.write_all(server_type.to_bytes().as_slice()).await?;
    input.read_exact(&mut buf[..1]).await?;

    let flags = StreamFlags::from(buf.as_slice());
    let client_type = flags.stream_type();
    let opus_enabled = flags.opus_enabled();
    let stream_logger = client_logger.new(o!("opus" => opus_enabled));
    let mut handles = Vec::new();

    if server_type.is_sink() && client_type.is_source() {
        let logger = stream_logger.new(o!("stream" => "playback"));
        let handle = handle_playback(
            Arc::clone(term),
            config,
            output_name,
            addr.to_string(),
            logger.clone(),
            input,
            opus_enabled,
        )?;
        handles.push((handle, logger));
    }

    if server_type.is_source() && client_type.is_sink() {
        let logger = stream_logger.new(o!("stream" => "record"));
        let handle = handle_record(
            Arc::clone(term),
            config,
            input_name,
            addr.to_string(),
            logger.clone(),
            output,
            opus_enabled,
        )?;
        handles.push((handle, logger));
    }

    let logger = client_logger.clone();
    tokio::spawn(async move {
        for (handle, logger) in handles {
            handle
                .await
                .map_err(|e| error!(logger, "Join error: {e}"))
                .unwrap_or_default();
        }
        info!(logger, "Client disconnected");
    });

    Ok(())
}
