# Build Stage
FROM rust:1.83-alpine AS builder
WORKDIR /usr/src/
RUN apk add pkgconfig openssl-dev libc-dev

# - Install dependencies
RUN USER=root cargo new gmail-prom-exporter-rs
WORKDIR /usr/src/gmail-prom-exporter-rs
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

# - Copy source
COPY src ./src
RUN touch src/main.rs && cargo build --release

# Runtime Stage
FROM alpine:latest AS runtime
WORKDIR /app
RUN apk update \
  && apk add openssl ca-certificates

COPY --from=builder /usr/src/gmail-prom-exporter-rs/target/release/gmail-prom-exporter-rs ./gmail-prom-exporter-rs
USER 1000
CMD ["./gmail-prom-exporter-rs"]
