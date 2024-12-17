FROM docker.io/niklasf/fishnet-builder:8 AS builder
WORKDIR /fishnet
COPY . .
RUN cargo auditable build --release -vv

FROM docker.io/alpine:3
RUN apk --no-cache add bash
COPY --from=builder /fishnet/target/*-unknown-linux-musl/release/fishnet /fishnet
COPY docker-entrypoint.sh /docker-entrypoint.sh
CMD ["/docker-entrypoint.sh"]
