use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use md5::{Digest, Md5};
use reqwest::{Client, Method};
use serde_json::{Value, json};
use sha1::Sha1;
use std::{
    collections::BTreeMap,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::time::{Instant, timeout_at};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Parser)]
#[command(
    version,
    about = "QQ 机器人：注册用户、发送文本和文件",
    arg_required_else_help = true
)]
struct Cli {
    /// 配置目录（默认 QQ_CONFIG_DIR，其次可执行文件所在目录）
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 等待用户私聊并保存 openid
    Register {
        user_name: String,
        #[arg(long, default_value = "10", value_parser = clap::value_parser!(u64).range(1..))]
        timeout: u64,
    },
    /// 发送消息
    Send {
        #[command(subcommand)]
        kind: Send,
    },
    /// 配置 APP_ID 和 CLIENT_SECRET
    #[command(after_help = "示例:\n  qq config --app-id <APP_ID> --client-secret <CLIENT_SECRET>")]
    Config {
        /// QQ 机器人 APP_ID
        #[arg(long, value_name = "APP_ID")]
        app_id: String,
        /// QQ 机器人 CLIENT_SECRET
        #[arg(long, value_name = "CLIENT_SECRET")]
        client_secret: String,
    },
}

#[derive(Subcommand)]
enum Send {
    Text {
        user_name: String,
        message: String,
    },
    File {
        user_name: String,
        file_path: PathBuf,
    },
}

fn load_users(dir: &Path) -> Result<BTreeMap<String, String>> {
    let path = dir.join("users.json");
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("users.json 格式错误"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e).context("无法读取 users.json"),
    }
}

fn user_id(dir: &Path, name: &str) -> Result<String> {
    load_users(dir)?
        .remove(name)
        .with_context(|| format!("用户 {name} 尚未注册"))
}

fn save_config(dir: &Path, app_id: &str, client_secret: &str) -> Result<()> {
    ensure!(!app_id.trim().is_empty(), "APP_ID 不能为空");
    ensure!(!client_secret.trim().is_empty(), "CLIENT_SECRET 不能为空");
    ensure!(
        !app_id.contains(['\n', '\r', '=']) && !client_secret.contains(['\n', '\r', '=']),
        "APP_ID 和 CLIENT_SECRET 不能包含换行符或等号"
    );

    fs::create_dir_all(dir).context("无法创建配置目录")?;
    let content = format!("APP_ID={app_id}\nCLIENT_SECRET={client_secret}\n");
    fs::write(dir.join(".env"), content).context("无法写入 .env")?;
    Ok(())
}

struct Api {
    client: Client,
    base: String,
    auth: String,
}

impl Api {
    async fn new(dir: &Path) -> Result<Self> {
        let dotenv = dir.join(".env");
        let mut config = BTreeMap::new();
        if dotenv.exists() {
            for item in dotenvy::from_path_iter(dotenv)? {
                let (key, value) = item?;
                config.insert(key, value);
            }
        }
        let get = |name: &str| {
            env::var(name)
                .ok()
                .or_else(|| config.get(name).cloned())
                .filter(|s| !s.is_empty())
                .with_context(|| {
                    format!(
                        "缺少配置 {name}。请先运行 `qq config --app-id <APP_ID> --client-secret <CLIENT_SECRET>`，\n                         或设置环境变量 {name}"
                    )
                })
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        let base = "https://api.bot.qq.com".to_owned();
        let response = client
            .post(format!("{base}/app/getAppAccessToken"))
            .json(&json!({"appId": get("APP_ID")?, "clientSecret": get("CLIENT_SECRET")?}))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;
        let token = response["access_token"]
            .as_str()
            .context("鉴权响应缺少 access_token")?;
        Ok(Self {
            client,
            base,
            auth: format!("QQBot {token}"),
        })
    }

    async fn request(&self, method: Method, route: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self
            .client
            .request(method, format!("{}{route}", self.base))
            .header("Authorization", &self.auth);
        if let Some(value) = body {
            req = req.json(&value);
        }
        let response = req.send().await?.error_for_status()?;
        let bytes = response.bytes().await?;
        if bytes.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(serde_json::from_slice(&bytes)?)
        }
    }

    async fn wait_for_user(&self, seconds: u64) -> Result<String> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        timeout_at(deadline, async {
            let gateway = self.request(Method::GET, "/gateway", None).await?;
            let (mut socket, _) = connect_async(gateway["url"].as_str().context("缺少 gateway URL")?).await?;
            let hello: Value = serde_json::from_str(&socket.next().await.context("Gateway 已关闭")??.into_text()?)?;
            ensure!(hello["op"] == 10, "Gateway 首包不是 Hello");
            let millis = hello["d"]["heartbeat_interval"].as_u64().filter(|v| *v > 0).context("无效心跳周期")?;
            let interval = Duration::from_millis(millis);
            socket.send(Message::text(json!({"op":2,"d":{"token":self.auth,"intents":1<<25,"shard":[0,1],"properties":{}}}).to_string())).await?;
            let mut sequence = Value::Null;
            let mut next_heartbeat = Instant::now() + interval;
            println!("等待用户发送消息，超时时间 {seconds} 秒...");
            loop {
                tokio::select! {
                    _ = tokio::time::sleep_until(next_heartbeat) => {
                        socket.send(Message::text(json!({"op":1,"d":sequence}).to_string())).await?;
                        next_heartbeat = Instant::now() + interval;
                    }
                    message = socket.next() => {
                        let message = message.context("Gateway 已关闭")??;
                        let text = match message {
                            Message::Text(text) => text,
                            Message::Ping(data) => { socket.send(Message::Pong(data)).await?; continue; }
                            Message::Close(_) => bail!("Gateway 已关闭"),
                            _ => continue,
                        };
                        let payload: Value = serde_json::from_str(&text)?;
                        if !payload["s"].is_null() { sequence = payload["s"].clone(); }
                        match payload["op"].as_u64() {
                            Some(1) => {
                                socket.send(Message::text(json!({"op":1,"d":sequence}).to_string())).await?;
                                next_heartbeat = Instant::now() + interval;
                            }
                            Some(7) => bail!("Gateway 要求重新连接，请重新执行注册"),
                            Some(9) => bail!("Gateway 鉴权或会话无效"),
                            Some(0) if payload["t"] == "C2C_MESSAGE_CREATE" => {
                                return Ok(payload["d"]["author"]["user_openid"].as_str()
                                    .filter(|v| !v.is_empty()).context("消息缺少 user_openid")?.to_owned());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }).await.context("等待用户发送消息超时")?
    }

    async fn send_file(&self, id: &str, path: &Path) -> Result<Value> {
        let info = file_info(path)?;
        let mut prepare = info.clone();
        prepare["file_type"] = json!(4);
        let upload = self
            .request(
                Method::POST,
                &format!("/v2/users/{id}/upload_prepare"),
                Some(prepare),
            )
            .await?;
        let upload_id = upload
            .get("upload_id")
            .filter(|v| !v.is_null())
            .context("缺少 upload_id")?;
        let mut file = fs::File::open(path)?;
        for part in upload["parts"].as_array().context("缺少上传分片列表")? {
            let size = part["block_size"]
                .as_u64()
                .or_else(|| part["block_size"].as_str()?.parse().ok())
                .context("无效分片大小")?;
            ensure!(size > 0, "无效分片大小");
            let mut block = Vec::new();
            Read::by_ref(&mut file).take(size).read_to_end(&mut block)?;
            ensure!(!block.is_empty(), "上传分片数量与文件大小不一致");
            let uploaded_size = block.len();
            let md5 = format!("{:x}", Md5::digest(&block));
            self.client
                .put(part["presigned_url"].as_str().context("缺少上传 URL")?)
                .body(block)
                .send()
                .await?
                .error_for_status()?;
            self.request(Method::POST, &format!("/v2/users/{id}/upload_part_finish"),
                Some(json!({"upload_id":upload_id,"part_index":part.get("index").context("缺少分片 index")?,"block_size":uploaded_size.to_string(),"md5":md5}))).await?;
        }
        ensure!(
            file.read(&mut [0u8; 1])? == 0,
            "上传分片数量与文件大小不一致"
        );
        self.request(Method::POST, &format!("/v2/users/{id}/files"),
            Some(json!({"file_type":4,"srv_send_msg":true,"file_name":info["file_name"],"upload_id":upload_id}))).await
    }
}

fn file_info(path: &Path) -> Result<Value> {
    let mut file = fs::File::open(path).context("无法打开待发送文件")?;
    ensure!(file.metadata()?.is_file(), "待发送路径不是文件");
    let (mut md5, mut sha1, mut prefix) = (Md5::new(), Sha1::new(), Md5::new());
    let mut size = 0u64;
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let len = file.read(&mut buffer)?;
        if len == 0 {
            break;
        }
        md5.update(&buffer[..len]);
        sha1.update(&buffer[..len]);
        let remaining = 10_002_432u64.saturating_sub(size) as usize;
        prefix.update(&buffer[..len.min(remaining)]);
        size += len as u64;
    }
    Ok(
        json!({"file_size":size.to_string(),"file_name":path.file_name().and_then(|s| s.to_str()).context("无效文件名")?,
        "md5":format!("{:x}",md5.finalize()),"sha1":format!("{:x}",sha1.finalize()),"md5_10m":format!("{:x}",prefix.finalize())}),
    )
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("错误: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let dir = cli
        .config_dir
        .or_else(|| env::var_os("QQ_CONFIG_DIR").map(PathBuf::from))
        .unwrap_or(
            env::current_exe()?
                .parent()
                .context("无法定位程序目录")?
                .to_path_buf(),
        );
    match cli.command {
        Command::Register { user_name, timeout } => {
            ensure!(
                !user_name.trim().is_empty() && !user_name.contains(['\n', '\r']),
                "用户名不能为空或包含换行"
            );
            load_users(&dir)?;
            let id = Api::new(&dir).await?.wait_for_user(timeout).await?;
            let mut users = load_users(&dir)?;
            users.insert(user_name.clone(), id);
            fs::create_dir_all(&dir)?;
            fs::write(
                dir.join("users.json"),
                serde_json::to_string_pretty(&users)? + "\n",
            )?;
            println!("已注册用户 {user_name}");
        }
        Command::Config {
            app_id,
            client_secret,
        } => {
            save_config(&dir, &app_id, &client_secret)?;
            println!("配置已保存到 {}", dir.join(".env").display());
        }
        Command::Send { kind } => {
            let name = match &kind {
                Send::Text { user_name, .. } | Send::File { user_name, .. } => user_name,
            };
            let id = user_id(&dir, name)?;
            ensure!(
                !id.is_empty()
                    && id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                "无效 user_openid"
            );
            if let Send::File { file_path, .. } = &kind {
                ensure!(file_path.is_file(), "文件不存在或不是普通文件");
            }
            let api = Api::new(&dir).await?;
            match kind {
                Send::Text { message, .. } => {
                    api.request(
                        Method::POST,
                        &format!("/v2/users/{id}/messages"),
                        Some(json!({"content":message,"msg_type":0})),
                    )
                    .await?;
                }
                Send::File { file_path, .. } => {
                    api.send_file(&id, &file_path).await?;
                }
            }
            println!("发送成功");
        }
    }
    Ok(())
}
