use std::{
    io::Read,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, atomic::Ordering},
    time::{Instant, SystemTime},
};

use anyhow::Result;
use audiowire_serde::Serialize;
use slog::{Logger, debug, info, o, warn};

use crate::{
    backend::config::Config,
    command::DeviceConfig,
    logging,
    packet::{
        command::client::{ClientClose, ClientCommand, ClientHeartbeat},
        data::{IncomingClientData, OutgoingAudioData, OutgoingServerData},
        handshake::{Handshake, HandshakeAck, HandshakeInit, HandshakeReply},
        message::{IncomingMessage, OutgoingMessage},
        socket::{SharedUdpWrapper, UdpWrapper, wrap_udp},
        stream::{AtomicStreamId, StreamFlags},
        time::NetworkTime,
    },
    server::{
        SharedClientMap,
        client::{Client, ClientHandshake, ClientRunning},
    },
    stream::{Peer, handle_playback, handle_record},
};

pub struct Context<'a> {
    pub log: &'a Logger,
    pub addr: &'a SocketAddr,
    pub rec_timestamp: SystemTime,
}

pub struct Server {
    config: Config,
    device: DeviceConfig,
    sock: SharedUdpWrapper<Arc<UdpSocket>>,

    next_stream_id: AtomicStreamId,
    clients: SharedClientMap,
}

impl Server {
    pub fn new(
        config: Config,
        device: DeviceConfig,
        sock: Arc<UdpSocket>,
        clients: SharedClientMap,
    ) -> Self {
        Self {
            config,
            device,
            sock: wrap_udp(sock),

            next_stream_id: AtomicStreamId::new(1),
            clients,
        }
    }

    pub fn clients(&self) -> &SharedClientMap {
        &self.clients
    }

    pub fn handle_packet<'a, R: Read>(&mut self, context: Context<'a>, reader: R) -> Result<()> {
        let message = IncomingMessage::deserialize(reader)?;
        match message {
            IncomingMessage::Handshake(hs) => {
                self.handle_handshake(context, hs)?;
            }
            IncomingMessage::ClientCommand(cmd) => {
                self.handle_command(context, cmd)?;
            }
            IncomingMessage::Data(reader) => {
                let data = IncomingClientData::deserialize(reader)?;

                self.handle_data(context, data)?;
            }
            _ => (),
        }
        Ok(())
    }

    fn handle_handshake<'a>(&mut self, context: Context<'a>, hs: Handshake) -> Result<()> {
        match hs {
            Handshake::Init(init) => {
                self.handle_handshake_init(context, init)?;
            }
            Handshake::Ack(ack) => {
                self.handle_handshake_ack(context, ack)?;
            }
            _ => (),
        }
        Ok(())
    }

    fn handle_command<'a>(&self, context: Context<'a>, cmd: ClientCommand) -> Result<()> {
        let Context { log, addr, .. } = context;
        match cmd {
            ClientCommand::Heartbeat(ClientHeartbeat(stream_id)) => {
                if let Some(Client::Running(c)) = self.clients.write().unwrap().get_mut(&stream_id)
                {
                    let log = log.new(o!("stream_id" => stream_id));
                    debug!(log, "Received client heartbeat");
                    c.maybe_update_addr(&log, addr);
                    c.last_heartbeat = Instant::now();
                }
            }
            ClientCommand::Close(ClientClose(stream_id)) => {
                if let Some(_) = self.clients.write().unwrap().remove(&stream_id) {
                    info!(log, "Client closed"; "stream_id" => stream_id);
                }
            }
            _ => (),
        }
        Ok(())
    }

    fn handle_data<'a, R: Read>(
        &self,
        context: Context<'a>,
        data: IncomingClientData<R>,
    ) -> std::io::Result<()> {
        let Context { log, .. } = context;
        let IncomingClientData(stream_id, reader) = data;
        let log = log.new(o!("stream_id" => stream_id));
        if let Some(client) = self.clients.write().unwrap().get_mut(&stream_id) {
            if let Client::Running(c) = client {
                c.handle_data(&log, reader)?;
            } else {
                warn!(log, "Received data packet for client that is not running");
            }
        }
        Ok(())
    }

    fn handle_handshake_init<'a>(
        &mut self,
        context: Context<'a>,
        init: HandshakeInit,
    ) -> Result<()> {
        let DeviceConfig {
            source_enabled,
            sink_enabled,
            opus_enabled,
            ..
        } = self.device.to_owned();
        let Context {
            log,
            addr,
            rec_timestamp,
        } = context;
        let HandshakeInit { flags } = init;
        info!(log, "Got handshake init"; "stream_flags" => flags);

        let stream_id = self.next_stream_id.fetch_add(1, Ordering::Acquire);
        let xmt_timestamp = {
            let org_timestamp = SystemTime::now();
            let client = ClientHandshake {
                flags,
                org_timestamp,
                last_handshake: Instant::now(),
            }
            .into();
            self.clients.write().unwrap().insert(stream_id, client);
            org_timestamp
        };

        let reply: Handshake = HandshakeReply {
            stream_id,
            flags: StreamFlags {
                source_enabled: source_enabled && flags.sink_enabled,
                sink_enabled: sink_enabled && flags.source_enabled,
                opus_enabled: opus_enabled && flags.opus_enabled,
            },
            time: NetworkTime {
                rec_timestamp,
                xmt_timestamp,
            },
        }
        .into();
        self.sock.send_message_to(reply, addr)?;
        Ok(())
    }

    fn handle_handshake_ack<'a>(&self, context: Context<'a>, ack: HandshakeAck) -> Result<()> {
        let Context {
            log,
            addr,
            rec_timestamp,
        } = context;
        let HandshakeAck { stream_id, time } = ack;
        let log = log.new(o!("stream_id" => stream_id));
        info!(log,
            "Got handshake ack";
            "rec_timestamp" => logging::Timestamp(time.rec_timestamp),
            "xmt_timestamp" => logging::Timestamp(time.xmt_timestamp)
        );

        let client = self.clients.write().unwrap().remove(&stream_id);
        let hs = if let Some(client) = client {
            match client {
                Client::Handshake(hs) => hs,
                Client::Running(_) => {
                    warn!(
                        log,
                        "Received handshake ack while the stream is already running"
                    );
                    return Ok(());
                }
            }
        } else {
            warn!(log, "Client not found");
            return Ok(());
        };

        let ClientHandshake {
            flags,
            org_timestamp,
            ..
        } = hs;
        let opus_enabled = self.device.opus_enabled && flags.opus_enabled;

        let mut sequence = 0;
        let record = if self.device.source_enabled && flags.sink_enabled {
            let log = log.new(o!("stream" => "record"));
            let stream = handle_record(
                &log,
                &self.config,
                addr.to_string(),
                self.device.source_name.as_deref(),
                Arc::clone(self.sock.as_inner()),
                addr.to_owned(),
                move |src, dst| {
                    sequence += 1;
                    OutgoingMessage::from(OutgoingServerData(OutgoingAudioData {
                        sequence,
                        timestamp: SystemTime::now(),
                        data: src,
                    }))
                    .serialize(dst)
                },
                opus_enabled,
            )?;
            Some(stream)
        } else {
            None
        };

        let playback = if self.device.sink_enabled && flags.source_enabled {
            let log = log.new(o!("stream" => "playback"));
            let stream = handle_playback(
                &log,
                &self.config,
                addr.to_string(),
                self.device.sink_name.as_deref(),
                &time,
                org_timestamp,
                rec_timestamp,
                opus_enabled,
            )?;
            Some(stream)
        } else {
            None
        };

        let client = ClientRunning::new(Peer { record, playback }, addr.to_owned());
        self.clients
            .write()
            .unwrap()
            .insert(stream_id, client.into());

        Ok(())
    }
}
