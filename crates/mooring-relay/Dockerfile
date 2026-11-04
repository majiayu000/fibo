# The relay on its own, for anyone hosting one. Build from the repository root:
#   docker build -f crates/mooring-relay/Dockerfile -t mooring-relay .
# Run it behind an HTTPS proxy; see docs/relay/self-host.md.
FROM rust:1.95-slim-bookworm AS build
WORKDIR /src
COPY Cargo.lock ./
COPY crates/mooring-remote crates/mooring-remote
COPY crates/mooring-relay crates/mooring-relay
# Only the two crates the relay needs; the rest of the workspace is the desktop app.
RUN printf '[workspace]\nresolver = "2"\nmembers = ["crates/mooring-remote", "crates/mooring-relay"]\n' > Cargo.toml \
 && cargo build --release -p mooring-relay \
 && strip target/release/mooring-relay

FROM debian:bookworm-slim
RUN useradd --system --no-create-home relay
COPY --from=build /src/target/release/mooring-relay /usr/local/bin/mooring-relay
USER relay
ENV RELAY_LISTEN=0.0.0.0:8787
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=3s CMD ["mooring-relay", "--check"]
ENTRYPOINT ["mooring-relay"]
