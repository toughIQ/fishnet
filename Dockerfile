FROM docker.io/niklasf/fishnet-builder:4 AS builder
WORKDIR /fishnet
COPY . .
RUN cargo auditable build --release -vv

FROM docker.io/alpine:3
RUN apk --no-cache add bash
COPY --from=builder /fishnet/target/x86_64-unknown-linux-musl/release/fishnet /fishnet
COPY docker-entrypoint.sh /docker-entrypoint.sh
CMD ["/docker-entrypoint.sh"]
