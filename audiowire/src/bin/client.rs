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
    peer::PeerWriteHalf,
    Config, StreamType, DEFAULT_CONFIG,
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
    let logger = logging::term_logger();

    audiowire::initialize()?;
    // TODO: Run audio device check before connecting to server
    let result = run(&addr, config, logger, input_name, output_name).await;
    audiowire::terminate()?;
    result
}

async fn run(
    addr: &str,
    config: Config,
    root_logger: Logger,
    input_name: Option<String>,
    output_name: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let stream_type = if input_name.as_ref().map_or(false, |s| s == "null") {
        StreamType::Sink
    } else if output_name.as_ref().map_or(false, |s| s == "null") {
        StreamType::Source
    } else {
        StreamType::Duplex
    };

    let socket = with_retry(&root_logger, || TcpStream::connect(addr)).await?;

    info!(root_logger, "Connected to server: {}", socket.peer_addr()?);
    let (input, mut output) = socket.into_split();

    let stream_type_buf: [u8; 1] = [stream_type.into()];
    output.write_all(&stream_type_buf).await?;

    let mut handles = Vec::new();
    let main_term = handle_signal()?;

    if [StreamType::Duplex, StreamType::Source].contains(&stream_type) {
        let term = Arc::clone(&main_term);
        let logger = root_logger.new(o!("stream" => "record"));
        let config_clone = config.clone();
        let handle = tokio::spawn(async move {
            handle_record(term, config_clone, input_name, &logger, output)
                .await
                .map_err(|err| error!(logger, "Record error: {}", err))
                .unwrap_or_default()
        });
        handles.push(handle);
    }

    if [StreamType::Duplex, StreamType::Sink].contains(&stream_type) {
        let term = Arc::clone(&main_term);
        let logger = root_logger.new(o!("stream" => "playback"));
        let config_clone = config.clone();
        let handle = tokio::spawn(async move {
            handle_playback(term, config_clone, output_name, &logger, input)
                .await
                .map_err(|err| error!(logger, "Playback error: {}", err))
                .unwrap_or_default()
        });
        handles.push(handle);
    }

    while !main_term.load(Ordering::Relaxed) {
        // Do nothing lmao
        sleep(Duration::from_secs(1)).await;
    }

    for handle in handles {
        handle.await?;
    }
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
