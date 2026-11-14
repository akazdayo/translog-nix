FROM rust:1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --workspace

FROM golang:1.27.3-bookworm AS go-build
WORKDIR /src
COPY transparency-log/go.mod transparency-log/go.sum ./
RUN go mod download
COPY transparency-log/ ./
RUN go build -o /out/transparency . && go build -o /out/sync ./cmd/sync

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=rust-build /src/target/release/translog-proxy /src/target/release/translog-log-server /usr/local/bin/
COPY --from=go-build /out/ /usr/local/bin/
COPY demo/start-log.sh /usr/local/bin/start-log
RUN chmod +x /usr/local/bin/start-log
