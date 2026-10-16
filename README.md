# QQ Rust CLI

通过 QQ 机器人注册私聊用户、发送文本和本地文件的命令行工具，使用 Rust 实现。

## 功能介绍

- **注册用户**：等待用户给机器人发送私聊消息，将用户名与 `user_openid` 保存到 `users.json`。
- **发送文本**：按已注册用户名发送文本消息。
- **发送文件**：计算文件哈希，按服务端返回的分片信息上传并发送文件。
- **单文件部署**：程序编译为一个可执行文件，无需 Python、Rust 或额外的脚本文件。

## 使用指南

### 1. 直接运行（无需安装）

程序无需安装，下载或编译后即可通过可执行文件的路径直接运行。注意要有执行权限。

Ubuntu 示例：

```bash
/path/to/qqbot-cli-ubuntu-x86_64 --help
```

Windows PowerShell 示例：

```powershell
& "/path/to/qqbot-cli-windows-x86_64.exe" --help
```

如需将程序注册为系统工具，以便在任意目录通过命令名调用，请自行学习所用操作系统中将可执行程序加入系统路径（`PATH`）的方法。

下文使用 `qq` 作为命令简写，直接运行时请替换为对应的可执行文件路径。后续仅提供 Ubuntu 示例。

### 2. 配置机器人

> 请自行在 QQ 开放平台创建机器人并获取 `APP_ID` 和 `CLIENT_SECRET`
程序需要 QQ 机器人的 `APP_ID` 和 `CLIENT_SECRET`，以及访问 QQ API 和 WebSocket Gateway 的网络连接。


在配置目录创建 .env：

```dotenv
APP_ID=your_app_id
CLIENT_SECRET=your_client_secret
```

也可以通过 `config` 子命令写入配置文件：

```bash
qq config --app-id your_app_id --client-secret your_client_secret
```

命令会在配置目录创建或覆盖 .env。也可以通过同名环境变量提供凭据；环境变量优先于 .env。

配置目录按以下顺序选择：

1. 命令行 `--config-dir <目录>`。
2. 环境变量 `QQ_CONFIG_DIR`。
3. 当前可执行文件所在目录。

例如，在本项目目录使用已有配置：

```bash
export QQ_CONFIG_DIR="$PWD"
qq --help
```

也可以在每次调用时显式指定：

```bash
qq --config-dir /path/to/config send text alice '你好'
```

### 3. 注册用户

```bash
qq register alice
qq register '测试 用户' --timeout 60
```

默认等待 **10 秒**，`--timeout` 必须为正整数。启动后，让目标用户在等待时间内向机器人发送一条私聊消息。

程序将收到的第一条私聊消息的发送者绑定到指定用户名；相同用户名再次注册会更新绑定。注册成功后自动创建或更新配置目录中的 `users.json`，因此该目录需要可写。
> 为了保证程序简单性，没做加密

数据格式如下，无需手工预先创建空文件：

```json
{
  "alice": "user_openid_value"
}
```

### 4. 发送文本和文件

```bash
qq send text alice '你好，这是一条测试消息'
qq send text '测试 用户' '支持带空格的用户名'
qq send file alice ./example.zip
qq send file alice '/path/to/带空格的文件.zip'
```

文件相对路径以**调用命令时的工作目录**为基准。用户名、文本或文件路径包含空格时应加引号。

注册和发送需要机器人具备相应 QQ 平台权限，实际发送结果以平台响应为准。

### 5. 查看帮助和版本

```bash
qq --help
qq register --help
qq send text --help
qq send file --help
qq config --help
qq --version
```

查看帮助和版本不需要联网鉴权。

## 编译指导

假设开发者已经安装 Rust 工具链（包含 `rustc` 和 `cargo`），并配置好对应平台的 C/C++ 链接工具。Rust 版本需要支持项目使用的 Rust 2024 edition 和依赖。

在项目根目录（包含 `Cargo.toml` 的目录）执行以下完整编译命令：

```bash
cargo build --locked --release
```

`--locked` 使用 `Cargo.lock` 中锁定的依赖版本，`--release` 生成优化后的发布版程序。首次构建需要联网下载依赖。该命令只编译当前平台的程序，不会运行测试。

### Ubuntu / Linux / Mac

```bash
cd /path/to/qqbot-cli
cargo build --locked --release
```

编译产物为 `target/release/qqbot-cli`。
> Mac 需要安装 Xcode Command Line Tools（`xcode-select --install`）。

### Windows PowerShell

使用 MSVC 工具链时，需要已安装 Visual Studio Build Tools 的 C++ 构建工具及 Windows SDK。

```powershell
Set-Location "C:/path/to/qqbot-cli"
cargo build --locked --release
```

编译产物为 `target/release/qqbot-cli.exe`。

上述命令不会自动将产物复制到 `dist/`。构建后可直接运行产物，或复制到所需目录；运行配置通过 `qq config` 设置，编译时不需要 `APP_ID` 或 `CLIENT_SECRET`。

## Agent Skill

`skill/qqbot-cli/SKILL.md` 提供使用 QQ 机器人发送消息和文件的 skill，包含配置目录选择、收件人注册、发送结果检查和固定版本下载命令。该目录可独立安装和迁移，以其中的 `SKILL.md` 所在目录为根目录，从自身的 `bin/` 运行单文件程序。本地程序统一命名为 `qqbot-cli`（Windows 为 `qqbot-cli.exe`），无需定位原仓库。

## GitHub Actions 与 Release

普通分支推送、Pull Request 和手动触发会构建 Linux x86_64、Windows x86_64、macOS Apple Silicon 三个平台的单文件程序，并上传 Actions 构建产物。

推送版本 tag（例如 `v0.0.0`）还会在全部平台构建成功后创建 GitHub Release，并上传以下可直接下载的附件（重跑时覆盖同名附件）：

- `qqbot-cli-ubuntu-x86_64`
- `qqbot-cli-windows-x86_64.exe`
- `qqbot-cli-macos-aarch64`

提交待发布代码后，使用以下命令发布（确保本地 tag 指向要发布的提交）：

```bash
git tag v0.0.0
git push origin v0.0.0
```

工作流仅给 Release job 授予 `contents: write`，使用内置 `GITHUB_TOKEN` 发布，无需额外的访问令牌；带预发布后缀的 tag（如 `v0.0.1-rc.1`）会创建为预发布版本。skill 下载命令固定使用 `v0.0.0`，需先成功发布该版本，再使用下载命令；后续升级时同步更新文档中的固定版本号。

## 目录结构

```text
qqbot-cli/
├── .github/workflows/build.yml # 三平台构建与 tag Release 工作流
├── Cargo.toml          # Rust 项目与依赖声明
├── Cargo.lock          # 锁定依赖版本
├── src/main.rs         # 程序源码
├── skill/qqbot-cli/     # 独立 Agent Skill，内含自己的 bin/（不提交程序）
└── README.md
```

运行时可在所选配置目录保存 `.env` 和 `users.json`。
