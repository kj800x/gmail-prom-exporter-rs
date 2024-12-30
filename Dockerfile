# Build Stage
# https://dev.to/deciduously/use-multi-stage-docker-builds-for-statically-linked-rust-binaries-3jgd
FROM rust:1.83 AS builder
WORKDIR /usr/src/
RUN rustup target add x86_64-unknown-linux-musl
RUN apt-get -y update && \
  apt-get install --no-install-recommends -y \
  musl-tools && \
  rm -rf /var/lib/apt/lists/*

RUN USER=root cargo new gmail-prom-exporter-rs
WORKDIR /usr/src/gmail-prom-exporter-rs
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

COPY src ./src
RUN cargo install --target x86_64-unknown-linux-musl --path .

# Bundle Stage
FROM scratch
COPY --from=builder /usr/local/cargo/bin/gmail-prom-exporter-rs .
USER 1000
CMD ["./gmail-prom-exporter-rs"]
