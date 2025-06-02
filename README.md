# decrypt_ncm

A tool for decrypting NCM files - a solution for converting encrypted NCM audio files to FLAC format.

## 项目概述

decrypt_ncm 是一个用于解密网易云音乐 NCM 文件的工具，将加密的 NCM 文件转换为FLAC音频文件。

## 核心功能

-  解密 NCM 文件
-  提取音频元数据（如歌曲名称、艺术家等）
-  将解密后的音频保存为FLAC格式

## 构建与运行

### 环境要求

- Rust 工具链

### 构建步骤

```bash
# 构建 debug 版本
cargo build

# 构建 release 版本
cargo build --release
```

### 使用方法

```bash
# 运行程序并指定 NCM 文件路径
cargo run <ncm_file_path>

# 或者运行已构建的二进制文件
./target/release/decrypt_ncm <ncm_file_path>
```

## 许可证

MIT License