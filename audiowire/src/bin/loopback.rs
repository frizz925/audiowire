use std::{env, error::Error, sync::atomic::Ordering, thread::sleep, time::Duration};

use audiowire::{
    handlers::handle_signal, initialize, start_playback, start_record, terminate, Config,
    SampleFormat, Stream,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args();
    let input = args.nth(1);
    let output = args.next();
    let config = Config {
        channels: 2,
        sample_rate: 48000,
        sample_format: SampleFormat::F32,
        buffer_frames: 480,
        max_buffer_frames: 4800,
    };

    initialize()?;

    let mut record = start_record(input.as_deref(), config)?;
    let mut playback = start_playback(output.as_deref(), config)?;

    let term = handle_signal()?;
    while !term.load(Ordering::Relaxed) {
        // Do nothing lmao
        sleep(Duration::from_secs(1));
    }

    record.stop()?;
    playback.stop()?;

    terminate()?;

    Ok(())
}
