# TFTP 服务器

## 简介
`TFTP`（Trivial File Transfer Protocol）服务器的`Rust`实现

## 功能特性
- 支持 `TFTP` 协议的标准读请求
- 支持选项扩展（`blksize`、`windowsize`、`ts`）
- 基于`tokio`异步运行时，高性能，高并发

## 安装与使用
```bash
# 通过cargo安装
cargo install --git https://github.com/lbhzy/tftp-rs

# 使用
tftpd
