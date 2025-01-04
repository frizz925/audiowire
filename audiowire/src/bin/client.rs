use std::{
    env,
    error::Error,
    ffi::c_void,
    io::{Read, Write},
    net::TcpStream,
};

use audiowire::{logging, CallbackResult, Stream};
use slog::{error, info, o, Logger};

struct Context {
    logger: Logger,
    socket: TcpStream,
}

fn main() -> Result<(), String> {
    let mut args = env::args();
    if let Some(addr) = args.nth(1) {
        let name = args.next();
        let name_ref = name.as_ref().map(|s| s.as_str());
        run(addr, name_ref).map_err(|err| err.to_string())
    } else {
        Err("Address argument is required".to_string())
    }
}

fn run(addr: String, name: Option<&str>) -> Result<(), Box<dyn Error>> {
    let logger = logging::term_logger();

    audiowire::initialize()?;

    let mut buf = [0u8; 16];
    let mut socket = TcpStream::connect(addr)?;
    info!(logger, "Connected to server: {}", socket.peer_addr()?);

    let context = Context {
        logger: logger.new(o!()),
        socket: socket.try_clone()?,
    };
    let mut stream = audiowire::start_record(&logger, name, handle_record, context)?;
    match stream.device_name() {
        Some(device) => info!(logger, "Record started, device: {}", device),
        None => info!(logger, "Record started"),
    }

    socket.read(&mut buf)?;
    stream.stop()?;
    info!(logger, "Record stopped");

    audiowire::terminate()?;

    Ok(())
}

fn handle_record(buf: &[u8], userdata: *mut c_void) -> CallbackResult {
    let ctx = unsafe { &mut *(userdata as *mut Context) };
    if let Err(err) = ctx.socket.write_all(buf) {
        error!(ctx.logger, "Write error: {}", err);
        CallbackResult::Abort
    } else {
        CallbackResult::Continue
    }
}
