# TFTP 服务器 & 客户端

## 简介
`TFTP`（Trivial File Transfer Protocol）服务器和客户端的`Rust`实现

## 功能特性
- 支持 `TFTP` 协议的标准读请求（RRQ）和写请求（WRQ）
- 支持选项扩展（`blksize`、`windowsize`、`tsize`）
- 基于`tokio`异步运行时，高性能，高并发
- 支持 Go-Back-N 滑动窗口协议
- 路径遍历安全防护

## 安装与使用
```bash
# 通过 cargo 安装
$ cargo install --git https://github.com/lbhzy/tftp-rs
```

### 服务端
```bash
$ server -h
A high-performance asynchronous TFTP server

Usage: server [OPTIONS]

Options:
  -a, --addr <ADDR>            Listen address [default: 0.0.0.0:69]
  -d, --directory <DIRECTORY>  Work directory [default: .]
  -t, --timeout <TIMEOUT>      Timeout (ms) [default: 1000]
  -r, --retry <RETRY>          Max retries [default: 3]
  -g, --gbn                    Enable GO-Back-N
  -h, --help                   Print help

# 启动服务端（启用 Go-Back-N）
$ server -g
```

### 客户端
```bash
$ client -h
A high-performance asynchronous TFTP client

Usage: client [OPTIONS] <COMMAND>

Commands:
  get   Download a file from TFTP server
  put   Upload a file to TFTP server
  help  Print this message or the help of the given subcommand(s)

Options:
  -a, --addr <ADDR>              Server address [default: 127.0.0.1:69]
  -d, --directory <DIRECTORY>    Local directory [default: .]
  -t, --timeout <TIMEOUT>        Timeout (ms) [default: 1000]
  -r, --retry <RETRY>            Max retries [default: 3]
  -b, --blksize <BLKSIZE>        Block size [default: 512]
  -w, --windowsize <WINDOWSIZE>  Window size [default: 1]
  -h, --help                     Print help

# 从服务端下载文件
$ client -a 192.168.1.1:69 get firmware.bin

# 上传文件到服务端
$ client -a 192.168.1.1:69 put config.txt

# 使用自定义块大小和窗口大小下载
$ client -a 192.168.1.1:69 -b 1468 -w 4 get large_file.bin
```
