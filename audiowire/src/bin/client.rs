use std::{env, error::Error, io::Write, net::TcpStream, thread::sleep, time::Duration};

use audiowire::{logging, Stream};
use slog::{debug, info, o};

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
    let root_logger = logging::term_logger();

    audiowire::initialize()?;

    let mut socket = TcpStream::connect(addr)?;
    info!(root_logger, "Connected to server: {}", socket.peer_addr()?);

    let name_str = name.as_ref().map(|s| s.as_str());
    let mut stream = audiowire::start_record(name_str)?;
    let logger = match stream.device_name() {
        Some(device) => root_logger.new(o!("device" => device)),
        None => root_logger,
    };
    info!(logger, "Record started");

    let mut buf = [0u8; 65536];
    loop {
        sleep(Duration::from_millis(20));
        let read = stream.read(&mut buf);
        if read > 0 {
            debug!(logger, "Read: {}", read);
            socket.write_all(&buf[..read])?;
        }
    }
}
