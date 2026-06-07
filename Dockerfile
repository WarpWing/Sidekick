FROM rust:1-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release


FROM alpine:latest

RUN apk add --no-cache ca-certificates

RUN mkdir /data
VOLUME /data
ENV DATABASE_PATH=/data/sidekick.db

COPY --from=builder /app/target/release/sidekick /usr/local/bin/sidekick

CMD ["sidekick"]
