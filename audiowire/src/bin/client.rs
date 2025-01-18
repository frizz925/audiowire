use std::{
    env,
    error::Error,
    fmt::Display,
    future::Future,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use audiowire::{
    handlers::{handle_playback, handle_record, handle_signal},
    logging,
    peer::{PeerReadHalf, PeerWriteHalf},
    Config, StreamFlags, StreamType, DEFAULT_CONFIG,
};
use slog::{error, info, o, Logger};
use tokio::{net::TcpStream, time::sleep};

const MAX_RETRY: u8 = 5;
const RETRY_DURATION: Duration = Duration::from_secs(3);

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args = env::args();
    if let Some(addr) = args.nth(1) {
        init(addr, args).await.map_err(|e| e.to_string())
    } else {
        Err("Address argument is required".to_string())
    }
}

async fn init(addr: String, mut args: env::Args) -> Result<(), Box<dyn Error>> {
    let config = DEFAULT_CONFIG;
    let input_name = args.next();
    let output_name = args.next();
    let opus_disabled = env::var("OPUS_DISABLED")
        .map(|s| s == "1")
        .unwrap_or_default();
    let logger = logging::term_logger();

    if opus_disabled {
        info!(logger, "Opus codec is disabled");
    }

    audiowire::initialize()?;

    let result = run(
        &addr,
        config,
        &logger,
        input_name,
        output_name,
        opus_disabled,
    )
    .await;

    audiowire::terminate()?;
    result
}

async fn run(
    addr: &str,
    config: Config,
    root_logger: &Logger,
    input_name: Option<String>,
    output_name: Option<String>,
    opus_disabled: bool,
) -> Result<(), Box<dyn Error>> {
    let mut buf = [0u8; 16];
    let client_type = StreamType::new(
        input_name.as_ref().map(|s| s != "null").unwrap_or(true),
        output_name.as_ref().map(|s| s != "null").unwrap_or(true),
    );
    let stream_flags = StreamFlags::new(client_type, !opus_disabled);

    info!(root_logger, "Connecting to server: {}", addr);
    let socket = with_retry(&root_logger, || TcpStream::connect(addr)).await?;
    info!(root_logger, "Connected to server: {}", socket.peer_addr()?);

    let (mut input, mut output) = socket.into_split();
    output.write_all(&stream_flags.to_bytes()).await?;
    input.read_exact(&mut buf[..1]).await?;
    let server_type = StreamType::from(buf.as_slice());

    let mut handles = Vec::new();
    let term = handle_signal()?;

    if client_type.is_source() && server_type.is_sink() {
        let handle = handle_record(
            Arc::clone(&term),
            config.clone(),
            input_name,
            addr.to_owned(),
            root_logger.new(o!("stream" => "record")),
            output,
            !opus_disabled,
        )?;
        handles.push(handle);
    }

    if client_type.is_sink() && server_type.is_source() {
        let handle = handle_playback(
            Arc::clone(&term),
            config.clone(),
            output_name,
            addr.to_owned(),
            root_logger.new(o!("stream" => "playback")),
            input,
            !opus_disabled,
        )?;
        handles.push(handle);
    }

    for handle in handles {
        handle.await?;
        term.store(true, Ordering::Relaxed);
    }

    info!(root_logger, "Connection terminated");
    Ok(())
}

async fn with_retry<T, E, F, G>(logger: &Logger, g: G) -> F::Output
where
    E: Display,
    F: Future<Output = Result<T, E>>,
    G: Fn() -> F,
{
    let mut retry = 0;
    loop {
        let err = match g().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                error!(logger, "{}", e);
                e
            }
        };
        if retry >= MAX_RETRY {
            return Err(err);
        }
        let duration = RETRY_DURATION.mul_f32((retry + 1) as f32);
        info!(
            logger,
            "Retrying in {} second(s) ({} retries left)",
            duration.as_secs(),
            MAX_RETRY - retry
        );
        if !duration.is_zero() {
            sleep(duration).await;
        }
        retry += 1;
    }
}
