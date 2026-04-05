use std::{
    collections::HashMap,
    io::ErrorKind,
    net::{IpAddr, SocketAddr, UdpSocket},
    process::ExitCode,
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};

use anyhow::{Ok as _Ok, Result};
use audiowire::{
    backend::{config::Config, util::audio_check},
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        command::server::SERVER_CLOSE,
        socket::{UdpWrapper, wrap_udp},
    },
    server::{
        Context, Server,
        client::{Client, ClientRunning},
        heartbeat::HeartbeatWorker,
        time_sync::TimeSyncWorker,
    },
};
use clap::{Arg, Command, value_parser};
use slog::{Logger, error, info, o, trace};

fn cmd() -> Command {
    let cmd = Command::new("audiowire-udp-server")
        .about("AudioWire server using UDP packets")
        .arg(
            Arg::new("host")
                .long("host")
                .value_parser(value_parser!(IpAddr))
                .default_value("::")
                .help("Host to be used by the server to listen for packets"),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_parser(value_parser!(u16).range(0..65536))
                .default_value("8760")
                .help("Port to be used by the server to listen for packets"),
        );
    add_device_args(cmd)
}

fn main() -> Result<ExitCode> {
    let matches = cmd().get_matches();
    let host = matches.get_one("host").map(IpAddr::to_owned).unwrap();
    let port = matches.get_one("port").map(u16::to_owned).unwrap();
    let addr = SocketAddr::new(host, port);

    let device = DeviceConfig::from(&matches);
    let config = Config::default();
    let log = logging::initialize();

    audiowire::initialize()?;
    info!(log, "Starting audio check");
    audio_check(&log, &config, &device)?;
    info!(log, "Audio check finished");

    let result = run(log, config, device, addr);

    audiowire::terminate()?;
    result
}

fn run(log: Logger, config: Config, device: DeviceConfig, addr: SocketAddr) -> Result<ExitCode> {
    let sock = Arc::new(UdpSocket::bind(addr)?);
    info!(log, "Server listening at {}", addr.to_string());
    sock.set_nonblocking(true)?;

    let notify = Arc::new((Mutex::new(()), Condvar::new(), AtomicBool::new(true)));

    // Signal handler
    {
        let notify = Arc::clone(&notify);
        ctrlc::set_handler(move || {
            let (lock, cvar, running) = &*notify;
            running.store(false, Ordering::Relaxed);
            let _locked = lock.lock().unwrap();
            cvar.notify_all();
        })
        .unwrap();
    }

    let clients = Arc::new(RwLock::new(HashMap::new()));
    let interval = config.buffer_duration() / 4;
    let mut server = Server::new(config, device, Arc::clone(&sock), Arc::clone(&clients));
    let mut handles = Vec::new();

    // Heartbeat handler
    let worker = HeartbeatWorker::new(
        log.new(o!("worker" => "heartbeat")),
        Arc::clone(&sock),
        Arc::clone(&clients),
        Arc::clone(&notify),
    );
    handles.push(thread::spawn(|| worker.run()));

    // Time sync handler
    let worker = TimeSyncWorker::new(
        log.new(o!("worker" => "time_sync")),
        Arc::clone(&sock),
        Arc::clone(&clients),
        Arc::clone(&notify),
    );
    handles.push(thread::spawn(|| worker.run()));

    let (_, _, running) = &*notify;
    let mut exit_code = ExitCode::SUCCESS;
    let mut sock = wrap_udp(sock);
    while running.load(Ordering::Acquire) {
        let (buf, addr) = match sock.raw_recv_from() {
            Ok(value) => value,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                thread::sleep(interval);
                continue;
            }
            Err(e) => {
                error!(log, "Socket recv_from returns error"; "error" => e);
                exit_code = ExitCode::FAILURE;
                break;
            }
        };
        trace!(log, "Received data {} bytes", buf.len(); "addr" => addr);
        let rec_timestamp = Instant::now();

        let log = log.new(o!("addr" => addr));
        let context = Context {
            log: &log,
            addr: &addr,
            rec_timestamp,
        };
        if let Err(e) = server.handle_packet(context, buf) {
            error!(log, "Failed to handle packet"; "error" => e.to_string());
        }
    }

    for client in server.clients().read().unwrap().values() {
        if let Client::Running(ClientRunning { addr, .. }) = client {
            sock.send_message_to(SERVER_CLOSE, addr)?;
        }
    }
    info!(log, "Server stopped listening");

    for handle in handles {
        handle.join().unwrap();
    }

    _Ok(exit_code)
}
