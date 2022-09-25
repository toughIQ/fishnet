FROM niklasf/fishnet-builder:2 AS builder
WORKDIR /fishnet
COPY . .
RUN git submodule update --init || true
RUN cargo auditable build --release -vv

FROM alpine:3
RUN apk --no-cache add bash
COPY --from=builder /fishnet/target/release/fishnet /fishnet
COPY docker-entrypoint.sh /docker-entrypoint.sh
CMD ["/docker-entrypoint.sh"]
