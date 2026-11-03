# .env 값(DB_HOST / DB_USER / DB_PASSWORD ...)을 make 와 하위 명령(gomachine, docker compose, cargo)에 넘긴다.
-include .env
export

APP       := basicproject_rust
TAG       ?= latest
IMAGE     ?= $(APP)
# gomachine (make install 하면 ~/bin/buildtool-model, 템플릿은 ~/bin/buildtool/)
BUILDTOOL ?= $(shell command -v buildtool-model 2>/dev/null || echo $(HOME)/bin/buildtool-model)
COMPOSE   ?= docker compose

.PHONY: all gen db-up db-down db-reset run build release test fmt lint check docker docker-up docker-down clean

all: build

# DB(information_schema)를 읽어 model.json 의 테이블 코드를 다시 만든다 (src/models, src/handlers, src/routes.rs, src/support).
# make db-up 이 먼저 필요하다. src/main.rs, src/support/auth.rs 는 있으면 건드리지 않는다.
gen:
	$(BUILDTOOL) . rust

db-up:
	$(COMPOSE) up -d db

db-down:
	$(COMPOSE) down

# 볼륨까지 지우고 db/schema.sql 로 다시 만든다.
db-reset:
	$(COMPOSE) down -v
	$(COMPOSE) up -d db

run:
	cargo run

build:
	cargo build

release:
	cargo build --release --locked

test:
	cargo test

fmt:
	cargo fmt

# fmt 확인 + clippy (경고도 실패)
lint:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings

check: lint test

docker:
	docker build -t $(IMAGE):$(TAG) .

# db + api 컨테이너 (docker-compose.yml 의 api profile)
docker-up:
	$(COMPOSE) --profile api up -d --build

docker-down:
	$(COMPOSE) --profile api down

clean:
	cargo clean
