// DB 없이 돌리는 앱 테스트: 통일 JWT, 라우터 구성, 인증 API 입력 검증.
// 풀은 connect_lazy (연결하지 않음) 라서 DB 를 읽기 전에 끝나는 경로만 확인한다.

use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::extract::OptionalFromRequestParts;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::auth_api;
use crate::support::{Session, auth, state};

const TEST_SECRET: &str = "test-secret-for-basicproject-rust-32b";

// auth.rs 는 JWT_SECRET 을 처음 쓸 때 한 번 읽는다. 비밀 키를 쓰는 테스트는 먼저 이 함수를 부른다.
fn init_secret() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: 테스트에서 JWT_SECRET 을 읽기 전에 한 번만 설정한다 (다른 테스트는 이 변수를 읽지 않는다).
        unsafe { std::env::set_var("JWT_SECRET", TEST_SECRET) };
    });
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn app() -> Router {
    let pool = state::connect_lazy("mysql://u:p@127.0.0.1:1/db", 1).unwrap();
    crate::app(pool, &crate::config::Cors::Off)
}

async fn session_for(token: &str) -> Option<Session> {
    let req = Request::builder()
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(())
        .unwrap();
    let (mut parts, _) = req.into_parts();
    <Session as OptionalFromRequestParts<()>>::from_request_parts(&mut parts, &())
        .await
        .unwrap()
}

async fn call(req: Request<Body>) -> (StatusCode, Value) {
    let res = app().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn post_json(uri: &str, body: &str) -> Request<Body> {
    Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn jwt_round_trip() {
    init_secret();
    let token = auth::make_token(42, "a@b.c", Some(1)).expect("JWT_SECRET set");
    let s = session_for(&token).await.expect("valid token");
    assert_eq!((s.id, s.email.as_str(), s.level), (42, "a@b.c", Some(1)));
    // 다른 키로 서명한 토큰 / 깨진 토큰은 비로그인
    let other = auth::token_with_key(42, "a@b.c", Some(1), 60, b"another-secret").unwrap();
    assert!(session_for(&other).await.is_none());
    assert!(session_for("garbage").await.is_none());
}

#[tokio::test]
async fn accepts_go_and_spring_tokens() {
    init_secret();
    let key = EncodingKey::from_secret(TEST_SECRET.as_bytes());
    let t = now();
    // Go / Spring 이 발급하는 통일 JWT
    let go = json!({"sub": "42", "id": 42, "email": "a@b.c", "level": 3, "iat": t, "exp": t + 86_400});
    let s = session_for(&encode(&Header::new(Algorithm::HS256), &go, &key).unwrap())
        .await
        .unwrap();
    assert_eq!((s.id, s.email.as_str(), s.level), (42, "a@b.c", Some(3)));
    // id 가 없으면 sub, level 은 없어도 된다
    let sub_only = json!({"sub": "7", "email": "x@y.z", "iat": t, "exp": t + 60});
    let s = session_for(&encode(&Header::new(Algorithm::HS256), &sub_only, &key).unwrap())
        .await
        .unwrap();
    assert_eq!((s.id, s.level), (7, None));
    // 만료된 토큰은 비로그인
    let expired = json!({"sub": "42", "id": 42, "email": "a@b.c", "iat": t - 7200, "exp": t - 3600});
    assert!(
        session_for(&encode(&Header::new(Algorithm::HS256), &expired, &key).unwrap())
            .await
            .is_none()
    );
}

#[tokio::test]
async fn issued_token_has_unified_claims() {
    init_secret();
    let token = auth::make_token(5, "u@x.y", Some(1)).unwrap();
    let mut v = jsonwebtoken::Validation::new(Algorithm::HS256);
    v.required_spec_claims.clear();
    let claims = jsonwebtoken::decode::<Value>(&token, &jsonwebtoken::DecodingKey::from_secret(TEST_SECRET.as_bytes()), &v)
        .unwrap()
        .claims;
    assert_eq!(
        (&claims["sub"], &claims["id"], &claims["email"], &claims["level"]),
        (&json!("5"), &json!(5), &json!("u@x.y"), &json!(1))
    );
    assert_eq!(
        claims["exp"].as_i64().unwrap() - claims["iat"].as_i64().unwrap(),
        auth::token_ttl_secs()
    );
}

#[tokio::test]
async fn router_builds_and_serves_without_db() {
    let (status, body) = call(Request::get("/api/health").body(Body::empty()).unwrap()).await;
    assert_eq!((status, body), (StatusCode::OK, json!({"status": "ok"})));
    // /api 밖은 404
    let res = app().oneshot(Request::get("/health").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn generated_routes_are_mounted() {
    // 생성 경로: id 를 해석하지 못하면 400 {"code":"bad request"} (Go 생성 라우터와 같다)
    let (status, body) = call(Request::get("/api/user/abc").body(Body::empty()).unwrap()).await;
    assert_eq!((status, body), (StatusCode::BAD_REQUEST, json!({"code": "bad request"})));
    // board 는 session 설정이 있어서 비로그인 목록은 HTTP 200 auth error (DB 를 읽지 않는다)
    let (status, body) = call(Request::get("/api/board").body(Body::empty()).unwrap()).await;
    assert_eq!((status, body), (StatusCode::OK, json!({"code": "auth error"})));
}

fn assert_error(status: StatusCode, v: &Value, want_status: StatusCode, message: &str) {
    assert_eq!(status, want_status, "{v}");
    assert_eq!(v["status"], json!(want_status.as_u16()), "{v}");
    assert_eq!(v["message"], json!(message), "{v}");
    assert!(v["fieldErrors"].is_object(), "{v}");
    assert!(v["timestamp"].is_string(), "{v}");
}

#[tokio::test]
async fn me_without_valid_token() {
    let (status, v) = call(Request::get("/api/auth/me").body(Body::empty()).unwrap()).await;
    assert_error(status, &v, StatusCode::UNAUTHORIZED, auth_api::MSG_AUTH_REQUIRED);
    init_secret();
    let req = Request::get("/api/auth/me")
        .header(header::AUTHORIZATION, "Bearer not-a-token")
        .body(Body::empty())
        .unwrap();
    let (status, v) = call(req).await;
    assert_error(status, &v, StatusCode::UNAUTHORIZED, auth_api::MSG_AUTH_REQUIRED);
}

#[tokio::test]
async fn auth_input_validation() {
    // 본문 해석 실패 / 빈 값: DB 를 읽기 전에 400
    let cases = [
        ("/api/auth/login", "not json", auth_api::MSG_BAD_REQUEST),
        ("/api/auth/login", "[]", auth_api::MSG_BAD_REQUEST),
        ("/api/auth/login", r#"{"email":"a@b.c"}"#, auth_api::MSG_INVALID_LOGIN),
        ("/api/auth/login", r#"{"password":"pw"}"#, auth_api::MSG_INVALID_LOGIN),
        ("/api/auth/signup", "", auth_api::MSG_BAD_REQUEST),
    ];
    for (uri, body, msg) in cases {
        let (status, v) = call(post_json(uri, body)).await;
        assert_error(status, &v, StatusCode::BAD_REQUEST, msg);
        assert_eq!(v["fieldErrors"], json!({}), "{uri} {body}");
    }
    // 필드 검증 실패: fieldErrors
    let (status, v) = call(post_json("/api/auth/signup", r#"{"email":"bad","password":"short","name":"n"}"#)).await;
    assert_error(status, &v, StatusCode::BAD_REQUEST, auth_api::MSG_VALIDATION);
    assert_eq!(
        v["fieldErrors"],
        json!({"email": auth_api::MSG_EMAIL_INVALID, "password": auth_api::MSG_PASSWORD_LEN})
    );
}

#[tokio::test]
async fn cors_preflight_allows_authorization_and_patch() {
    let pool = state::connect_lazy("mysql://u:p@127.0.0.1:1/db", 1).unwrap();
    let cors = crate::config::cors(None, false); // develop 기본 origin
    let app = crate::app(pool, &cors);
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/board/1")
        .header(header::ORIGIN, "http://localhost:5173")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PATCH")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization,content-type")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let h = res.headers();
    assert_eq!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://localhost:5173");
    assert_eq!(h.get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS).unwrap(), "true");
    assert!(
        h.get(header::ACCESS_CONTROL_ALLOW_METHODS)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("PATCH")
    );
    assert!(
        h.get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .unwrap()
            .to_str()
            .unwrap()
            .to_lowercase()
            .contains("authorization")
    );
}

#[tokio::test]
async fn cors_off_in_production_without_origins() {
    let pool = state::connect_lazy("mysql://u:p@127.0.0.1:1/db", 1).unwrap();
    let app = crate::app(pool, &crate::config::cors(None, true));
    let req = Request::get("/api/health")
        .header(header::ORIGIN, "http://evil.example")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert!(res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
}
