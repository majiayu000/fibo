# Host your own relay

The relay lets your phone reach your computer from any network. The computer and the phone both connect outward to the relay, which only forwards traffic. Everything between them is end-to-end encrypted; the relay can't decrypt it.

You need three things: a machine with a public IP, a domain pointing at it, and Docker.

## 1. Start the relay

```sh
git clone https://github.com/majiayu000/fibo && cd fibo
docker build -f crates/fibo-relay/Dockerfile -t fibo-relay .
docker run -d --name fibo-relay --restart unless-stopped \
  --log-opt max-size=20m --log-opt max-file=10 \
  -p 127.0.0.1:8787:8787 \
  -e RELAY_REGISTRATION_KEY="$(openssl rand -hex 24)" \
  -e RELAY_TRUST_PROXY=1 \
  fibo-relay
docker exec fibo-relay printenv RELAY_REGISTRATION_KEY   # note this key; step 3 needs it
```

- The port only listens on `127.0.0.1`; the HTTPS proxy from step 2 brings traffic in from outside. `RELAY_TRUST_PROXY=1` makes the relay rate-limit by the real client IP the proxy passes along. Don't set it if the port is exposed directly to the internet, or anyone can forge an IP and get around the limits.
- The registration key decides which computers can attach to this relay. Phones don't need it; they use their own pairing keys. Without a key anyone can attach, which is only fit for trying it out on a private network.
- Without Docker: `cargo build --release -p fibo-relay`, then run `target/release/fibo-relay --listen 127.0.0.1:8787 --key <key>`. CI also builds a Linux x86_64 binary on every run, under `fibo-relay-linux-x86_64` on the Actions page.

## 2. Add HTTPS

The relay itself only speaks plain HTTP and WebSocket; the proxy in front of it handles certificates.

Caddy (requests certificates and forwards WebSocket on its own):

```caddy
relay.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

With nginx you have to write the WebSocket upgrade yourself and raise the read timeout, since computers and phones stay connected for a long time:

```nginx
location / {
    proxy_pass http://127.0.0.1:8787;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header X-Real-IP $remote_addr;
    proxy_read_timeout 1h;
    access_log off;
}
```

**Don't let the proxy log phone addresses.** The path a phone connects to, `/c/<routing code>/…`, contains its routing code. If the access log records it, anyone who can read the log can keep taking up this computer's connection slots (they still can't read anything sent). For nginx, put `access_log off;` in this `location`; Caddy writes no access log by default, so remove `log` if you ever turned it on. The `/agent` path computers connect to carries no credentials since 11-09.

Check: `curl https://relay.example.com/healthz` should return `"ok":true` and `"protocol":3` or higher. This relay protocol is separate from the JSON protocol between the phone and the host.

Caddy renews certificates itself. Still watch HTTPS availability and renewal failures on their own: one successful renewal says nothing about the next. Keep the server clock correct and the certificate-validation ports reachable.

## 3. Fill in two values in Fibo

Open **Settings › Phone & remote**:

| Setting | Value |
|---|---|
| Relay address | `https://relay.example.com` |
| Relay secret | the registration key noted in step 1 |

Once the status shows **Relay connected**, click **Pair a new phone** and scan the QR code with the [phone app built from source](../../mobile/README.md). From the command line: `fibo remote --enable --relay https://relay.example.com --relay-key <key>`.

## Upgrading

```sh
cd fibo && git pull
docker build -f crates/fibo-relay/Dockerfile -t fibo-relay .
docker rm -f fibo-relay   # then start it with the same docker run as step 1, using the same key
```

The relay stores nothing. After a restart, computers reconnect on their own and paired phones are unaffected.

If you deployed from this guide before 11-13, the container and image are called `mooring-relay`: remove the old container with `docker rm -f mooring-relay`, or the new one won't start because the port is taken.

This version of Fibo needs a relay speaking protocol 3 or newer. With an older relay, the app shows that the relay version is too old (protocol N, this Fibo needs 3 or newer) and stops retrying. After upgrading the relay, turn **Connect a phone** off and on again in Settings. `docker exec fibo-relay fibo-relay --version` prints the relay's version and protocol.

## What the relay can see

- It can't see: terminal screens, what you type, or approvals. These are encrypted between the computer and the phone; if the handshake fails, the connection is dropped rather than falling back to plain text.
- It can see: the IPs of computers and phones, a hash of each phone's routing code, connection times and traffic volume.

## Default limits

- Each IP can hold at most 64 connections at once and open at most 120 new ones per minute; beyond that the relay returns 429. Change them with the environment variables `RELAY_LIMIT_OPEN` and `RELAY_LIMIT_PER_MINUTE`; 0 means no limit.
- One relay holds at most 10,000 computers, each with at most 256 phone connections at once.
- A phone more than 16 MB behind is disconnected; when it reconnects, it picks up from the latest screen.

The last two live in `Config` in `crates/fibo-relay/src/server.rs`; changing them means editing the source and rebuilding. There are no traffic quotas yet: for a relay open to everyone, use the registration key to control who can attach.

## Traffic per computer

Once an hour the relay writes one log line per computer that had traffic that hour, without any content:

```
usage machine=3f9a1c0b7d2e from_machine=1834221 to_machine=402117
```

`machine` is the first 12 characters of the hash of that computer's token; `from_machine` / `to_machine` are the bytes the computer sent / received that hour, its phones' traffic included. The server's actual traffic in and out is roughly twice their sum. The relay keeps no history; to total it by day or month, add up the log:

```sh
docker logs fibo-relay 2>/dev/null | awk -F'[ =]' '/^usage /{b[$3]+=$5+$7} END{for(m in b) printf "%s %.1f MB\n", m, b[m]/1e6}'
```

The log keeps growing, so pass `--log-opt max-size=20m --log-opt max-file=10` to `docker run` to rotate it (each line is about 80 bytes; with 100 computers attached that holds a little over two years of usage lines). Deleting and recreating the container also deletes the old logs, so if you want a long-term record, save the totals above regularly.

`/healthz` doesn't say how many computers are attached: anyone can reach it, and that number is only for whoever runs the relay.
