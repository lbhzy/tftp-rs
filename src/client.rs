use crate::packet::TftpPacket;
use crate::session::Session;
use crate::{SessionConfig, session};
use anyhow::Ok;
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
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(addr).await?;
        let mut session = Session::new(socket, self.config.clone());

        Ok(())
    }
}
