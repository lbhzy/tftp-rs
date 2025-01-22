use anyhow::{anyhow, Ok};
use std::collections::HashMap;
use std::str;

#[derive(Debug)]
pub enum TftpPacket {
    RRQ {
        filename: String,
        mode: String,
        options: HashMap<String, String>,
    },
    WRQ {
        filename: String,
        mode: String,
        options: HashMap<String, String>,
    },
    DATA {
        block: u16,
        data: Vec<u8>,
    },
    ACK(u16),
    ERROR {
        code: u16,
        msg: String,
    },
    OACK(HashMap<String, String>),
}

impl TftpPacket {
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = vec![0];

        match self {
            TftpPacket::RRQ {
                filename,
                mode,
                options,
            }
            | TftpPacket::WRQ {
                filename,
                mode,
                options,
            } => {
                if let TftpPacket::RRQ { .. } = self {
                    bytes.push(1);
                } else {
                    bytes.push(2);
                }
                bytes.extend(filename.as_bytes());
                bytes.push(0);
                bytes.extend(mode.as_bytes());
                bytes.push(0);
                for (key, value) in options {
                    bytes.extend(key.as_bytes());
                    bytes.push(0);
                    bytes.extend(value.as_bytes());
                    bytes.push(0);
                }
            }
            TftpPacket::DATA { block, data } => {
                bytes.push(3);
                bytes.push(block.to_be_bytes()[0]);
                bytes.push(block.to_be_bytes()[1]);
                bytes.extend_from_slice(data);
            }
            TftpPacket::ACK(block) => {
                bytes.push(4);
                bytes.push(block.to_be_bytes()[0]);
                bytes.push(block.to_be_bytes()[1]);
            }
            TftpPacket::ERROR { code, msg } => {
                bytes.push(5);
                bytes.push(code.to_be_bytes()[0]);
                bytes.push(code.to_be_bytes()[1]);
                bytes.extend_from_slice(msg.as_bytes());
                bytes.push(0);
            }
            TftpPacket::OACK(nego_options) => {
                bytes.push(6);
                for (key, value) in nego_options {
                    bytes.extend_from_slice(key.as_bytes());
                    bytes.push(0);
                    bytes.extend_from_slice(value.as_bytes());
                    bytes.push(0);
                }
            }
        }
        bytes
    }

    pub fn deserialize(buf: &[u8]) -> anyhow::Result<Self> {
        if buf.len() < 4 {
            return Err(anyhow!("Packet length too short"));
        }

        let opcode = u16::from_be_bytes([buf[0], buf[1]]);
        let pkt = match opcode {
            1 | 2 => {
                let filename = read_cstr(&buf[2..])?;
                let mode = read_cstr(&buf[2 + filename.len() + 1..])?;
                let options = read_options(&buf[2 + filename.len() + 1 + mode.len() + 1..])?;
                if opcode == 1 {
                    TftpPacket::RRQ {
                        filename,
                        mode,
                        options,
                    }
                } else {
                    TftpPacket::WRQ {
                        filename,
                        mode,
                        options,
                    }
                }
            }
            3 => {
                let block = u16::from_be_bytes([buf[2], buf[3]]);
                let data = buf[4..].to_vec();

                TftpPacket::DATA { block, data }
            }
            4 => TftpPacket::ACK(u16::from_be_bytes(buf[2..4].try_into()?)),
            5 => {
                let code = u16::from_be_bytes([buf[2], buf[3]]);
                let msg = read_cstr(&buf[4..])?;

                TftpPacket::ERROR { code, msg }
            }
            6 => TftpPacket::OACK(read_options(&buf[2..])?),
            _ => {
                return Err(anyhow!("Invalid opcode: {}", opcode));
            }
        };

        Ok(pkt)
    }
}

// 读取以 \0 结尾的 C 风格字符串
fn read_cstr(buf: &[u8]) -> anyhow::Result<String> {
    let pos = buf
        .iter()
        .position(|&b| b == 0)
        .ok_or(anyhow!("Missing cstr terminator"))?;
    let s = str::from_utf8(&buf[..pos])
        .map_err(|_| anyhow!("Invalid cstr encoding"))?
        .to_string();
    Ok(s)
}

// 读取选项（键值对）
fn read_options(buf: &[u8]) -> anyhow::Result<HashMap<String, String>> {
    let mut options = HashMap::new();
    let mut pos = 0;
    while pos < buf.len() {
        // 解析选项
        let key_end = buf[pos..].iter().position(|&b| b == 0).ok_or(anyhow!(
            "Missing option key terminator"
        ))?;
        let key = str::from_utf8(&buf[pos..pos + key_end])
            .map_err(|_| anyhow!("Invalid option key encoding"))?
            .to_string();
        pos += key_end + 1;

        // 解析选项值
        let value_end = buf[pos..].iter().position(|&b| b == 0).ok_or(anyhow!(
            "Missing option value terminator"
        ))?;
        let value = str::from_utf8(&buf[pos..pos + value_end])
            .map_err(|_| anyhow!("Invalid option value encoding"))?
            .to_string();
        pos += value_end + 1;

        options.insert(key, value);
    }
    Ok(options)
}
