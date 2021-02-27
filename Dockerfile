FROM rust:1.50 AS builder
WORKDIR /fishnet
COPY . .
RUN (git submodule update --init --recursive || true) && cargo build --release && strip target/release/fishnet

# Not using alpine due to https://andygrove.io/2020/05/why-musl-extremely-slow/
FROM debian:buster-slim
COPY --from=builder /fishnet/target/release/fishnet /fishnet
COPY docker-entrypoint.sh /docker-entrypoint.sh
ENV CORES=auto USER_BACKLOG=0s SYSTEM_BACKLOG=0s ENDPOINT=https://lichess.org/fishnet
CMD ["/docker-entrypoint.sh"]
