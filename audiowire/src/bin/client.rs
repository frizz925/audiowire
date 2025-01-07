use std::{env, error::Error, time::Duration};

use audiowire::{
    handlers::{handle_playback, handle_record},
    logging, Config, DEFAULT_CONFIG,
};
use slog::{error, info, Logger};
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

    let mut last_err: Option<Box<dyn Error>> = None;
    let mut running = true;
    let mut retry = 0;
    loop {
        let result = run(
            &addr,
            &config,
            &logger,
            input_name.clone(),
            output_name.clone(),
        );
        let duration = match result.await {
            Ok(_) => {
                running = false;
                Duration::ZERO
            }
            Err(err) => {
                error!(logger, "Error: {}", err);
                last_err = Some(err);
                RETRY_DURATION.mul_f32((retry + 1) as f32)
            }
        };
        if !running || retry >= MAX_RETRY {
            break;
        }
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

    audiowire::terminate()?;

    last_err.map(|e| Err(e)).unwrap_or(Ok(()))
}

async fn run(
    addr: &str,
    config: &Config,
    root_logger: &Logger,
    input_name: Option<String>,
    output_name: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let socket = TcpStream::connect(addr).await?;
    info!(root_logger, "Connected to server: {}", socket.peer_addr()?);
    let (input, output) = socket.into_split();

    let record_handle = {
        let logger = root_logger.clone();
        let config_clone = config.clone();
        tokio::spawn(async move {
            handle_record(config_clone, input_name, &logger, output)
                .await
                .map_err(|err| error!(logger, "Record error: {}", err))
                .unwrap_or_default()
        })
    };

    let playback_handle = {
        let logger = root_logger.clone();
        let config_clone = config.clone();
        tokio::spawn(async move {
            handle_playback(config_clone, output_name, &logger, input)
                .await
                .map_err(|err| error!(logger, "Playback error: {}", err))
                .unwrap_or_default()
        })
    };

    record_handle.await?;
    playback_handle.await?;
    Ok(())
}
