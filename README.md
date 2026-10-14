# decrypt_ncm

## 项目概述

decrypt_ncm 是一个用于解密网易云音乐 NCM 文件的工具，将加密的 NCM 文件转换为FLAC音频文件。

## 核心功能

-  解密 NCM 文件
-  提取音频元数据（如歌曲名称、艺术家等）
-  将解密后的音频保存为FLAC格式

## 构建与运行

### 构建环境要求

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
# 运行已构建的二进制文件
./target/release/decrypt_ncm <ncm_file_path>

# 批量处理目录中的 NCM 文件
./target/release/decrypt_ncm <directory_path>

# 显式允许替换已有输出文件
./target/release/decrypt_ncm --overwrite <file_or_directory>

# 限制目录批量转换的并发数
./target/release/decrypt_ncm --jobs 2 <directory_path>
```

`--jobs` 必须为正整数，默认同时处理最多 2 个文件。

### 文件保护

- 默认不覆盖已有输出文件；需要替换时使用 `--overwrite`。
- 输出格式必须为 MP3 或 FLAC，并且与解密后的音频内容一致。

### 元数据与异常输入

- 缺少歌曲名时使用输入文件名；缺少封面、专辑或艺术家信息时跳过相应标签。
- 缺少格式声明时从音频内容识别 MP3 或 FLAC；无法识别或声明与内容不符时报错。

## 许可证

MIT License
