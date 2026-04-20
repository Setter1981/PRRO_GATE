################################################################
# Stage 1 — build static binary on Alpine + musl
################################################################
FROM rust:1.82-alpine AS builder

RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /build
COPY rust/Cargo.toml rust/Cargo.lock ./
COPY rust/prro_crypto ./prro_crypto
COPY rust/prro_crypto_v2 ./prro_crypto_v2
COPY rust/prro_sidecar ./prro_sidecar
COPY rust/maria304_driver ./maria304_driver

RUN cargo build --release -p maria304_driver --bin maria304_driver

################################################################
# Stage 2 — minimal runtime image
################################################################
FROM alpine:3.19

RUN apk add --no-cache ca-certificates && \
    adduser -D -u 10001 maria304

WORKDIR /opt/maria304
COPY --from=builder /build/target/release/maria304_driver /opt/maria304/maria304_driver
RUN chmod +x /opt/maria304/maria304_driver

USER maria304
ENV MARIA304_CONFIG=/etc/maria304_driver/config.yaml
ENV MARIA304_LOG=info
EXPOSE 9100 9101 9102 9202

ENTRYPOINT ["/opt/maria304/maria304_driver"]
