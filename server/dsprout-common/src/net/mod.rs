pub mod hello;

use anyhow::Result;
use libp2p::{
    PeerId, StreamProtocol, Swarm,
    core::{Transport, upgrade},
    identify, noise,
    pnet::PnetConfig,
    request_response,
    swarm::{Config as SwarmConfig, NetworkBehaviour},
    tcp, yamux,
};
use std::path::{Path, PathBuf};

use crate::{identity::load_or_create_keypair_for, pnet::load_psk};

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "DsproutEvent")]
pub struct DsproutBehaviour {
    pub identify: identify::Behaviour,
    pub request_response: request_response::cbor::Behaviour<hello::NetRequest, hello::NetResponse>,
}

#[derive(Debug)]
pub enum DsproutEvent {
    Identify(identify::Event),
    RequestResponse(request_response::Event<hello::NetRequest, hello::NetResponse>),
}

impl From<identify::Event> for DsproutEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<request_response::Event<hello::NetRequest, hello::NetResponse>> for DsproutEvent {
    fn from(event: request_response::Event<hello::NetRequest, hello::NetResponse>) -> Self {
        Self::RequestResponse(event)
    }
}

pub fn default_swarm_key_path(manifest_dir: &str) -> PathBuf {
    Path::new(manifest_dir).join("..").join("swarm.key")
}

pub fn build_swarm(
    psk_path: impl AsRef<Path>,
    identity_profile: &str,
) -> Result<Swarm<DsproutBehaviour>> {
    let local_key = load_or_create_keypair_for(identity_profile)?;
    let local_peer_id = PeerId::from(local_key.public());

    let psk = load_psk(psk_path)?;
    let pnet = PnetConfig::new(psk);

    let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
        .and_then(move |socket, _| pnet.handshake(socket))
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(&local_key)?)
        .multiplex(yamux::Config::default())
        .boxed();

    let identify = identify::Behaviour::new(identify::Config::new(
        "/dsprout/identify/1.0.0".to_string(),
        local_key.public(),
    ));

    let rr_cfg =
        request_response::Config::default().with_request_timeout(std::time::Duration::from_secs(5));
    let request_response = request_response::cbor::Behaviour::new(
        [(
            StreamProtocol::new("/dsprout/control/1.0.0"),
            request_response::ProtocolSupport::Full,
        )],
        rr_cfg,
    );

    let behaviour = DsproutBehaviour {
        identify,
        request_response,
    };

    let swarm_config = SwarmConfig::with_tokio_executor()
        .with_idle_connection_timeout(std::time::Duration::from_secs(60));

    let swarm = Swarm::new(transport, behaviour, local_peer_id, swarm_config);

    Ok(swarm)
}
