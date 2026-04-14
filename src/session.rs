use crate::packet::TftpPacket;
use crate::window::Window;
use anyhow::anyhow;
use log::{error, info, warn};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use tokio::net::UdpSocket;
use tokio::time::{Duration, Instant, timeout};

const DEF_BLOCK_SIZE: u16 = 512; // RFC 1350
const MIN_BLOCK_SIZE: u16 = 8; // RFC 2348
const MAX_BLOCK_SIZE: u16 = 65464; // RFC 2348
const DEF_WINDOW_SIZE: u16 = 1;

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub directory: PathBuf,
    pub timeout: u64,
    pub retry: u8,
    pub gbn: bool,
}

pub struct Options {
    pub blksize: Option<u16>,
    pub timeout: Option<u64>,
    pub tsize: Option<u64>,
}

pub struct Session {
    socket: UdpSocket,
    config: SessionConfig,

    filename: Option<String>,
    filesize: Option<u64>,
    mode: Option<String>,
    blksize: u16,
    windowsize: u16,
}

impl Session {
    pub fn new(socket: UdpSocket, config: SessionConfig) -> Self {
        Self {
            socket,
            config,
            filename: None,
            filesize: None,
            mode: None,
            blksize: DEF_BLOCK_SIZE,
            windowsize: DEF_WINDOW_SIZE,
        }
    }

    pub async fn negotiation(
        &mut self,
        filename: String,
        mode: String,
        options: HashMap<String, String>,
    ) -> anyhow::Result<()> {
        self.filename = Some(filename.clone());
        self.mode = Some(mode);

        self.get_filesize()?;

        let mut nego_options: HashMap<String, String> = HashMap::new();
        for (key, value) in options {
            match key.as_str() {
                "blksize" => {
                    self.blksize = value.parse()?;
                    self.blksize = std::cmp::min(self.blksize, MAX_BLOCK_SIZE);
                    self.blksize = std::cmp::max(self.blksize, MIN_BLOCK_SIZE);
                    nego_options.insert(key, self.blksize.to_string());
                }
                "windowsize" => {
                    self.windowsize = value.parse()?;
                    nego_options.insert(key, self.windowsize.to_string());
                }
                "tsize" => {
                    nego_options.insert(key, self.filesize.unwrap().to_string());
                }
                _ => (),
            }
        }
        if !nego_options.is_empty() {
            info!("nego: {:?}", nego_options);
            let oack = TftpPacket::OACK(nego_options);
            let oack = oack.serialize();

            let mut retries: u8 = 0;
            loop {
                self.socket.send(&oack).await?;
                let block = match timeout(
                    Duration::from_millis(self.config.timeout),
                    self.recv_ack(),
                )
                .await
                {
                    Ok(res) => res?,
                    Err(_) => {
                        println!("timeout");
                        retries += 1;
                        if retries == self.config.retry {
                            self.send_error(format!("Max retries reached")).await?;
                        }
                        continue;
                    }
                };
                if block == 0 {
                    break;
                } else {
                    self.send_error(format!("expect block #0, but #{block}"))
                        .await?;
                }
            }
        }

        Ok(())
    }

    pub async fn send_error(&self, msg: String) -> anyhow::Result<()> {
        let pkt = TftpPacket::ERROR {
            code: 0,
            msg: msg.clone(),
        };
        let bytes = pkt.serialize();
        self.socket.send(&bytes).await?;
        Err(anyhow!(msg))
    }

    fn get_filesize(&mut self) -> anyhow::Result<()> {
        let filename = self.filename.as_ref().ok_or(anyhow!("No filename"))?;
        let path = std::path::Path::new(filename);
        let metadata = fs::metadata(path)?;
        self.filesize = Some(metadata.len());
        Ok(())
    }

    async fn recv_ack(&self) -> anyhow::Result<u16> {
        let mut buf = [0; 100];
        let n = self.socket.recv(&mut buf).await?;

        match TftpPacket::deserialize(&buf[..n])? {
            TftpPacket::ACK(ack) => Ok(ack),
            TftpPacket::ERROR { code, msg } => {
                Err(anyhow!("Get error packet: code: {code}, msg: {msg}"))
            }
            _ => Err(anyhow!("Not ack packet")),
        }
    }

    pub async fn send_file(&mut self) -> anyhow::Result<()> {
        let start = Instant::now();
        let mut gbn = self.config.gbn;
        if gbn && self.windowsize == 1 {
            self.windowsize = 4;
        } else {
            gbn = false;
        }
        let mut send_buf: Vec<u8> = vec![0; usize::from(self.blksize)];
        let mut file = File::open(self.filename.as_ref().unwrap())?;
        let mut window = Window::new(self.windowsize);
        let mut size: usize = 0;
        let mut finish = false;
        let mut retries: u8 = 0;
        while !finish {
            for block in &mut window {
                if let Ok(_) = self.socket.try_peek_sender() {
                    window.next_send = window.next_send.wrapping_sub(1);
                    break;
                }

                size = file.read(&mut send_buf[..])?;
                let data = &send_buf[..size];
                let pkt = TftpPacket::DATA {
                    block: block,
                    data: data.to_vec(),
                };

                self.socket.send(&pkt.serialize()).await?;
                if size < usize::from(self.blksize) {
                    finish = true;
                    break;
                }
            }
            let ack_block =
                match timeout(Duration::from_millis(self.config.timeout), self.recv_ack()).await {
                    Ok(res) => {
                        retries = 0;
                        res?
                    }
                    Err(_) => {
                        warn!("timeout");
                        retries += 1;
                        if retries == self.config.retry {
                            return self.send_error(format!("Max retries reached")).await;
                        }
                        window.start.wrapping_sub(1)
                    }
                };
            let offset = window.update(ack_block, gbn);
            let offset_size;
            if offset != 0 {
                warn!("retrans #{}", window.next_send);
                if offset < 0 {
                    offset_size = (offset + 1) * (self.blksize as i64) + (size as i64);
                } else {
                    offset_size = offset * (self.blksize as i64);
                }
                file.seek(SeekFrom::Current(offset_size))?;
                finish = false;
            }
        }

        let cost = start.elapsed();
        info!(
            "cost: {:.3}s, speed: {:.2} MB/s",
            cost.as_secs_f64(),
            self.filesize.unwrap() as f64 / cost.as_secs_f64() / 1024.0 / 1024.0
        );
        Ok(())
    }

    pub fn recv_file(&self) {}

    pub fn send_rrq(&self, filename: &str) {}
}
