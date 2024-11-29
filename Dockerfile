FROM rust:1.83

WORKDIR /usr/src/gmail-prom-exporter-rs
COPY . .

RUN cargo install --path .

CMD ["gmail-prom-exporter-rs"]
