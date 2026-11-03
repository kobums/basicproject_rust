<div align="center">

# 🦀 basicproject_rust

**axum + sqlx 기반 Rust REST API 시작 템플릿 + gomachine 코드 생성**

[![Rust](https://img.shields.io/badge/Rust-2024_edition-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![axum](https://img.shields.io/badge/axum-0.8-6E4A7E?style=for-the-badge&logo=rust&logoColor=white)](https://github.com/tokio-rs/axum)
[![sqlx](https://img.shields.io/badge/sqlx-0.9-CE422B?style=for-the-badge&logo=rust&logoColor=white)](https://github.com/launchbadge/sqlx)
[![MariaDB](https://img.shields.io/badge/MariaDB-11.4-003545?style=for-the-badge&logo=mariadb&logoColor=white)](https://mariadb.org/)
[![Docker](https://img.shields.io/badge/Docker-Ready-2496ED?style=for-the-badge&logo=docker&logoColor=white)](https://www.docker.com/)

</div>

---

## 📖 소개

**basicproject_rust** 는 새 Rust 백엔드를 바로 시작하기 위한 템플릿이다. [basicproject_go](https://github.com/kobums/basicproject_go) 의 Rust 판이다.
"이 템플릿 + 표준 DB(`db/schema.sql`) + [gomachine](https://github.com/kobums/gomachine)" 으로 초기 구성이 끝난다.

- 템플릿은 **인증 / 인프라만** 가진다: 로그인 · 회원가입 · 내 정보, 이미지 업로드, health, DB 풀, 설정, CORS, 로그.
- **테이블 CRUD 는 모두 gomachine 의 Rust 생성기가 만든다**: `src/models/{table}.rs`, `src/handlers/{table}.rs`, `src/routes.rs`, `src/support/*`.
  표준 스키마(user / board)로 만든 파일이 커밋되어 있어 DB 없이도 바로 빌드된다.
- HTTP 계약(경로, 파라미터, 응답 key, 권한 규칙)은 Go 백엔드와 같다. 토큰은 Go / Spring / Rust 가 같이 쓰는 **통일 JWT** 라서
  프론트엔드(basicproject_web / basicproject_vue)는 어느 백엔드에도 그대로 붙는다.
- gomachine 의 `buildtool-model init` (워크스페이스 모드)이 Rust 백엔드를 만들 때 이 템플릿을 복사해서 쓴다. 그래서 `model.json`,
  `db/schema.sql`, `.env.example` 이름은 gomachine `bootstrap/` 표준 자산과 같게 유지하고, 생성 파일은 gomachine 출력 그대로 커밋한다.

---

## ✨ 주요 기능

| 기능 | 설명 |
|:---|:---|
| ⚙️ **코드 자동 생성** | `model.json` + DB 스키마로 모델 / 핸들러 / 경로 생성 (`make gen`) |
| 🔐 **JWT 인증** | HS256 통일 JWT, bcrypt(`$2a$` cost 10, Go 와 같은 해시), `Option<Session>` 추출기로 생성 코드에 전달 |
| 🔒 **설정** | 환경변수 우선 (`.env` → 환경변수), JWT_SECRET 길이 검사, production 모드 |
| 🐬 **DB** | MariaDB 11.4 (docker compose), sqlx 풀 하나 (서버 시간대 유지: Go 와 같은 TIMESTAMP 값) |
| 🐳 **Docker** | multi-stage 빌드 (rust → debian-slim), 비 root 실행, compose 로 db + api |
| 🖼️ **업로드** | `POST /api/upload/index` 이미지 업로드 (확장자 + 매직 바이트 검사, 5MB), `/uploads` 정적 서빙 |
| ✅ **테스트** | DB 없이 JWT / 라우터 / 읽기 정책 / 인증 API / 업로드 / CORS 테스트 (`cargo test`) |

---

## ⚡ 빠른 시작

준비물: Rust 1.94+ (`rustup`), Docker (DB), [gomachine](https://github.com/kobums/gomachine) (`make gen` 할 때만).

```bash
# 1. 설정 파일
cp .env.example .env              # JWT_SECRET 은 openssl rand -base64 48 로 바꾼다

# 2. DB (MariaDB, db/schema.sql 로 user_tb / board_tb 생성)
make db-up

# 3. 코드 생성 (선택: 표준 스키마로 만든 파일은 이미 커밋되어 있다)
make gen

# 4. 실행 → http://localhost:8008
make run
```

확인:

```bash
curl -s localhost:8008/api/health                       # {"status":"ok"}

# 회원가입 (level 1) → 토큰
curl -s -X POST localhost:8008/api/auth/signup -H 'Content-Type: application/json' \
  -d '{"email":"a@b.c","name":"홍길동","password":"password123"}'

# 로그인
TOKEN=$(curl -s -X POST localhost:8008/api/auth/login -H 'Content-Type: application/json' \
  -d '{"email":"a@b.c","password":"password123"}' | sed 's/.*"token":"\([^"]*\)".*/\1/')

curl -s localhost:8008/api/auth/me -H "Authorization: Bearer $TOKEN"

# 생성된 board API (작성자 = 로그인 사용자)
curl -s -X POST localhost:8008/api/board -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{"title":"첫 글","content":"내용"}'
curl -s 'localhost:8008/api/board?page=1&pagesize=10' -H "Authorization: Bearer $TOKEN"
```

> 포트: Go 템플릿은 8007, 이 템플릿은 **8008** 이다 (같이 띄울 수 있게). 프론트엔드 템플릿의 vite 프록시는 8007 을 보므로
> Rust 백엔드에 붙일 때는 프록시 대상을 `http://localhost:8008` 로 바꾸거나 `PORT=8007` 로 띄운다.
> gomachine 이 `main.rs` 를 새로 만들 때의 기본 포트는 8080 이다 (이 템플릿은 직접 관리하는 `main.rs` 를 쓴다).

---

## 🏗️ 프로젝트 구조

```
basicproject_rust/
├── Cargo.toml / Cargo.lock      # gomachine README 의존성 + dotenvy, getrandom, axum multipart, tower-http fs (+ 테스트용 tower, http-body-util)
├── model.json                   # gomachine 설정 (테이블, 인증, 권한)
├── db/schema.sql                # 표준 스키마 (gomachine bootstrap/db/schema.sql 과 같다)
├── docker-compose.yml           # db (MariaDB) + api (profile)
├── Dockerfile                   # multi-stage 빌드
├── .env.example                 # 환경변수 예 (cp .env.example .env)
├── rustfmt.toml                 # cargo fmt 형식 (gomachine 도 생성 파일을 이 설정으로 rustfmt 한다)
└── src/
    ├── main.rs                  # 엔트리포인트, app() 라우터, CORS, graceful shutdown   (손으로 관리)
    ├── config.rs                # 환경변수 해석 (DB URL, PORT, CORS, JWT_SECRET 검사)   (손으로 관리)
    ├── auth_api.rs              # 로그인 / 회원가입 / me                                (손으로 관리)
    ├── upload.rs                # 이미지 업로드 (POST /api/upload/index)               (손으로 관리)
    ├── app_tests.rs             # DB 없는 앱 테스트                                     (손으로 관리)
    ├── routes.rs                # 모든 테이블 경로 merge                                (gomachine 생성)
    ├── models/{table}.rs        # 테이블 모델 + SQL                                     (gomachine 생성)
    ├── handlers/{table}.rs      # REST 핸들러 + 경로                                    (gomachine 생성)
    └── support/
        ├── auth.rs              # 통일 JWT → Session, make_token                       (사용자 파일: gomachine 이 한 번만 만든다)
        └── *.rs                 # query / response / session / state / password ...   (gomachine 생성)
```

- **gomachine 생성 파일** 은 첫 줄이 `// @generated by gomachine - do not edit` 이다. 직접 고치지 말고 `model.json` / 스키마를 바꾼 뒤
  `make gen` 한다 (내용이 같으면 파일을 다시 쓰지 않는다).
- `src/main.rs`, `src/support/auth.rs` 는 gomachine 이 **없을 때만** 만드는 사용자 파일이다. 이 템플릿은 둘 다 가지고 있어서
  `make gen` 이 건드리지 않는다. `auth.rs` 는 gomachine 기본 `auth.rs` (`JWT_EXPIRATION_MS`, `MIN_SECRET_LEN` 32 바이트 / `check_secret`)
  와 같은 내용이다 (gomachine 이 기본 auth.rs 를 바꿔도 이미 있는 파일은 바뀌지 않으므로 필요하면 손으로 옮긴다).
- gomachine 은 `rustfmt` 가 PATH 에 있으면 생성 파일을 이 crate 의 `rustfmt.toml` 로 정리해서 쓴다. 그래서 생성 파일도
  `cargo fmt --check` 를 통과하고 `cargo fmt` 해도 바뀌지 않는다. **`make gen` 은 rustfmt 가 있는 환경(rustup 의 rustfmt 컴포넌트)에서 한다**
  (없으면 gomachine 이 경고를 찍고 형식 없이 써서 `make lint` 가 실패한다).
- `src/models/`, `src/handlers/` 에 직접 파일을 만들 때는 머리줄을 넣지 않는다 (머리줄이 있고 테이블이 없는 파일은 `make gen` 이 지운다).

---

## 🔐 인증 흐름

1. `POST /api/auth/signup` 또는 `POST /api/auth/login` 이 토큰을 돌려준다.
2. 클라이언트는 이후 요청에 `Authorization: Bearer <token>` 을 붙인다.
3. 생성 핸들러는 `Option<Session>` 추출기(`support/session.rs` → `support/auth.rs`)로 토큰을 검증한다 (없거나 무효면 `None`).
4. 생성 코드는 세션이 없으면 `{"code": "auth error"}` 를 돌려준다. 읽기는 model.json `read` 정책을 따른다:
   board 는 `read: "login"` 이라 로그인 사용자는 모든 글을 읽고, 쓰기는 작성자 컬럼을 `session.id` 로 고정하고 남의 글 수정 / 삭제를 거부한다.
   user 는 본인 행만 읽고(목록 / 개수 / 검색에 `id = session.id`, 남의 단건은 `auth error`) 관리자는 모두 읽는다.
   `level >= adminLevel`(3) 이면 관리자다.

### 인증 API (gomachine README "인증 API 계약", basicproject_go 와 같은 응답)

| 요청 | 설명 | 응답 |
|:---|:---|:---|
| `POST /api/auth/signup` `{email, name, password}` | 회원가입 (level 1, 비밀번호 8~72 바이트, name 이 없으면 이메일 앞부분) | 201 `TokenResponse`, 검증 실패 400, 중복 이메일 409 |
| `POST /api/auth/login` `{email, password}` | 로그인 | 200 `TokenResponse`, 실패 401 (빈 값 / 잘못된 본문 400) |
| `GET /api/auth/me` | 내 정보 (토큰 필요) | 200 `user`, 토큰 없음 / 무효 / 지워진 회원 401 |
| `POST /api/upload/index` multipart `upload` | 이미지 업로드 (토큰 필요) | 200 `{filename, url}`, 파일 없음 / 형식 / 크기 400, 토큰 없음 401 |
| `GET /api/health` | 상태 확인 | `{"status": "ok"}` |

`user` = `{id, email, name, level, createddate}` 로 생성된 User 응답과 같은 key 다 (비밀번호 없음, `createddate` 는 `yyyy-MM-dd HH:mm:ss`).

`TokenResponse`:

```json
{ "token": "eyJ...", "tokenType": "Bearer", "expiresInMs": 86400000,
  "user": { "id": 42, "email": "a@b.c", "name": "홍길동", "level": 1, "createddate": "2026-01-01 09:00:00" } }
```

인증 API 의 에러는 `{status, message, fieldErrors, timestamp}` (Spring 템플릿과 같은 모양)이다.
본문 해석 실패 400, 필드 검증 실패 400 `message: "validation failed"` + `fieldErrors: {email, password, name}`,
DB 접속 실패 503 (업로드 API 도 같은 모양). 없는 이메일로 로그인해도 bcrypt 비교를 한 번 해서 응답 시간으로 계정 유무를 알 수 없다.

### 업로드

- `POST /api/upload/index` (로그인 필요), multipart 파일 field `upload`. jpg / jpeg / png / gif / webp 만, 5MB 까지 (요청 본문 전체는 20MB).
  파일 이름 확장자와 내용(매직 바이트)을 모두 검사하고, 저장 이름은 서버가 만든 임의 이름 + 내용 기준 확장자다 (클라이언트 파일 이름은 쓰지 않는다).
- `{UPLOAD_PATH}/uploads/` (기본 `webdata/uploads`, basicproject_go 와 같은 위치)에 저장하고 `GET /uploads/{filename}` 으로 서빙한다.
- 응답 `{"filename": "3f2a...e1.png", "url": "/uploads/3f2a...e1.png"}`. 게시글 이미지(`board.img`)에는 `filename` 이나 `url` 을 넣는다.

### JWT 형식 (통일 JWT)

- 알고리즘 **HS256** 고정, 비밀키 `JWT_SECRET` (**32 바이트 이상**, 짧으면 서버가 시작하지 않는다. production 은 `change-me...` 예시 값도 거부)
- 유효 시간 기본 24시간 (`JWT_EXPIRATION_MS`)
- payload:

```json
{ "sub": "42", "id": 42, "email": "a@b.c", "level": 1, "iat": 1767225600, "exp": 1767312000 }
```

`sub` 는 id 문자열, `iat` / `exp` 는 초 단위 정수다. 읽을 때 `id` 가 없으면 `sub` 를 숫자로 쓰고, `level` 은 없어도 된다.
Go / Spring 이 발급한 토큰을 그대로 받는다 (같은 `JWT_SECRET`). 형식을 바꾸려면 `src/support/auth.rs` 만 고친다.
자세한 내용은 gomachine README 의 [통일 JWT](https://github.com/kobums/gomachine#통일-jwt-go--spring--rust).

### 생성 API (예: board)

| 요청 | 설명 |
|:---|:---|
| `GET /api/board?page=1&pagesize=10&orderby=createddate desc&title=...` | 목록 `{items, total, code}` (page 가 없으면 전체) |
| `GET /api/board/count` | 개수 `{total, count, code}` |
| `GET /api/board/{id}` | 단건 `{item, code}` |
| `GET /api/board/search/{field}?{field}=값` | 같은 값 검색 |
| `POST /api/board`, `POST /api/board/batch` | 생성 (`{id, code}`), batch 는 1000 행까지 |
| `PUT /api/board/{id}`, `PATCH /api/board/{id}` | 보낸 필드만 수정 |
| `DELETE /api/board/{id}`, `DELETE /api/board/batch` `[{"id":1}]` | 삭제 |

- 모두 토큰이 필요하다 (board 읽기는 `read: "login"`). 예외: `POST /api/user` (gomachine 의 공개 회원가입, level 은 보호 컬럼이라
  DB 기본값 1 로 저장되고 토큰은 주지 않는다). 이 템플릿의 회원가입은 `POST /api/auth/signup` 을 쓴다.
- board 응답의 `extra.user` 에 작성자 정보가 들어간다 (`join`, 비밀번호 제외). 비로그인 읽기는 `auth error`.
- 목록의 문자열 필터(`title=...`)는 부분 일치(`like %값%`, `%` `_` 는 문자 그대로)다.
- `user` 의 비밀번호(`passwd`)는 응답 / 필터에서 빠지고, 저장할 때 bcrypt 로 해시한다. `level` 은 관리자만 바꾼다 (`protect`).
- 에러는 HTTP 200 + `code: "error" | "auth error"`, 잘못된 요청은 400 `{"code": "bad request"}` 이다.
- 자세한 규칙은 gomachine README 의 [Rust (axum + sqlx)](https://github.com/kobums/gomachine#rust-axum--sqlx), "HTTP 계약" 을 본다.

---

## ➕ 테이블 추가하기

1. **스키마** — `db/schema.sql` 에 테이블을 추가한다. 이름 규칙: 테이블 `{name}_tb`, 컬럼 `{접두사}_{이름}`,
   `{접두사}_id` (bigint auto_increment), `{접두사}_createddate` (Insert 때 현재 시각).

   ```sql
   CREATE TABLE IF NOT EXISTS `comment_tb` (
     `c_id` bigint NOT NULL AUTO_INCREMENT,
     `c_board` bigint NOT NULL,
     `c_user` bigint NOT NULL,
     `c_content` text NOT NULL,
     `c_createddate` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
     PRIMARY KEY (`c_id`)
   ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
   ```

2. **DB 반영** — 개발 DB 를 다시 만들거나 (`make db-reset`, 데이터 삭제) 직접 실행한다
   (`docker compose exec -T db mariadb -u"$DB_USER" -p"$DB_PASSWORD" "$DB_NAME" < db/schema.sql`).

3. **model.json** — 권한이 필요하면 `table` 에 추가한다 (없으면 로그인만 확인).

   ```json
   { "name": "comment", "session": [{ "key": "id", "column": "user" }] }
   ```

4. **생성** — `make gen` → `src/models/comment.rs`, `src/handlers/comment.rs` 가 생기고 `src/models/mod.rs`,
   `src/handlers/mod.rs`, `src/routes.rs` 에 추가된다. `main.rs` 는 고치지 않는다 (`routes::routes()` 가 모든 테이블을 merge 한다).

5. `make check && make run`

조인(응답의 `extra.{이름}`)은 model.json 의 `"join": [{ "name": "user", "column": "user" }]` 로 켠다 (표준 board 는 작성자 조인).
읽기 정책은 `"read": "owner" | "login" | "public"` (session 이 있으면 기본 `owner`, 없으면 `login`)이다.

---

## 🧩 model.json

```jsonc
{
  "database": "project",            // gomachine 이 읽을 DB 이름 (.env 의 DB_NAME / MARIADB_DATABASE 와 같게)
  "server": "127.0.0.1",            // DB_HOST / DB_USER / DB_PASSWORD 환경변수가 있으면 그것을 쓴다
  "auth": "jwt",                    // 모든 생성 API 가 Session 을 요구한다
  "adminLevel": "3",                // level 이 이 값 이상이면 관리자
  "languages": { "rust": true },
  "rustModelFilePath": ".",         // crate 루트 (Cargo.toml 위치, model.json 기준). 생성 파일은 그 아래 src/
  "table": [
    { "name": "user", "protect": ["level"] },  // level 은 관리자만 변경 (생성 때는 DB 기본값 1). 읽기는 본인 / 관리자만
    {
      "name": "board",
      "session": [{ "key": "id", "column": "user" }], // 작성자 = 로그인 사용자 (수정 / 삭제는 작성자만)
      "join": [{ "name": "user", "column": "user" }], // 응답 extra.user = 작성자
      "read": "login"                                 // 로그인 사용자는 모든 글을 읽는다
    }
  ]
}
```

DB 접속 정보(IP, 비밀번호)는 model.json 에 커밋하지 않는다. `make gen` 이 `.env` 의 `DB_HOST / DB_USER / DB_PASSWORD` 를 넘긴다.
`make gen` 은 `buildtool-model` 을 PATH 에서 찾고, 없으면 `~/bin/buildtool-model` 을 쓴다 (gomachine 에서 `make install`).
템플릿은 `$HOME/bin/buildtool/` 에서 읽는다. 다른 위치면 `make gen BUILDTOOL=/path/to/buildtool-model`.
gomachine 은 `DB_PORT` (없으면 3306) 와 `DB_NAME` 도 읽는다.

---

## 📝 환경 변수

앱은 시작할 때 `.env` 를 읽는다 (이미 설정된 환경변수가 우선). 이름은 basicproject_go 와 같다.

| 변수 | 기본값 | 설명 |
|:---|:---|:---|
| `APP_MODE` | `develop` | `develop` / `production` (docker compose 의 api 는 `production`) |
| `PORT` | `8008` | HTTP 포트 |
| `DATABASE_URL` | | `mysql://user:pass@host:3306/db`. 있으면 아래 `DB_*` 대신 쓴다 |
| `DB_HOST` / `DB_PORT` | `127.0.0.1` / `3306` | DB 주소 |
| `DB_USER` / `DB_PASSWORD` | | DB 계정 (URL 에 넣을 때 자동으로 percent-encoding) |
| `DB_NAME` | `MARIADB_DATABASE` | DB 이름 (필수) |
| `JWT_SECRET` | | HS256 비밀키, **32 바이트 이상 필수** |
| `JWT_EXPIRATION_MS` | `86400000` | 토큰 유효 시간 (24시간) |
| `CORS_ORIGINS` | develop: `http://localhost:5173,http://localhost:3000,http://localhost:9007` | 허용 출처 (쉼표 구분, `*` 는 모두). production 기본값 없음 (CORS 끔) |
| `UPLOAD_PATH` | `webdata` | 업로드 루트. 파일은 `{UPLOAD_PATH}/uploads`, `/uploads/{파일명}` 으로 서빙 (docker compose 는 `upload-data` 볼륨) |
| `RUST_LOG` | `info` | 로그 필터. SQL 로그는 `info,sql=debug` |
| `MARIADB_DATABASE` / `MARIADB_USER` / `MARIADB_PASSWORD` / `MARIADB_ROOT_PASSWORD` | | docker compose `db` 서비스 |

CORS 는 `Authorization` 헤더와 `GET, POST, HEAD, PUT, PATCH, DELETE, OPTIONS` 를 허용한다 (origin 목록이면 credentials 도 허용).

---

## 🛠️ Make 명령

| 명령 | 설명 |
|:---|:---|
| `make gen` | gomachine 으로 테이블 코드 생성 (`buildtool-model . rust`, DB 필요) |
| `make db-up` / `make db-down` / `make db-reset` | MariaDB 시작 / 중지 / 볼륨 삭제 후 다시 만들기 |
| `make run` | `cargo run` |
| `make build` / `make release` | debug 빌드 / `--release --locked` |
| `make test` | `cargo test` |
| `make fmt` / `make lint` | `cargo fmt` / fmt 확인 + `clippy -D warnings` |
| `make check` | `lint` + `test` |
| `make docker` | Docker 이미지 (`IMAGE=... TAG=...`) |
| `make docker-up` / `make docker-down` | compose 로 db + api |

---

## 🐳 Docker

```bash
make docker                       # 이미지 빌드 (설정 / 비밀값은 이미지에 없다)
make docker-up                    # db + api (api 는 db healthcheck 뒤에 시작, .env 를 읽는다)
docker run --env-file .env -e DB_HOST=host.docker.internal -p 8008:8008 basicproject_rust:latest
```

- 컨테이너는 `APP_MODE=production` 이 기본이다. `CORS_ORIGINS` 와 실제 `JWT_SECRET` 을 넣는다.
- 빌드 단계는 `rust:1.95-slim-trixie`, 실행 단계는 `debian:trixie-slim` (비 root `app` 사용자). TLS 는 rustls 라 OpenSSL 이 필요 없다.

---

## ✅ 테스트

```bash
make check       # cargo fmt --check + clippy -D warnings + cargo test
```

DB 없이 돈다 (풀은 `connect_lazy`, 연결하지 않는다).

- `app_tests`: 통일 JWT 왕복 (`make_token` → `Option<Session>` 추출기), Go / Spring 형식 토큰(`sub` 만 있는 토큰, 만료 토큰) 처리,
  발급 토큰의 claim, 라우터 구성(경로 충돌 panic 없음, health), 생성 경로의 400, 비로그인 `GET /api/board`·`/api/user` 의 `auth error`,
  인증 API 의 400 / 401 응답, 업로드 (토큰 없음 401, 파일 없음 / 형식 / 크기 400, 저장 후 `/uploads` 서빙, 경로 탈출 404), CORS preflight
- `auth_api`: 로그인 / 회원가입 입력 검증, `TokenResponse` / 에러 응답 모양 (비밀번호 미포함, `user` key = 생성된 User 응답 key)
- `upload`: 확장자 / 매직 바이트 / 크기 검사, 업로드 디렉터리
- `config`: DB URL 조립 (percent-encoding), PORT, CORS, JWT_SECRET 규칙
- `support/*`: gomachine 이 넣은 단위 테스트 (쿼리 빌더, 파라미터 해석, bcrypt 가 Go 해시와 호환 ...)

---

## 🔀 Go 백엔드(basicproject_go)와 다른 점

- 포트 기본값 8008 (Go 8007).
- 업로드는 gomachine 계약(`{filename, url}`, `/uploads` 서빙)만 있다. `/webdata` 정적 서빙과 SPA 서빙은 없다.
- `.env.yml` 을 읽지 않는다 (환경변수 / `.env` 만). `DB_DSN` 대신 `DATABASE_URL`.
- 생성 API 의 세부 차이(빈 목록 `items: []`, null 허용 컬럼 `null`, 경로 대소문자 / 끝 `/` 구분 등)는
  gomachine README 의 [Rust 절 "Go 와 일부러 다른 점"](https://github.com/kobums/gomachine#rust-axum--sqlx) 을 본다.

---

## 🔒 보안 메모

- `.env` 는 커밋하지 않는다 (`.gitignore`, `.dockerignore`).
- 생성 모델의 조건은 컬럼 / 연산자를 검증하고 값은 바인딩한다 (`support/query.rs`).
- 생성 `GET /api/user` 는 본인 행만 돌려준다 (관리자는 모두). 회원 목록을 아예 열지 않으려면 `routes.rs` 대신
  `main.rs` 에서 user 경로를 빼고 붙인다 (`handlers::board::routes()` 만 merge).
- 업로드는 로그인 사용자만, 이미지(확장자 + 매직 바이트)만, 서버가 만든 이름으로 저장한다 (저장형 XSS / 경로 조작 방지).
