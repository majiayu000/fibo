# The relay on its own, for anyone hosting one. Build from the repository root:
#   docker build -f crates/fibo-relay/Dockerfile -t fibo-relay .
# Run it behind an HTTPS proxy; see docs/relay/self-host.md.
FROM rust:1.99.1-slim-bookworm AS build
WORKDIR /src
COPY Cargo.lock ./
COPY crates/fibo-remote crates/fibo-remote
COPY crates/fibo-relay crates/fibo-relay
# Only the two crates the relay needs; the rest of the workspace is the desktop app.
RUN printf '[workspace]\nresolver = "2"\nmembers = ["crates/fibo-remote", "crates/fibo-relay"]\n' > Cargo.toml \
 && cargo build --release -p fibo-relay \
 && strip target/release/fibo-relay

FROM debian:bookworm-slim
RUN useradd --system --no-create-home relay
COPY --from=build /src/target/release/fibo-relay /usr/local/bin/fibo-relay
# Its name before the rename, for a compose file or service that still runs that one.
RUN ln -s fibo-relay /usr/local/bin/mooring-relay
USER relay
ENV RELAY_LISTEN=0.0.0.0:8787
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=3s CMD ["fibo-relay", "--check"]
ENTRYPOINT ["fibo-relay"]
