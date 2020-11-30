FROM rust:latest AS builder
WORKDIR /fishnet
COPY . .
RUN git submodule update --init --recursive && cargo build --release && strip target/release/fishnet

# Not using alpine due to https://andygrove.io/2020/05/why-musl-extremely-slow/
FROM debian:buster-slim
COPY --from=builder /fishnet/target/release/fishnet /fishnet
ENV CORES=auto USER_BACKLOG=0s SYSTEM_BACKLOG=0s ENDPOINT=https://lichess.org/fishnet
CMD /fishnet --no-conf --cores "$CORES" --user-backlog "$USER_BACKLOG" --system-backlog "$SYSTEM_BACKLOG" --endpoint "$ENDPOINT" --key "$KEY" run
