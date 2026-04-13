use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::UdpSocket;

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub directory: PathBuf,
    pub timeout: u64,
    pub retry: u8,
    pub gbn: bool,
}

pub struct Session {
    peer: SocketAddr,
    socket: UdpSocket,
    config: SessionConfig,
}

impl Session {
    pub async fn try_new(peer: SocketAddr, config: SessionConfig) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        Ok(Self {
            peer,
            socket,
            config,
        })
    }
}