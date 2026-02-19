use log::LevelFilter;

pub fn initialize() {
    env_logger::builder().filter_level(LevelFilter::Info).init();
}
