// basicproject_rust 진입점 (직접 관리하는 파일, gomachine 은 이 파일이 있으면 만들지 않는다).
//
// 환경변수 (.env 를 읽는다, 이미 있는 환경변수가 우선. 자세한 내용은 README / .env.example):
//   DATABASE_URL       mysql://user:pass@host:3306/db (없으면 DB_HOST / DB_PORT / DB_USER / DB_PASSWORD / DB_NAME 으로 만든다)
//   PORT               기본 8008
//   JWT_SECRET         통일 JWT 서명 키 (32 바이트 이상, support/auth.rs), JWT_EXPIRATION_MS (기본 86400000)
//   APP_MODE           develop | production
//   CORS_ORIGINS       쉼표로 구분한 허용 origin (비우면 develop 은 localhost:5173,3000,9007, production 은 없음, "*" 는 모두)
//   UPLOAD_PATH        업로드 루트 (기본 webdata, 파일은 {UPLOAD_PATH}/uploads, /uploads 로 서빙)
//   RUST_LOG           기본 info (SQL 로그는 info,sql=debug)

// gomachine 생성 모듈 (support/auth.rs 만 사용자 파일). gomachine 은 rustfmt 가 PATH 에 있으면 이 crate 의
// rustfmt.toml 로 정리해서 쓰므로 cargo fmt --check 를 그대로 통과한다.
mod handlers;
mod models;
mod routes;
mod support;

// 직접 작성한 모듈
mod auth_api;
mod config;
mod upload;

#[cfg(test)]
mod app_tests;

use std::path::PathBuf;

use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, Method, header};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use sqlx::MySqlPool;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use config::Cors;
use support::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // .env 가 있으면 읽는다 (이미 있는 환경변수가 우선).
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let env = |k: &str| std::env::var(k).ok();
    let production = config::is_production(env("APP_MODE").as_deref());
    config::validate_jwt_secret(env("JWT_SECRET").as_deref(), production)?;
    let url = config::database_url(env)?;
    let port = config::port(env("PORT").as_deref())?;
    let cors = config::cors(env("CORS_ORIGINS").as_deref(), production);
    let upload_dir = upload::upload_dir(env("UPLOAD_PATH").as_deref());

    // support::state::connect 는 세션 time_zone 을 바꾸지 않는다 (TIMESTAMP 를 Go 백엔드와 같은 서버 시간대로 읽는다).
    let pool = support::state::connect(&url, 20)
        .await
        .map_err(|e| format!("cannot connect to the database (is it running? make db-up): {e}"))?;

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(production, ?cors, upload_dir = %upload_dir.display(), "listening on {}", listener.local_addr()?);
    axum::serve(listener, app(pool, &cors, upload_dir))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

/// 전체 라우터: /api 아래에 health + 인증 API + 업로드 + gomachine 생성 경로 (basicproject_go router.SetRouter 와 같은 순서),
/// /uploads 는 upload_dir 정적 서빙. 요청 본문은 20MB 까지 (basicproject_go BodyLimit).
pub fn app(pool: MySqlPool, cors: &Cors, upload_dir: PathBuf) -> Router {
    let api = Router::new()
        .route("/health", get(|| async { Json(json!({"status": "ok"})) }))
        .merge(auth_api::routes())
        .merge(upload::routes(upload_dir.clone()))
        .merge(routes::routes())
        .layer(DefaultBodyLimit::max(upload::BODY_LIMIT));
    let router = Router::new()
        .nest("/api", api)
        .nest_service(upload::URL_PREFIX, ServeDir::new(upload_dir))
        .with_state(AppState::new(pool));
    match cors_layer(cors) {
        Some(layer) => router.layer(layer),
        None => router,
    }
}

/// basicproject_go 와 같은 CORS: 생성 경로의 메서드(PATCH 포함)와 Authorization 헤더를 허용한다.
/// origin 목록이면 credentials 도 허용하고, "*" 면 credentials 없이 모든 origin.
fn cors_layer(cors: &Cors) -> Option<CorsLayer> {
    let layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::HEAD,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::ORIGIN,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::CONTENT_LENGTH,
            header::ACCEPT_LANGUAGE,
            header::ACCEPT_ENCODING,
            header::AUTHORIZATION,
            HeaderName::from_static("x-requested-with"),
        ]);
    match cors {
        Cors::Off => None,
        Cors::Any => Some(layer.allow_origin(AllowOrigin::any())),
        Cors::List(origins) => Some(
            layer
                .allow_origin(AllowOrigin::list(origins.iter().cloned()))
                .allow_credentials(true),
        ),
    }
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    tracing::info!("shutting down");
}
