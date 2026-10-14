# decrypt_ncm

## 项目概述

decrypt_ncm 用于解密网易云音乐 NCM 文件，保留原始 MP3 或 FLAC 音频格式。

## 核心功能

- 解密单个 NCM 文件或批量处理目录
- 写入歌曲名称、艺术家、专辑和封面等元数据
- 指定输出目录、覆盖策略和文件并发数

## 构建与运行

### 构建环境要求

- Rust 1.88 或更高版本

### 构建步骤

```bash
# 构建 debug 版本
cargo build --locked

# 构建 release 版本
cargo build --release --locked
```

### 使用方法

```bash
# 运行已构建的二进制文件
./target/release/decrypt_ncm <ncm_file_path>

# 批量处理目录中的 NCM 文件
./target/release/decrypt_ncm <directory_path>

# 将输出保存到指定目录；目录不存在时会自动创建
./target/release/decrypt_ncm --output-dir ./converted <file_or_directory>

# 显式允许替换已有输出文件
./target/release/decrypt_ncm --overwrite <file_or_directory>

# 限制目录批量转换的并发数
./target/release/decrypt_ncm --jobs 2 <directory_path>

# 查看参数说明和版本
./target/release/decrypt_ncm --help
./target/release/decrypt_ncm --version
```

| 参数 | 说明 |
| --- | --- |
| `-o, --output-dir <DIRECTORY>` | 指定输出目录，默认保存到输入文件所在目录 |
| `--overwrite` | 允许替换已有输出文件，默认不覆盖 |
| `-j, --jobs <COUNT>` | 文件并发数，必须为正整数，默认 2 |
| `-h, --help` | 显示帮助信息 |
| `-V, --version` | 显示版本信息 |

### 目录处理与结果

目录扫描仅处理当前层的 `.ncm` 文件，扩展名不区分大小写。
不递归处理子目录。

转换完成后输出成功、失败和跳过数量。跳过数量包含非 NCM 文件、子目录和符号链接；
未指定 `--overwrite` 时，已有输出按失败处理。目录内没有符合条件的文件时会明确提示。

退出码：`0` 表示处理成功（包括没有符合条件的文件），`1` 表示转换或文件操作失败，
`2` 表示命令行参数错误。批处理中单个文件失败不会阻止其他文件继续处理，但最终退出码为 `1`。

输入和输出路径支持空格及操作系统原生路径；路径以 `-` 开头时，可使用 `--` 分隔，例如
`decrypt_ncm -- -song.ncm`。

### 文件保护

- 默认不覆盖已有输出文件；需要替换时使用 `--overwrite`。
- 输出格式必须为 MP3 或 FLAC，并且与解密后的音频内容一致。

### 元数据与异常输入

- 缺少歌曲名时使用输入文件名；缺少封面、专辑或艺术家信息时跳过相应标签。
- 缺少格式声明时从音频内容识别 MP3 或 FLAC；无法识别或声明与内容不符时报错。

## 许可证

MIT License
