use crate::packet::TftpPacket;
use crate::window::Window;
use anyhow::anyhow;
use log::{info, warn};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write, Seek, SeekFrom};
use std::net::SocketAddr;
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

pub struct Session {
    socket: UdpSocket,
    config: SessionConfig,

    filename: Option<String>,
    filesize: Option<u64>,
    mode: Option<String>,
    blksize: u16,
    windowsize: u16,
    first_data: Option<(u16, Vec<u8>)>,
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
            first_data: None,
        }
    }

    fn resolve_path(&self, filename: &str) -> anyhow::Result<PathBuf> {
        let path = std::path::Path::new(filename);
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    return Err(anyhow!("Access denied: path traversal detected"));
                }
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                    return Err(anyhow!("Access denied: absolute paths not allowed"));
                }
                _ => {}
            }
        }
        Ok(self.config.directory.join(filename))
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
        let filename = self.filename.as_ref().ok_or(anyhow!("No filename"))?.clone();
        let path = self.resolve_path(&filename)?;
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
        let path = self.resolve_path(self.filename.as_ref().unwrap())?;
        let mut file = File::open(path)?;
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
                    offset_size = (offset + 1) * (self.blksize as i64) - (size as i64);
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

    pub async fn recv_file(&mut self) -> anyhow::Result<()> {
        let start = Instant::now();
        let path = self.resolve_path(self.filename.as_ref().unwrap())?;
        let mut file = File::create(path)?;
        let mut expected_block: u16 = 1;
        let mut retries: u8 = 0;
        let mut total_size: u64 = 0;
        let mut window_count: u16 = 0;

        // Handle buffered first DATA packet (server responded with DATA#1 instead of OACK)
        if let Some((block, data)) = self.first_data.take() {
            if block == expected_block {
                file.write_all(&data)?;
                total_size += data.len() as u64;
                window_count += 1;
                let is_last = data.len() < self.blksize as usize;
                if is_last || window_count >= self.windowsize {
                    let ack = TftpPacket::ACK(block);
                    self.socket.send(&ack.serialize()).await?;
                    window_count = 0;
                }
                if is_last {
                    let cost = start.elapsed();
                    info!(
                        "recv cost: {:.3}s, size: {} bytes, speed: {:.2} MB/s",
                        cost.as_secs_f64(),
                        total_size,
                        total_size as f64 / cost.as_secs_f64() / 1024.0 / 1024.0
                    );
                    return Ok(());
                }
                expected_block = expected_block.wrapping_add(1);
            }
        }

        loop {
            let mut buf = vec![0u8; self.blksize as usize + 4];
            match timeout(
                Duration::from_millis(self.config.timeout),
                self.socket.recv(&mut buf),
            )
            .await
            {
                Ok(Ok(n)) => {
                    retries = 0;
                    match TftpPacket::deserialize(&buf[..n])? {
                        TftpPacket::DATA { block, data } => {
                            if block == expected_block {
                                file.write_all(&data)?;
                                total_size += data.len() as u64;
                                window_count += 1;
                                let is_last = data.len() < self.blksize as usize;

                                // RFC 7440: only ACK at window boundary or last packet
                                if is_last || window_count >= self.windowsize {
                                    let ack = TftpPacket::ACK(block);
                                    self.socket.send(&ack.serialize()).await?;
                                    window_count = 0;
                                }

                                if is_last {
                                    break;
                                }
                                expected_block = expected_block.wrapping_add(1);
                            } else {
                                let ack = TftpPacket::ACK(expected_block.wrapping_sub(1));
                                self.socket.send(&ack.serialize()).await?;
                                window_count = 0;
                            }
                        }
                        TftpPacket::ERROR { code, msg } => {
                            return Err(anyhow!("Peer error: code={}, msg={}", code, msg));
                        }
                        _ => {}
                    }
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => {
                    warn!("timeout waiting for DATA#{}", expected_block);
                    retries += 1;
                    if retries >= self.config.retry {
                        return self.send_error("Max retries reached".to_string()).await;
                    }
                    let ack = TftpPacket::ACK(expected_block.wrapping_sub(1));
                    self.socket.send(&ack.serialize()).await?;
                    window_count = 0;
                }
            }
        }

        let cost = start.elapsed();
        info!(
            "recv cost: {:.3}s, size: {} bytes, speed: {:.2} MB/s",
            cost.as_secs_f64(),
            total_size,
            total_size as f64 / cost.as_secs_f64() / 1024.0 / 1024.0
        );
        Ok(())
    }

    pub async fn send_rrq(
        &mut self,
        server_addr: SocketAddr,
        filename: &str,
        blksize: u16,
        windowsize: u16,
    ) -> anyhow::Result<()> {
        self.filename = Some(filename.to_string());
        self.blksize = blksize;
        self.windowsize = windowsize;

        let mut options = HashMap::new();
        if blksize != DEF_BLOCK_SIZE {
            options.insert("blksize".to_string(), blksize.to_string());
        }
        if windowsize != DEF_WINDOW_SIZE {
            options.insert("windowsize".to_string(), windowsize.to_string());
        }
        options.insert("tsize".to_string(), "0".to_string());

        let pkt = TftpPacket::RRQ {
            filename: filename.to_string(),
            mode: "octet".to_string(),
            options: options.clone(),
        };
        let bytes = pkt.serialize();
        self.socket.send_to(&bytes, server_addr).await?;

        let mut retries: u8 = 0;
        loop {
            let mut buf = vec![0u8; self.blksize as usize + 4];
            match timeout(
                Duration::from_millis(self.config.timeout),
                self.socket.recv_from(&mut buf),
            )
            .await
            {
                Ok(Ok((n, peer))) => {
                    // Connect to the server's new TID (transfer port)
                    self.socket.connect(peer).await?;
                    match TftpPacket::deserialize(&buf[..n])? {
                        TftpPacket::OACK(opts) => {
                            if let Some(v) = opts.get("blksize") {
                                self.blksize = v.parse()?;
                            }
                            if let Some(v) = opts.get("windowsize") {
                                self.windowsize = v.parse()?;
                            }
                            if let Some(v) = opts.get("tsize") {
                                self.filesize = Some(v.parse()?);
                            }
                            info!("negotiated: {:?}", opts);
                            let ack = TftpPacket::ACK(0);
                            self.socket.send(&ack.serialize()).await?;
                            break;
                        }
                        TftpPacket::DATA { block, data } => {
                            // Server didn't send OACK, responded with DATA directly
                            self.blksize = DEF_BLOCK_SIZE;
                            self.windowsize = DEF_WINDOW_SIZE;
                            self.first_data = Some((block, data));
                            break;
                        }
                        TftpPacket::ERROR { code, msg } => {
                            return Err(anyhow!("Server error: code={}, msg={}", code, msg));
                        }
                        _ => {
                            return Err(anyhow!("Unexpected packet during RRQ negotiation"));
                        }
                    }
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => {
                    retries += 1;
                    if retries >= self.config.retry {
                        return Err(anyhow!("Max retries reached during RRQ"));
                    }
                    warn!("timeout, resending RRQ");
                    self.socket.send_to(&bytes, server_addr).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn send_wrq(
        &mut self,
        server_addr: SocketAddr,
        filename: &str,
        blksize: u16,
        windowsize: u16,
    ) -> anyhow::Result<()> {
        self.filename = Some(filename.to_string());
        self.blksize = blksize;
        self.windowsize = windowsize;
        self.get_filesize()?;

        let mut options = HashMap::new();
        if blksize != DEF_BLOCK_SIZE {
            options.insert("blksize".to_string(), blksize.to_string());
        }
        if windowsize != DEF_WINDOW_SIZE {
            options.insert("windowsize".to_string(), windowsize.to_string());
        }
        options.insert("tsize".to_string(), self.filesize.unwrap().to_string());

        let pkt = TftpPacket::WRQ {
            filename: filename.to_string(),
            mode: "octet".to_string(),
            options: options.clone(),
        };
        let bytes = pkt.serialize();
        self.socket.send_to(&bytes, server_addr).await?;

        let mut retries: u8 = 0;
        loop {
            let mut buf = [0u8; 1500];
            match timeout(
                Duration::from_millis(self.config.timeout),
                self.socket.recv_from(&mut buf),
            )
            .await
            {
                Ok(Ok((n, peer))) => {
                    // Connect to the server's new TID
                    self.socket.connect(peer).await?;
                    match TftpPacket::deserialize(&buf[..n])? {
                        TftpPacket::OACK(opts) => {
                            if let Some(v) = opts.get("blksize") {
                                self.blksize = v.parse()?;
                            }
                            if let Some(v) = opts.get("windowsize") {
                                self.windowsize = v.parse()?;
                            }
                            info!("WRQ negotiated: {:?}", opts);
                            break;
                        }
                        TftpPacket::ACK(0) => {
                            self.blksize = DEF_BLOCK_SIZE;
                            self.windowsize = DEF_WINDOW_SIZE;
                            break;
                        }
                        TftpPacket::ERROR { code, msg } => {
                            return Err(anyhow!("Server error: code={}, msg={}", code, msg));
                        }
                        _ => return Err(anyhow!("Unexpected packet during WRQ negotiation")),
                    }
                }
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => {
                    retries += 1;
                    if retries >= self.config.retry {
                        return Err(anyhow!("Max retries reached during WRQ"));
                    }
                    warn!("timeout, resending WRQ");
                    self.socket.send_to(&bytes, server_addr).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn negotiation_wrq(
        &mut self,
        filename: String,
        mode: String,
        options: HashMap<String, String>,
    ) -> anyhow::Result<()> {
        self.filename = Some(filename);
        self.mode = Some(mode);

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
                    self.filesize = Some(value.parse()?);
                    nego_options.insert(key, value);
                }
                _ => (),
            }
        }

        if !nego_options.is_empty() {
            info!("wrq nego: {:?}", nego_options);
            let oack = TftpPacket::OACK(nego_options);
            self.socket.send(&oack.serialize()).await?;
        } else {
            let ack = TftpPacket::ACK(0);
            self.socket.send(&ack.serialize()).await?;
        }

        Ok(())
    }
}
