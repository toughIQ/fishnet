FROM rust:1.64 AS builder
WORKDIR /fishnet
RUN cargo install cargo-auditable
COPY . .
RUN (git submodule update --init --recursive || true) \
    && ( \
        if [ -e sde-external-9.0.0-2021-11-07-lin/sde64 ]; then \
            env SDE_PATH="$PWD/sde-external-9.0.0-2021-11-07-lin/sde64" cargo auditable build --release -vv; \
        else \
            cargo auditable build --release -vv; \
        fi \
    )

FROM debian:11-slim
COPY --from=builder /fishnet/target/release/fishnet /fishnet
COPY docker-entrypoint.sh /docker-entrypoint.sh
CMD ["/docker-entrypoint.sh"]
