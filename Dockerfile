FROM ubuntu:20.04 AS builder

# Install build dependencies
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    build-essential \
    curl \
    ca-certificates \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain stable

WORKDIR /app
COPY . /app

# Build release binary
RUN /usr/local/cargo/bin/cargo build --release

FROM ubuntu:20.04 AS runtime
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/baby_sdcs /app/baby_sdcs

EXPOSE 8001 8002 8003

ENV RUST_LOG=info
ENTRYPOINT ["/app/baby_sdcs"]
