use std::{env, error::Error};

use audiowire::{
    handlers::{handle_playback, handle_record},
    logging, DEFAULT_CONFIG,
};
use slog::{error, info};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args = env::args();
    if let Some(addr) = args.nth(1) {
        run(addr, args).await.map_err(|err| err.to_string())
    } else {
        Err("Address argument is required".to_string())
    }
}

async fn run(addr: String, mut args: env::Args) -> Result<(), Box<dyn Error>> {
    let config = DEFAULT_CONFIG;
    let root_logger = logging::term_logger();
    let input_name = args.next();
    let output_name = args.next();

    audiowire::initialize()?;

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
    audiowire::terminate()?;

    Ok(())
}
