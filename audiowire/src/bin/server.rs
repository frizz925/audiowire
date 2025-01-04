use std::{
    collections::HashMap,
    env,
    error::Error,
    ffi::c_void,
    io::Read,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{mpsc, Arc, Mutex},
    thread,
};

use audiowire::{logging, CallbackResult, Stream};
use slog::{error, info, o, Logger};

#[allow(dead_code)]
enum Signal {
    Stop(SocketAddr),
    Terminate,
}

struct Context {
    logger: Logger,
    socket: TcpStream,
    addr: SocketAddr,
    signal: mpsc::Sender<Signal>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let root_logger = logging::term_logger();
    let name = env::args().nth(1);

    audiowire::initialize()?;
    let streams_mut = Arc::new(Mutex::new(HashMap::new()));
    let (tx, rx) = mpsc::channel();

    let signal_logger = root_logger.new(o!("thread" => "signal"));
    let signal_streams = streams_mut.clone();
    thread::spawn(move || handle_signal(signal_logger, rx, signal_streams));

    let listener = TcpListener::bind("0.0.0.0:8760")?;
    info!(
        root_logger,
        "Server listening at {}",
        listener.local_addr()?
    );
    loop {
        let (socket, addr) = listener.accept()?;
        let logger = root_logger.new(o!("addr" => addr));
        let name_ref = name.as_ref().map(|s| s.as_str());
        info!(logger, "Client connected");
        match handle_client(name_ref, &logger, socket, addr.clone(), tx.clone()) {
            Ok(stream) => {
                let mut streams = streams_mut.lock().unwrap();
                streams.insert(addr, stream);
            }
            Err(err) => {
                error!(logger, "Client error: {}", err);
            }
        }
    }
}

fn handle_signal(
    logger: Logger,
    rx: mpsc::Receiver<Signal>,
    streams_mut: Arc<Mutex<HashMap<SocketAddr, impl Stream>>>,
) {
    let mut running = true;
    for signal in rx {
        match signal {
            Signal::Stop(addr) => {
                info!(logger, "Received stop signal for {}", addr);
                let mut streams = streams_mut.lock().unwrap();
                streams.remove(&addr);
            }
            Signal::Terminate => {
                running = false;
            }
        }
        if !running {
            break;
        }
    }
    info!(logger, "Signal thread exited")
}

fn handle_client(
    name: Option<&str>,
    logger: &Logger,
    socket: TcpStream,
    addr: SocketAddr,
    tx: mpsc::Sender<Signal>,
) -> Result<impl Stream, Box<dyn Error>> {
    let ctx = Context {
        logger: logger.new(o!("thread" => "playback")),
        socket,
        addr,
        signal: tx,
    };
    let stream = audiowire::start_playback(logger, name, handle_playback, ctx)?;
    match stream.device_name() {
        Some(device) => info!(logger, "Playback started, device: {}", device),
        None => info!(logger, "Playback started"),
    }
    Ok(stream)
}

fn handle_playback(buf: &mut [u8], userdata: *mut c_void) -> CallbackResult {
    let ctx = unsafe { &mut *(userdata as *mut Context) };
    if let Err(err) = ctx.socket.read_exact(buf) {
        error!(ctx.logger, "Client read error: {}", err);
        ctx.signal.send(Signal::Stop(ctx.addr)).unwrap();
        CallbackResult::Abort
    } else {
        CallbackResult::Continue
    }
}
