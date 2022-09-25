FROM niklasf/fishnet-builder:3 AS builder
WORKDIR /fishnet
COPY . .
RUN git submodule update --init || true
RUN cargo auditable build --target=x86_64-unknown-linux-musl --release -vv

FROM alpine:3
RUN apk --no-cache add bash
COPY --from=builder /fishnet/target/x86_64-unknown-linux-musl/release/fishnet /fishnet
COPY docker-entrypoint.sh /docker-entrypoint.sh
CMD ["/docker-entrypoint.sh"]
