use std::{env, error::Error};

use audiowire::{
    handlers::{handle_playback, handle_record},
    logging, DEFAULT_CONFIG,
};
use slog::{error, info, o};
use tokio::net::TcpListener;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    let config = DEFAULT_CONFIG;
    let root_logger = logging::term_logger();
    let mut args = env::args();
    let output_name = args.nth(1);
    let input_name = args.next();

    audiowire::initialize()?;

    let listener = TcpListener::bind("0.0.0.0:8760").await?;
    info!(
        root_logger,
        "Server listening at {}",
        listener.local_addr()?
    );
    loop {
        let (socket, addr) = listener.accept().await?;
        let (input, output) = socket.into_split();
        let client_logger = root_logger.new(o!("addr" => addr));
        info!(client_logger, "Client connected");

        {
            let logger = client_logger.clone();
            let name_clone = output_name.clone();
            tokio::spawn(async move {
                handle_playback(config, name_clone, &logger, input)
                    .await
                    .map_err(|err| error!(logger, "Client playback error: {}", err))
                    .unwrap_or_default()
            });
        }

        {
            let logger = client_logger.clone();
            let name_clone = input_name.clone();
            tokio::spawn(async move {
                handle_record(config, name_clone, &logger, output)
                    .await
                    .map_err(|err| error!(logger, "Client record error: {}", err))
                    .unwrap_or_default()
            });
        }
    }
}
