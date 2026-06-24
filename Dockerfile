FROM rust:1-slim-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/popovic /usr/local/bin/popovic

ENV POPOVIC_HOME=/data \
    POPOVIC_DASHBOARD_ADDR=0.0.0.0:7626 \
    POPOVIC_STATIC_ADDR=0.0.0.0:80 \
    POPOVIC_BOOTSTRAP=1 \
    POPOVIC_BOOTSTRAP_REDEPLOY=1

EXPOSE 7626 80

CMD ["popovic"]
