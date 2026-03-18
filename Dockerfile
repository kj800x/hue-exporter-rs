# Build Stage
FROM rust:1.93-alpine AS builder
WORKDIR /usr/src/
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static gcc g++ make

WORKDIR /usr/src
RUN USER=root cargo new hue-exporter-rs
WORKDIR /usr/src/hue-exporter-rs
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

COPY src ./src
RUN touch src/main.rs && cargo build --release

# Runtime Stage
FROM alpine:latest AS runtime
WORKDIR /app
COPY --from=builder /usr/src/hue-exporter-rs/target/release/hue-exporter-rs /usr/local/bin/hue-exporter-rs
USER 1000
CMD ["hue-exporter-rs"]
