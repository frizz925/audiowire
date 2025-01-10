use std::{
    collections::HashMap,
    env,
    error::Error,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use audiowire::{
    handlers::{handle_playback, handle_record, handle_signal},
    logging,
    peer::{UdpPeer, UdpPeerProducer},
    Config, StreamType, DEFAULT_CONFIG,
};
use slog::{error, info, o, Logger};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, UdpSocket},
    time::timeout,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const UDP_BACKLOG: usize = 64;

#[tokio::main]
async fn main() -> Result<()> {
    audiowire::initialize()?;
    let result = run().await;
    audiowire::terminate()?;
    result
}

async fn run() -> Result<()> {
    let config = DEFAULT_CONFIG;
    let root_logger = logging::term_logger();
    let mut args = env::args();
    let output_name = args.nth(1);
    let input_name = args.next();

    let main_term = handle_signal()?;

    let tcp_handle = {
        let term = Arc::clone(&main_term);
        let logger = root_logger.new(o!("proto" => "tcp"));
        let output = output_name.clone();
        let input = input_name.clone();
        tokio::spawn(async move {
            listen_tcp(term, config, &logger, output, input)
                .await
                .map_err(|e| error!(logger, "Listener error: {}", e))
                .unwrap_or_default()
        })
    };

    /*
    let udp_handle = {
        let logger = root_logger.new(o!("proto" => "udp"));
        let output = output_name.clone();
        let input = input_name.clone();
        tokio::spawn(async move {
            listen_udp(config, &logger, output, input)
                .await
                .map_err(|e| error!(logger, "Listener error: {}", e))
                .unwrap_or_default()
        })
    };
    udp_handle.await?;
    */

    tcp_handle.await?;
    Ok(())
}

async fn listen_tcp(
    main_term: Arc<AtomicBool>,
    config: Config,
    root_logger: &Logger,
    output_name: Option<String>,
    input_name: Option<String>,
) -> Result<()> {
    let mut buf = [0u8; 16];
    let listener = TcpListener::bind("0.0.0.0:8760").await?;
    info!(
        root_logger,
        "Server listening at {}",
        listener.local_addr()?
    );
    while !main_term.load(Ordering::Relaxed) {
        let listener_ref = &listener;
        let future = timeout(Duration::from_secs(1), async move {
            listener_ref.accept().await
        });
        let (socket, addr) = match future.await {
            Ok(v) => v?,
            Err(_) => continue,
        };
        let (mut input, output) = socket.into_split();
        let client_logger = root_logger.new(o!("addr" => addr));
        info!(client_logger, "Client connected");

        let stream_type_buf = &mut buf[..1];
        input.read_exact(stream_type_buf).await?;
        let stream_type = StreamType::try_from(stream_type_buf[0])?;

        if [StreamType::Duplex, StreamType::Sink].contains(&stream_type) {
            let term = Arc::clone(&main_term);
            let logger = client_logger.new(o!("stream" => "playback"));
            let name_clone = output_name.clone();
            tokio::spawn(async move {
                handle_playback(term, config, name_clone, &logger, input)
                    .await
                    .map_err(|err| error!(logger, "Client playback error: {}", err))
                    .unwrap_or_default()
            });
        }

        if [StreamType::Duplex, StreamType::Source].contains(&stream_type) {
            let term = Arc::clone(&main_term);
            let logger = client_logger.new(o!("stream" => "record"));
            let name_clone = input_name.clone();
            tokio::spawn(async move {
                handle_record(term, config, name_clone, &logger, output)
                    .await
                    .map_err(|err| error!(logger, "Client record error: {}", err))
                    .unwrap_or_default()
            });
        }
    }
    info!(root_logger, "Server terminated");
    Ok(())
}

// UDP listener is unavailable since using UdpSocket::send_to to a peer that has
// closed its socket is causing the entire server socket to be unusable.
#[allow(dead_code)]
async fn listen_udp(
    main_term: Arc<AtomicBool>,
    config: Config,
    root_logger: &Logger,
    output_name: Option<String>,
    input_name: Option<String>,
) -> Result<()> {
    let mut buf = [0u8; 8192];
    let mut producers: HashMap<SocketAddr, UdpPeerProducer> = HashMap::new();
    let listener = Arc::new(UdpSocket::bind("0.0.0.0:8760").await?);
    info!(
        root_logger,
        "Server listening at {}",
        listener.local_addr()?
    );
    while !main_term.load(Ordering::Relaxed) {
        let (read, addr) = listener.recv_from(&mut buf).await?;
        if let Some(prod) = producers.get(&addr) {
            prod.send(&buf[..read]).await?;
            continue;
        }

        let peer = UdpPeer::new(listener.clone(), addr, UDP_BACKLOG);
        let (input, output, producer) = peer.into_split();
        let client_logger = root_logger.new(o!("addr" => addr));
        info!(client_logger, "Client connected");

        let stream_type_buf = &mut buf[..1];
        let stream_type = StreamType::try_from(stream_type_buf[0])?;

        if [StreamType::Duplex, StreamType::Source].contains(&stream_type) {
            let term = Arc::clone(&main_term);
            let logger = client_logger.new(o!("stream" => "playback"));
            let name_clone = output_name.clone();
            tokio::spawn(async move {
                handle_playback(term, config, name_clone, &logger, input)
                    .await
                    .map_err(|err| error!(logger, "Client playback error: {}", err))
                    .unwrap_or_default()
            });
        }

        if [StreamType::Duplex, StreamType::Sink].contains(&stream_type) {
            let term = Arc::clone(&main_term);
            let logger = client_logger.new(o!("stream" => "record"));
            let name_clone = input_name.clone();
            tokio::spawn(async move {
                handle_record(term, config, name_clone, &logger, output)
                    .await
                    .map_err(|err| error!(logger, "Client record error: {}", err))
                    .unwrap_or_default()
            });
        }

        producers.insert(addr, producer);
    }
    info!(root_logger, "Server terminated");
    Ok(())
}
