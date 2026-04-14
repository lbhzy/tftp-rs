use crate::SessionConfig;
use crate::packet::TftpPacket;
use crate::session::Session;
use log::info;
use std::net::SocketAddr;
use tokio::net::UdpSocket;

pub struct TftpServer {
    addr: SocketAddr,
    config: SessionConfig,
}

impl TftpServer {
    pub fn new(addr: SocketAddr, config: SessionConfig) -> Self {
        Self { addr, config }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(self.addr).await?;

        info!("TFTP server listening on {}", self.addr);

        loop {
            let mut buf = [0u8; 1500];
            let (len, peer) = socket.recv_from(&mut buf).await?;

            if let Ok(pkt) = TftpPacket::deserialize(&buf[..len]) {
                info!("{peer} {pkt:?}");
                match pkt {
                    TftpPacket::RRQ {
                        filename,
                        mode,
                        options,
                    } => {
                        let config = self.config.clone();
                        tokio::spawn(async move {
                            let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
                            socket.connect(peer).await.unwrap();
                            let mut session = Session::new(socket, config);
                            session.negotiation(filename, mode, options).await.unwrap();
                            session.send_file().await.unwrap();
                        });
                    }
                    TftpPacket::WRQ { .. } => {
                        println!("WRQ is not supported");
                    }
                    _ => (),
                }
            }
        }
    }
}
