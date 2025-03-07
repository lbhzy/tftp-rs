use anyhow::anyhow;
use clap::Parser;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::task;
use tokio::time::{timeout, Duration, Instant};

use tftp::Cli;
use tftp::TftpPacket;
use tftp::Window;

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Cli::parse();
    let socket = UdpSocket::bind(format!("{}:{}", args.ip, args.port)).await?;
    let mut buf: [u8; 100] = [0; 100];
    std::env::set_current_dir(args.directory)?;
    let workdir = std::env::current_dir()?;
    let timeout = Duration::from_millis(args.timeout);
    let max_retries = args.retry;
    let gbn = args.gbn;

    println!(
        "TFTP server listen on {}, workdir: {}\nGBN: {}, timeout: {} ms, retry: {}",
        socket.local_addr()?,
        workdir.display(),
        gbn,
        args.timeout,
        max_retries
    );

    loop {
        let (num, addr) = socket.recv_from(&mut buf).await?;

        if let Ok(pkt) = TftpPacket::deserialize(&buf[..num]) {
            println!("{addr} {pkt:?}");
            match pkt {
                TftpPacket::RRQ {
                    filename,
                    mode,
                    options,
                } => {
                    task::spawn(async move {
                        rrq_handler(addr, filename, mode, options, timeout, max_retries, gbn)
                            .await
                            .unwrap()
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

async fn rrq_handler(
    addr: SocketAddr,
    filename: String,
    mode: String,
    options: HashMap<String, String>,
    timeout_duration: Duration,
    max_retries: u8,
    mut gbn: bool,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(addr).await?;

    // 仅支持octet模式
    if mode != "octet" {
        return send_error(&socket, format!("Unsupported '{mode}' mode")).await;
    }

    // 获取文件大小
    let path = std::path::Path::new(&filename);
    let filename = path
        .file_name()
        .ok_or(anyhow!("{:?}", path))?
        .to_str()
        .ok_or(anyhow!("Illegal characters"))?;
    let filesize = match fs::metadata(&filename) {
        Ok(metadata) => metadata.len(),
        Err(e) => {
            return send_error(&socket, format!("{e}")).await;
        }
    };

    let mut windowsize = tftp::DEF_WINDOW_SIZE;
    let mut blksize = tftp::DEF_BLOCK_SIZE;
    // 选项协商
    let mut nego_options: HashMap<String, String> = HashMap::new();
    for (key, value) in options {
        match key.as_str() {
            "blksize" => {
                blksize = value.parse()?;
                blksize = std::cmp::min(blksize, tftp::MAX_BLOCK_SIZE);
                blksize = std::cmp::max(blksize, tftp::MIN_BLOCK_SIZE);
                nego_options.insert(key, blksize.to_string());
            }
            "windowsize" => {
                windowsize = value.parse()?;
                nego_options.insert(key, value);
            }
            "tsize" => {
                nego_options.insert(key, filesize.to_string());
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
            let Ok(block) = timeout(timeout_duration, recv_ack(&socket)).await? else {
                println!("timeout");
                retries += 1;
                if retries == max_retries {
                    return send_error(&socket, format!("Max retries reached")).await;
                }
                continue;
            };
            if block == 0 {
                break;
            } else {
                return send_error(&socket, format!("expect block #0, but #{block}")).await;
            }
        }
    }

    if gbn && windowsize == 1 {
        windowsize = 4;
    } else {
        gbn = false;
    }
    // 开始传输
    let mut send_buf: Vec<u8> = vec![0; usize::from(blksize)];
    let mut file = File::open(filename)?;
    let mut window = Window::new(windowsize);
    let mut size: usize = 0;
    let mut finish = false;
    let mut retries: u8 = 0;
    while !finish {
        for block in &mut window {
            if let Ok(_) = socket.try_peek_sender() {
                window.next_send = window.next_send.wrapping_sub(1);
                break;
            }

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
        let ack_block = match timeout(timeout_duration, recv_ack(&socket)).await {
            Ok(res) => {
                retries = 0;
                res?
            }
            Err(_) => {
                println!("timeout");
                retries += 1;
                if retries == max_retries {
                    return send_error(&socket, format!("Max retries reached")).await;
                }
                window.start.wrapping_sub(1)
            }
        };
        let offset = window.update(ack_block, gbn);
        let offset_size;
        if offset != 0 {
            println!("retrans #{}", window.next_send);
            if offset < 0 {
                offset_size = (offset + 1) * (blksize as i64) + (size as i64);
            } else {
                offset_size = offset * (blksize as i64);
            }
            file.seek(SeekFrom::Current(offset_size))?;
            finish = false;
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

async fn send_error(socket: &UdpSocket, msg: String) -> anyhow::Result<()> {
    let error = TftpPacket::ERROR {
        code: 0,
        msg: msg.clone(),
    };
    socket.send(&error.serialize()).await?;
    Err(anyhow!(msg))
}

async fn recv_ack(socket: &UdpSocket) -> anyhow::Result<u16> {
    let mut buf = [0; 100];
    let n = socket.recv(&mut buf).await?;

    match TftpPacket::deserialize(&buf[..n])? {
        TftpPacket::ACK(ack) => Ok(ack),
        TftpPacket::ERROR { code, msg } => {
            Err(anyhow!("Get error packet: code: {code}, msg: {msg}"))
        }
        _ => Err(anyhow!("Not ack packet")),
    }
}
