use crate::session::Session;
use crate::SessionConfig;
use log::info;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

pub struct TftpClient {
    config: SessionConfig,
    blksize: u16,
    windowsize: u16,
}

impl TftpClient {
    pub fn new(config: SessionConfig, blksize: u16, windowsize: u16) -> Self {
        Self {
            config,
            blksize,
            windowsize,
        }
    }

    pub async fn get_file(&self, addr: SocketAddr, filename: String) -> anyhow::Result<()> {
        info!("GET {} from {}", filename, addr);
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let mut session = Session::new(socket, self.config.clone());
        session
            .send_rrq(addr, &filename, self.blksize, self.windowsize)
            .await?;
        session.recv_file().await?;
        Ok(())
    }

    pub async fn put_file(&self, addr: SocketAddr, filename: String) -> anyhow::Result<()> {
        info!("PUT {} to {}", filename, addr);
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let mut session = Session::new(socket, self.config.clone());
        session
            .send_wrq(addr, &filename, self.blksize, self.windowsize)
            .await?;
        session.send_file().await?;
        Ok(())
    }
}
