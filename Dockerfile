ARG RUST_VERSION=1.37.0

FROM rust:$RUST_VERSION as build

RUN USER=root cargo new --bin app
WORKDIR /app

COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

RUN cargo test --release --verbose --all

RUN cargo build --release --verbose && \
    rm src/*.rs

COPY ./ ./

RUN rm ./target/release/deps/howtocards_ssi* && \
    cargo build --release

FROM debian:9-slim

RUN seq 1 8 | xargs -I{} mkdir -p /usr/share/man/man{} && \
    touch .env

COPY --from=build /app/target/release/howtocards_ssi ./
RUN chmod +x howtocards_ssi

CMD ["/howtocards_ssi"]
