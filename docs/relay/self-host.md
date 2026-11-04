# 自建中继

中继让手机在任何网络下都能连到你的电脑。电脑和手机都主动连向中继，中继只负责转发。内容在两端之间端到端加密，中继解不开。

自建要准备三样东西：一台有公网 IP 的机器、一个指向它的域名、Docker。

## 1. 启动中继

```sh
git clone https://github.com/majiayu000/mooring && cd mooring
docker build -f crates/mooring-relay/Dockerfile -t mooring-relay .
docker run -d --name mooring-relay --restart unless-stopped \
  -p 127.0.0.1:8787:8787 \
  -e RELAY_REGISTRATION_KEY="$(openssl rand -hex 24)" \
  mooring-relay
docker exec mooring-relay printenv RELAY_REGISTRATION_KEY   # 记下这个密钥，第 3 步要填
```

- 端口只开在 `127.0.0.1`，外面由第 2 步的 HTTPS 代理接进来。
- 注册密钥决定哪些电脑能挂到这个中继上，手机不需要它。不设密钥时谁都能挂，只适合内网试用。
- 不想用 Docker 时，`cargo build --release -p mooring-relay`，然后运行 `target/release/mooring-relay --listen 127.0.0.1:8787 --key <密钥>`。CI 每次也会产出一份 Linux x86_64 的二进制，在 Actions 页面的 `mooring-relay-linux-x86_64` 里。

## 2. 加上 HTTPS

中继本身只说明文 HTTP 和 WebSocket，证书由前面的代理负责。

Caddy（会自动申请证书，也会自动转发 WebSocket）：

```
relay.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

nginx 要自己写 WebSocket 升级，并把读超时调长（电脑和手机会长时间连着）：

```
location / {
    proxy_pass http://127.0.0.1:8787;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 1h;
}
```

检查：`curl https://relay.example.com/healthz` 应该返回 `"ok":true`，以及 `"protocol":3` 或更高。

## 3. 在 Mooring 里填两个值

打开 设置与记录 › 手机与远程：

| 设置 | 填什么 |
|---|---|
| 中继地址 | `https://relay.example.com` |
| 中继密钥 | 第 1 步记下的注册密钥 |

状态变成「中继已连上」后，点配对，用手机扫二维码。命令行也可以：`mooring remote --enable --relay https://relay.example.com --relay-key <密钥>`。

## 升级

```sh
cd mooring && git pull
docker build -f crates/mooring-relay/Dockerfile -t mooring-relay .
docker rm -f mooring-relay   # 然后用第 1 步同样的 docker run 启动，填同一个密钥
```

中继不存东西，重启后电脑会自己连回来，已配对的手机不受影响。

这台 Mooring 需要协议 3 以上的中继。中继太旧时，App 会显示「没连上：中继版本太旧（协议 N，这台 Mooring 要 3 以上）：请升级中继」，并且不再重试。升级中继后，在设置里把「让手机连过来」关掉再打开。中继的版本可以用 `docker exec mooring-relay mooring-relay --version` 查看。

## 中继能看到什么

- 看不到：终端画面、你打的字、审批内容。这些都在电脑和手机之间加密，握手不通过就断开，不会退回明文。
- 看得到：电脑和手机的 IP、每台手机的路由码哈希、连接时间和流量大小。

## 默认限制

一台中继最多挂 10,000 台电脑，每台电脑最多同时开 256 个手机连接。手机落后超过 16 MB 时会被断开，重连后从最新画面接着看。这些数值都在 `crates/mooring-relay/src/server.rs` 的 `Config` 里，要改就改源码重新构建。
