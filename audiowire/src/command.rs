use clap::{Arg, ArgAction, ArgMatches, Command};

pub fn add_device_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("source")
            .short('i')
            .long("source")
            .conflicts_with("disable-source")
            .help("Name of the audio device to be used as source"),
    )
    .arg(
        Arg::new("sink")
            .short('o')
            .long("sink")
            .conflicts_with("disable-sink")
            .help("Name of the audio device to be used as sink"),
    )
    .arg(
        Arg::new("disable-source")
            .long("disable-source")
            .conflicts_with("source")
            .action(ArgAction::SetTrue)
            .help("Don't use any audio device as source"),
    )
    .arg(
        Arg::new("disable-sink")
            .long("disable-sink")
            .conflicts_with("sink")
            .action(ArgAction::SetTrue)
            .help("Don't use any audio device as sink"),
    )
    .arg(
        Arg::new("disable-opus")
            .long("disable-opus")
            .action(ArgAction::SetTrue)
            .help("Don't use Opus audio packet compression"),
    )
}

#[derive(Clone)]
pub struct DeviceConfig {
    pub source_name: Option<String>,
    pub sink_name: Option<String>,

    pub source_enabled: bool,
    pub sink_enabled: bool,
    pub opus_enabled: bool,
}

impl From<&ArgMatches> for DeviceConfig {
    fn from(matches: &ArgMatches) -> Self {
        Self {
            source_name: match_opt_string(&matches, "source"),
            sink_name: match_opt_string(&matches, "sink"),

            source_enabled: !match_bool(&matches, "disable-source"),
            sink_enabled: !match_bool(&matches, "disable-sink"),
            opus_enabled: !match_bool(&matches, "disable-opus"),
        }
    }
}

fn match_bool(matches: &ArgMatches, id: &str) -> bool {
    matches.get_one(id).map(bool::to_owned).unwrap_or_default()
}

fn match_opt_string(matches: &ArgMatches, id: &str) -> Option<String> {
    matches.get_one(id).map(String::to_owned)
}
