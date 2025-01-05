use std::{env, error::Error};

use audiowire::{logging, Stream};
use slog::{debug, error, info, o, Logger};
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
                .map_err(|err| error!(logger, "Client erorr: {}", err))
                .unwrap_or_default()
        });
    }
}

async fn handle_client(
    name: Option<String>,
    root_logger: Logger,
    mut socket: TcpStream,
) -> Result<()> {
    let mut buf = [0u8; 65536];
    let name_str = name.as_ref().map(|s| s.as_str());
    let mut stream = audiowire::start_playback(name_str)?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device)),
        None => root_logger,
    };
    let mut running = true;
    while running {
        match socket.read(&mut buf).await {
            Ok(read) => {
                if read > 0 {
                    debug!(logger, "Read: {}", read);
                    stream.write(&buf[..read]);
                } else {
                    info!(logger, "Client closed connection");
                    running = false;
                }
            }
            Err(err) => {
                error!(logger, "Client read error: {}", err);
                running = false;
            }
        }
    }
    stream.stop()?;
    Ok(())
}
