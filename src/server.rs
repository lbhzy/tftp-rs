use std::net::SocketAddr;
use log::info;
use tokio::net::UdpSocket;
use crate::SessionConfig;
use crate::session::Session;

pub struct TftpServer {
    addr: SocketAddr,
    config: SessionConfig,
}

impl TftpServer {
    pub fn new(addr: SocketAddr, config: SessionConfig) -> Self {
        Self {
            addr,
            config,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(self.addr).await?;
        
        info!("TFTP server listening on {}", self.addr);

        loop {
            let mut buf = [0u8; 1500];
            let (len, peer) = socket.recv_from(&mut buf).await?;

            info!("Received packet from {}", peer);

            let config = self.config.clone();
            tokio::spawn(async move {
                let session = Session::try_new(peer, config).await.unwrap();

            });
        }
    }
}