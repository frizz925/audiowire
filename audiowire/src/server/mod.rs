use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{packet::stream::StreamId, server::client::Client};

pub mod client;
pub mod heartbeat;
pub mod server;

pub type SharedClientMap = Arc<RwLock<HashMap<StreamId, Client>>>;
