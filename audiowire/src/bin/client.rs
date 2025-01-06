use std::{env, error::Error, io::Write, net::TcpStream, thread::sleep, time::Duration};

use audiowire::{convert_slice, logging, opus::ChannelsParser, Stream, DEFAULT_CONFIG};
use slog::{info, o};

fn main() -> Result<(), String> {
    let mut args = env::args();
    if let Some(addr) = args.nth(1) {
        let name = args.next();
        run(addr, name).map_err(|err| err.to_string())
    } else {
        Err("Address argument is required".to_string())
    }
}

fn run(addr: String, name: Option<String>) -> Result<(), Box<dyn Error>> {
    let config = DEFAULT_CONFIG;
    let root_logger = logging::term_logger();

    audiowire::initialize()?;

    let mut socket = TcpStream::connect(addr)?;
    info!(root_logger, "Connected to server: {}", socket.peer_addr()?);

    let bufsize = config.frame_buffer_size();
    let mut encoder = opus::Encoder::new(
        config.sample_rate,
        opus::Channels::from_u8(config.channels)?,
        opus::Application::Audio,
    )?;

    let name_str = name.as_ref().map(|s| s.as_str());
    let mut stream = audiowire::start_record(name_str, config)?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device)),
        None => root_logger,
    };
    info!(logger, "Record started");

    let mut buf = [0u8; 65536];
    let mut tmp = [0u8; 8192];
    loop {
        sleep(Duration::from_millis(20));
        while stream.peek() >= bufsize {
            let read = stream.read(&mut buf[..bufsize]);
            let size = encoder.encode(convert_slice(&buf, read), &mut tmp)?;
            let size_buf = (size as u16).to_be_bytes();

            let mid = size_buf.len();
            let end = mid + size;
            let (head, tail) = buf[..end].split_at_mut(mid);
            head.clone_from_slice(size_buf.as_slice());
            tail.clone_from_slice(&tmp[..size]);

            socket.write_all(&buf[..end])?;
        }
    }
}
