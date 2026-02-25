use std::sync::{Arc, Condvar, Mutex, mpsc};

use bytes::{Buf, Bytes, BytesMut};
use slog::{Logger, info};

use crate::{
    OPUS_APPLICATION,
    backend::{
        config::{Config, SampleFormat},
        error::Error,
        result::Result,
        stream::{ErrorFn, ReadFn, StreamBuilder, WriteFn},
    },
    command::DeviceConfig,
    opus::{convert_slice, convert_slice_mut},
};

type Notify = Arc<(Mutex<bool>, Condvar)>;

pub fn audio_check(log: &Logger, config: &Config, device: &DeviceConfig) -> Result<()> {
    let DeviceConfig {
        source_name,
        sink_name,
        source_enabled,
        sink_enabled,
        opus_enabled,
    } = device.clone();
    let notify = Arc::new((Mutex::new(false), Condvar::new()));
    let (error_tx, error_rx) = mpsc::channel();
    let (data_tx, data_rx) = mpsc::sync_channel(1);

    let encoder = if opus_enabled {
        Some(
            opus::Encoder::new(config.sample_rate, config.opus_channels(), OPUS_APPLICATION)
                .unwrap(),
        )
    } else {
        None
    };
    let decoder = if opus_enabled {
        Some(opus::Decoder::new(config.sample_rate, config.opus_channels()).unwrap())
    } else {
        None
    };
    let opus_message = if opus_enabled { "enabled" } else { "disabled" };

    let _source = if source_enabled {
        let stream = StreamBuilder::new(config.clone())
            .read_cb(on_read(config, data_tx, encoder, notify.clone()))
            .error_cb(on_error(error_tx.clone(), notify.clone()))
            .start("AudioWire test record", source_name)?;
        info!(
            log, "Started record device";
            "device" => stream.device_name(),
            "sample_rate" => stream.sample_rate(),
            "opus" => opus_message
        );
        Some(stream)
    } else {
        let src = BytesMut::zeroed(config.buffer_size());
        let buf = if let Some(mut enc) = encoder {
            opus_encode(config, &mut enc, &src, &mut [0u8; 65536])
        } else {
            src.freeze()
        };
        data_tx.send(buf).unwrap();
        condvar_notify(&notify);
        None
    };

    let _sink = if sink_enabled {
        let stream = StreamBuilder::new(config.clone())
            .write_cb(on_write(config, data_rx, decoder, notify.clone()))
            .error_cb(on_error(error_tx.clone(), notify.clone()))
            .start("AudioWire test playback", sink_name)?;
        info!(
            log, "Started playback device";
            "device" => stream.device_name(),
            "sample_rate" => stream.sample_rate(),
            "opus" => opus_message
        );
        Some(stream)
    } else {
        let src = data_rx.recv().unwrap();
        if let Some(mut dec) = decoder {
            let mut buf = [0u8; 65536];
            opus_decode(config, &mut dec, &src, &mut buf);
        }
        condvar_notify(&notify);
        None
    };

    let (lock, condvar) = &*notify;
    let mut done = lock.lock().unwrap();
    while !*done {
        done = condvar.wait(done).unwrap();
    }

    if let Ok(err) = error_rx.try_recv() {
        Err(err)
    } else {
        Ok(())
    }
}

fn on_read(
    cfg: &Config,
    tx: mpsc::SyncSender<Bytes>,
    mut encoder: Option<opus::Encoder>,
    notify: Notify,
) -> impl ReadFn {
    let cfg = cfg.to_owned();
    let mut buf = [0u8; 65536];
    move |src| {
        let buf = if let Some(enc) = &mut encoder {
            opus_encode(&cfg, enc, src, &mut buf)
        } else {
            Bytes::copy_from_slice(src)
        };
        tx.send(buf).ok();
        condvar_notify(&notify);
    }
}

fn on_write(
    cfg: &Config,
    rx: mpsc::Receiver<Bytes>,
    mut decoder: Option<opus::Decoder>,
    notify: Notify,
) -> impl WriteFn {
    let cfg = cfg.to_owned();
    move |dst| {
        let mut src = if let Ok(buf) = rx.recv() {
            buf
        } else {
            dst.fill(0);
            condvar_notify(&notify);
            return;
        };

        let len = if let Some(dec) = &mut decoder {
            opus_decode(&cfg, dec, &src, dst)
        } else {
            let len = usize::min(src.len(), dst.len());
            src.copy_to_slice(&mut dst[..len]);
            len
        };

        if len < dst.len() {
            dst[len..].fill(0);
        }
        condvar_notify(&notify);
    }
}

fn on_error(tx: mpsc::Sender<Error>, notify: Notify) -> impl ErrorFn {
    move |err| {
        tx.send(err).ok();
        condvar_notify(&notify);
    }
}

fn opus_encode(cfg: &Config, enc: &mut opus::Encoder, src: &[u8], buf: &mut [u8]) -> Bytes {
    let len = match cfg.sample_format {
        SampleFormat::S16 => enc.encode(convert_slice(src), buf),
        SampleFormat::F32 => enc.encode_float(convert_slice(src), buf),
    }
    .unwrap();
    Bytes::copy_from_slice(&buf[..len])
}

fn opus_decode(cfg: &Config, dec: &mut opus::Decoder, src: &[u8], buf: &mut [u8]) -> usize {
    let cnt = match cfg.sample_format {
        SampleFormat::S16 => dec.decode(&src, convert_slice_mut(buf), false),
        SampleFormat::F32 => dec.decode_float(&src, convert_slice_mut(buf), false),
    }
    .unwrap();
    cfg.frame_count_to_bytes(cnt)
}

fn condvar_notify(notify: &Notify) {
    let (lock, condvar) = &**notify;
    let mut done = lock.lock().unwrap();
    *done = true;
    condvar.notify_one();
}
