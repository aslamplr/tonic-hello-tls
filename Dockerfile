FROM rust:1.72-slim-buster as builder

WORKDIR /usr/src/app

RUN apt-get update && apt-get install -y \
    build-essential \
    cmake \
    git \
    libssl-dev \
    pkg-config \
    protobuf-compiler libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . .

RUN cargo install --path .

FROM debian:buster-slim

RUN apt-get update && apt-get install -y \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

COPY ./tls ./tls

COPY --from=builder /usr/local/cargo/bin/ /usr/local/bin/

CMD ["tonic-hello-tls"]
