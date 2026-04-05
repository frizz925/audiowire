use std::{
    io::ErrorKind,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    process::ExitCode,
    sync::{
        Arc, Condvar, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};

use anyhow::Result;
use audiowire::{
    backend::{config::Config, util::audio_check},
    client::{
        Client,
        handshake::{HandshakeResult, start_handshake},
        heartbeat::HeartbeatWorker,
        time_sync::TimeSyncWorker,
    },
    command::{DeviceConfig, add_device_args},
    logging,
    packet::{
        command::client::{ClientClose, ClientCommand},
        data::{OutgoingAudioData, OutgoingClientData},
        message::OutgoingMessage,
        socket::{UdpWrapper, wrap_udp, wrap_udp_owned},
    },
    stream::{Peer, handle_playback, handle_record},
};
use audiowire_serde::Serialize;
use clap::{Arg, Command};
use slog::{Logger, debug, error, info, o};

fn cmd() -> Command {
    let cmd = Command::new("audiowire-udp-client")
        .about("AudioWire server using UDP packets")
        .arg_required_else_help(true)
        .arg(
            Arg::new("addr")
                .help("Address of the AudioWire server (eg. localhost:8760)")
                .required(true),
        );
    add_device_args(cmd)
}

fn main() -> Result<ExitCode> {
    let matches = cmd().get_matches();
    let addr = matches.get_one("addr").map(String::to_owned).unwrap();
    let saddr = addr
        .to_socket_addrs()?
        .next()
        .expect("Unable to resolve server address");

    let device = DeviceConfig::from(&matches);
    let config = Config::default();
    let log = logging::initialize();

    audiowire::initialize()?;
    info!(log, "Starting audio check");
    audio_check(&log, &config, &device)?;
    info!(log, "Audio check finished");

    let result = run(log, config, device, addr, saddr);
    audiowire::terminate()?;

    result
}

fn run(
    log: Logger,
    config: Config,
    device: DeviceConfig,
    name: String,
    addr: SocketAddr,
) -> Result<ExitCode> {
    let mut sock = wrap_udp_owned(UdpSocket::bind("0.0.0.0:0")?);
    info!(log, "Initiating handshake with server"; "addr" => addr);

    let HandshakeResult {
        org_timestamp,
        rec_timestamp,
        stream_id,
        flags,
        time,
    } = start_handshake(&log, &device, &mut sock, &addr)?;
    let DeviceConfig {
        source_name,
        sink_name,
        opus_enabled,
        ..
    } = device;
    let opus_enabled = opus_enabled && flags.opus_enabled;

    let rtt = time.calculate_rtt(org_timestamp, rec_timestamp);
    let remote_epoch = time.calculate_remote_epoch(org_timestamp, rtt);

    let sock = Arc::new(sock.into_inner());
    sock.set_nonblocking(true)?;

    let mut sequence = 0;
    let record = if device.source_enabled && flags.sink_enabled {
        let log = log.new(o!("stream" => "record"));
        let stream = handle_record(
            &log,
            &config,
            name.as_str(),
            source_name,
            Arc::clone(&sock),
            addr,
            move |src, dst| {
                sequence += 1;
                OutgoingMessage::from(OutgoingClientData(
                    stream_id,
                    OutgoingAudioData {
                        sequence,
                        timestamp: Instant::now().duration_since(org_timestamp),
                        data: src,
                    },
                ))
                .serialize(dst)
            },
            opus_enabled,
        )?;
        Some(stream)
    } else {
        None
    };

    let playback = if device.sink_enabled && flags.source_enabled {
        let log = log.new(o!("stream" => "playback"));
        let stream = handle_playback(
            &log,
            &config,
            name.as_str(),
            sink_name,
            opus_enabled,
            remote_epoch,
            rtt,
        )?;
        Some(stream)
    } else {
        None
    };

    let notify = Arc::new((Mutex::new(()), Condvar::new(), AtomicBool::new(true)));
    let last_heartbeat = Arc::new(RwLock::new(Instant::now()));
    let mut client = {
        let notify = Arc::clone(&notify);
        Client::new(
            Peer { record, playback },
            log.clone(),
            Arc::clone(&last_heartbeat),
            move || foreground_stop(&notify),
        )
    };
    let mut handles = Vec::new();

    // Cancel handler
    {
        let notify = Arc::clone(&notify);
        ctrlc::set_handler(move || foreground_stop(&notify)).unwrap();
    }

    // Heartbeat monitor
    handles.push({
        let worker = HeartbeatWorker::new(
            log.new(o!("worker" => "heartbeat")),
            stream_id,
            Arc::clone(&sock),
            addr,
            Arc::clone(&notify),
            last_heartbeat,
        );
        thread::spawn(|| worker.run())
    });

    // Time syncer
    handles.push({
        let worker = TimeSyncWorker::new(
            log.new(o!("worker" => "time_sync")),
            stream_id,
            Arc::clone(&sock),
            addr,
            Arc::clone(&notify),
            org_timestamp,
        );
        thread::spawn(|| worker.run())
    });

    let (_, _, running) = &*notify;
    let interval = config.buffer_duration() / 4;
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
        debug!(log, "Received data {} bytes", buf.len(); "addr" => addr);
        if let Err(e) = client.handle_packet(buf) {
            error!(log, "Failed to handle packet"; "addr" => addr, "error" => e);
        }
    }
    let cmd: ClientCommand = ClientClose(stream_id).into();
    sock.send_message_to(cmd, &addr)?;

    for handle in handles {
        handle.join().unwrap();
    }

    Ok(exit_code)
}

fn foreground_stop(notify: &Arc<(Mutex<()>, Condvar, AtomicBool)>) {
    let (lock, cvar, running) = &**notify;
    running.store(false, Ordering::Release);
    let mut _locked = lock.lock().unwrap();
    cvar.notify_all();
}
