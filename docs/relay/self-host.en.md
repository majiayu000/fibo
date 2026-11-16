# Host your own relay

The computer and phone both connect outward to the relay. Terminal content, messages, and approvals are encrypted between those devices; the relay forwards ciphertext. You need a server, a domain pointing to it, and Docker.

## Start the relay

Build the image from this repository:

```sh
git clone https://github.com/majiayu000/fibo
cd fibo
docker build -f crates/fibo-relay/Dockerfile -t fibo-relay .
```

Set `RELAY_REGISTRATION_KEY` in your shell to a long, randomly generated secret, save it securely, then run:

```sh
docker run -d --name fibo-relay --restart unless-stopped \
  --log-opt max-size=20m --log-opt max-file=10 \
  -p 127.0.0.1:8787:8787 \
  -e RELAY_REGISTRATION_KEY -e RELAY_TRUST_PROXY=1 fibo-relay
```

The key controls which computers can register. Phones use their own pairing keys. Keep the relay port on loopback when trusting proxy headers; expose it through HTTPS.

## HTTPS and pairing

A Caddy configuration for your domain:

```caddy
relay.example.com {
    reverse_proxy 127.0.0.1:8787
}
```

Leave access logging disabled: phone URL paths contain private routing codes. If using nginx, enable WebSocket upgrades, set a long read timeout, and disable access logging for this route. See the [full configuration reference](self-host.md).

`curl https://relay.example.com/healthz` should return `"ok":true` and relay `"protocol":3` or higher. This relay protocol is separate from the phone-to-host JSON protocol.

In Fibo, open Settings › Phone & remote, enter the HTTPS relay URL and the registration key, and enable remote access. Once connected, create a pairing QR and scan it with your [source-built phone app](../../mobile/README.md).

## Operations

Rebuild the image to update, then recreate the container with the same registration key and options. Connections pause during replacement and reconnect afterward; the relay stores no application data.

Caddy manages certificate renewal. Monitor HTTPS availability and renewal failures separately: a successful renewal today does not verify future renewals. Keep the server clock correct and the certificate-validation ports reachable.

The relay can see IP addresses, route hashes, connection times, and traffic volume. It cannot decrypt conversation content. Logs rotate with the Docker options above; retain them separately if you need a longer usage history. Default limits are 64 simultaneous connections and 120 new connections per minute per IP; see [the full reference](self-host.md) for limits and usage reports.
