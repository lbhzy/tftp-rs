use anyhow::anyhow;
use clap::Parser;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::task;
use tokio::time::{Duration, Instant};

use tftp::Cli;
use tftp::TftpPacket;
use tftp::Window;

#[tokio::main]
async fn main() -> io::Result<()> {
    let _args = Cli::parse();
    let socket = UdpSocket::bind("0.0.0.0:69").await?;
    let mut buf: [u8; 100] = [0; 100];
    let workdir = std::env::current_dir()?;

    println!(
        "TFTP server listen on {}, workdir: {}",
        socket.local_addr()?,
        workdir.display()
    );

    loop {
        let (num, addr) = socket.recv_from(&mut buf).await?;

        let Ok(pkt) = TftpPacket::deserialize(&buf[..num]) else {
            continue;
        };
        println!("{addr} {pkt:?}");

        match pkt {
            TftpPacket::RRQ {
                filename,
                mode,
                options,
            } => {
                task::spawn(
                    async move { rrq_handler(addr, filename, mode, options).await.unwrap() },
                );
            }
            _ => (),
        }
    }
}

async fn rrq_handler(
    addr: SocketAddr,
    filename: String,
    mode: String,
    options: HashMap<String, String>,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(addr).await?;
    // socket.set_read_timeout(Some(Duration::from_secs(tftp::DEF_TIMEOUT_SEC)))?;

    // 仅支持octet模式
    if mode != "octet" {
        let error = TftpPacket::ERROR {
            code: 0,
            msg: format!("unsupported mode: {mode}").to_string(),
        };
        socket.send(&error.serialize()).await?;
        return Err(anyhow!("unsupported mode: {mode}"));
    }

    // 判断文件是否存在
    let file_metadata = match fs::metadata(&filename) {
        Ok(x) => x,
        Err(e) => {
            let error = TftpPacket::ERROR {
                code: 1,
                msg: format!("{:?}", e.kind()).to_string(),
            };
            socket.send(&error.serialize()).await?;
            Err(e)
        }?,
    };
    let filesize = file_metadata.len();

    let mut windowsize: u16 = tftp::DEF_WINDOW_SIZE;
    let mut blksize: u16 = tftp::DEF_BLOCK_SIZE;
    // 选项协商
    let mut nego_options: HashMap<String, String> = HashMap::new();
    for (key, value) in options {
        match key.as_str() {
            "blksize" => {
                let Ok(value) = value.parse() else {
                    return Err(anyhow!("key value parse error"));
                };
                let value = std::cmp::min(value, tftp::MAX_BLOCK_SIZE);
                let value = std::cmp::max(value, tftp::MIN_BLOCK_SIZE);
                blksize = value;
                nego_options.insert(key, format!("{value}"));
            }
            "windowsize" => {
                windowsize = value.parse()?;
                nego_options.insert(key, value);
            }
            "tsize" => {
                nego_options.insert(key, format!("{}", filesize));
            }
            _ => (),
        }
    }
    if !nego_options.is_empty() {
        println!("nego: {:?}", nego_options);
        let oack = TftpPacket::OACK(nego_options);
        let oack = oack.serialize();

        let mut retries: u8 = 0;
        loop {
            socket.send(&oack).await?;

            let mut recv_buf: [u8; 100] = [0; 100];
            match recv_packet(&socket, &mut recv_buf).await {
                Ok(pkt) => {
                    if let TftpPacket::ACK(block) = pkt {
                        if block == 0 {
                            break;
                        } else {
                            return Err(anyhow!("expect block #0, but #{block}"));
                        }
                    } else {
                        return Err(anyhow!("expect block #0, but {pkt:?}"));
                    }
                }
                Err(e) => {
                    if let Some(io_error) = e.downcast_ref::<io::Error>() {
                        if io_error.kind() == io::ErrorKind::WouldBlock
                            || io_error.kind() == io::ErrorKind::TimedOut
                        {
                            println!("timeout");
                            retries += 1;
                            if retries == tftp::MAX_RETRY_COUNT {
                                return Err(anyhow!("Max retries reached"));
                            }
                        } else {
                            return Err(e);
                        }
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }

    // 开始传输
    // socket.send(TftpPacket::ERROR { code: 0, msg: "no way".to_string() }.serialize().as_slice())?;
    let mut send_buf: Vec<u8> = vec![0; usize::from(blksize)];
    let mut recv_buf: [u8; 100] = [0; 100];
    let mut file = File::open(filename)?;
    let mut window = Window::new(windowsize);
    let mut size: usize = 0;
    let mut finish = false;
    while !finish {
        for block in &mut window {
            size = file.read(&mut send_buf[..])?;
            let data = &send_buf[..size];
            let pkt = TftpPacket::DATA {
                block: block,
                data: data.to_vec(),
            };

            socket.send(&pkt.serialize()).await?;
            if size < usize::from(blksize) {
                finish = true;
                break;
            }
        }
        if let TftpPacket::ACK(ack_block) = recv_packet(&socket, &mut recv_buf).await? {
            let tmp = window.next_send;
            window.update(ack_block);
            let diff = tmp.wrapping_sub(window.next_send);
            if diff != 0 {
                println!("retrans {diff}");
                let diff: i64 = ((diff - 1) as i64) * (blksize as i64) + (size as i64);
                let _ = file.seek(SeekFrom::Current(-diff))?;
                finish = false;
            }
        }
    }

    let cost = start.elapsed();
    println!(
        "cost: {:.3}s, speed: {:.2} MB/s",
        cost.as_secs_f64(),
        filesize as f64 / cost.as_secs_f64() / 1024.0 / 1024.0
    );
    Ok(())
}

async fn recv_packet(socket: &UdpSocket, buf: &mut [u8]) -> anyhow::Result<TftpPacket> {
    let num = socket.recv(buf).await?;
    match TftpPacket::deserialize(&buf[..num])? {
        TftpPacket::ERROR { code, msg } => {
            Err(anyhow!("Get error packet: code: {code}, msg: {msg}"))
        }
        other => Ok(other),
    }
}
