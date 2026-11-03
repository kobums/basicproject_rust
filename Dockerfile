# syntax=docker/dockerfile:1
# 빌드: docker build -t basicproject_rust .   (또는 make docker / docker compose --profile api up -d)

FROM rust:1.95-slim-trixie AS builder
WORKDIR /app
# 의존성만 먼저 빌드해서 캐시한다 (소스만 바뀌면 이 단계는 다시 하지 않는다)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src
COPY src ./src
# main.rs 의 수정 시각을 갱신해서 더미 바이너리를 다시 빌드하게 한다
RUN touch src/main.rs && cargo build --release --locked

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 app
COPY --from=builder /app/target/release/basicproject_rust /usr/local/bin/basicproject_rust
USER app
ENV PORT=8008 RUST_LOG=info APP_MODE=production
EXPOSE 8008
CMD ["basicproject_rust"]
