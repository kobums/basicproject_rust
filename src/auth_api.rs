// 인증 API (직접 관리하는 파일, gomachine 은 만들지 않는다). basicproject_go router/routers/auth.go 와 같은 계약이다
// (Spring AuthController 와 같은 JSON. 생성 CRUD 의 HTTP 200 + code 규칙과 달리 HTTP 상태 코드를 쓴다).
//
//   POST /api/auth/signup  {email, password, name?}  → 201 TokenResponse   (level 1, bcrypt 해시 저장)
//   POST /api/auth/login   {email, password}         → 200 TokenResponse
//   GET  /api/auth/me      (Authorization: Bearer)   → 200 UserResponse
//
// TokenResponse = {token, tokenType: "Bearer", expiresInMs, user: UserResponse}
// UserResponse  = {id, email, name, level, createdAt}            (비밀번호 없음)
// 에러          = {status, message, fieldErrors: {..}, timestamp} (Spring ErrorResponse)
//   400 본문 해석 실패 / 검증 실패 (fieldErrors), 401 로그인 실패 / 토큰 없음, 404 사용자 없음,
//   409 이메일 중복, 500 해시 / 저장 / 토큰 발급 실패, 503 DB 접속 실패.

use std::collections::BTreeMap;

use axum::extract::{FromRef, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;

use crate::models::user::User;
use crate::support::params::{self, BodyArg};
use crate::support::{Arg, Error, Session, Where, auth, password};

/// 회원가입 사용자의 level (관리자는 model.json adminLevel 3 이상).
pub const SIGNUP_LEVEL: i32 = 1;
/// 비밀번호 길이 (바이트, basicproject_go 와 같다. bcrypt 는 72 바이트까지만 쓴다).
pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_PASSWORD_LEN: usize = password::MAX_PASSWD_BYTES;
pub const MAX_EMAIL_LEN: usize = 255;
pub const MAX_NAME_CHARS: usize = 100;

pub const MSG_BAD_REQUEST: &str = "잘못된 요청입니다.";
pub const MSG_VALIDATION: &str = "validation failed";
pub const MSG_EMAIL_REQUIRED: &str = "이메일을 입력하세요.";
pub const MSG_EMAIL_INVALID: &str = "이메일 형식이 올바르지 않습니다.";
pub const MSG_PASSWORD_LEN: &str = "비밀번호는 8~72자로 입력하세요.";
pub const MSG_NAME_TOO_LONG: &str = "이름은 100자 이하로 입력하세요.";
pub const MSG_INVALID_LOGIN: &str = "이메일 또는 비밀번호가 올바르지 않습니다.";
pub const MSG_DUPLICATE_EMAIL: &str = "이미 사용 중인 이메일입니다.";
pub const MSG_AUTH_REQUIRED: &str = "인증이 필요합니다.";
pub const MSG_USER_NOT_FOUND: &str = "user not found";
pub const MSG_DB_UNAVAILABLE: &str = "데이터베이스에 연결할 수 없습니다.";
pub const MSG_SIGNUP_FAILED: &str = "회원가입에 실패했습니다.";
pub const MSG_PASSWORD_FAILED: &str = "비밀번호 처리 중 오류가 발생했습니다.";
pub const MSG_TOKEN_FAILED: &str = "토큰을 발급할 수 없습니다.";
pub const MSG_SERVER_ERROR: &str = "서버 오류가 발생했습니다.";

// 없는 이메일로 로그인할 때도 bcrypt 비교를 한 번 해서 응답 시간으로 계정 유무를 알 수 없게 한다 (cost 10).
const DUMMY_HASH: &str = "$2a$10$m0HY8Jqr.Oj7.SBKA.OeseqHUFooIrv4N6uM/jVnfbwPUwQJ1bW4i";

pub fn routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    MySqlPool: FromRef<S>,
{
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/login", post(login))
        .route("/auth/me", get(me))
}

// ---- 요청 ----

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LoginBody {
    email: String,
    password: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SignupBody {
    email: String,
    name: String,
    password: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
    pub name: String,
}

// ---- 응답 ----

/// 사용자 정보 (비밀번호 없음). basicproject_go dto.UserResponse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: i64,
    pub email: String,
    pub name: String,
    pub level: i32,
    pub created_at: String,
}

impl From<&User> for UserResponse {
    fn from(u: &User) -> Self {
        UserResponse {
            id: u.id,
            email: u.email.clone(),
            name: u.name.clone(),
            level: u.level,
            created_at: u.createddate.clone(),
        }
    }
}

/// 로그인 / 회원가입 응답. basicproject_go dto.TokenResponse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    pub token: String,
    pub token_type: &'static str,
    pub expires_in_ms: i64,
    pub user: UserResponse,
}

impl TokenResponse {
    pub fn new(user: &User, token: String, ttl_secs: i64) -> Self {
        TokenResponse {
            token,
            token_type: "Bearer",
            expires_in_ms: ttl_secs * 1000,
            user: UserResponse::from(user),
        }
    }
}

/// Spring ErrorResponse 와 같은 에러 본문.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub status: u16,
    pub message: String,
    pub field_errors: BTreeMap<String, String>,
    pub timestamp: String,
}

/// 에러 응답 (fieldErrors 는 빈 객체).
pub fn error_json(status: StatusCode, message: &str) -> Response {
    error_with_fields(status, message, BTreeMap::new())
}

/// 필드 검증 실패: 400 {message: "validation failed", fieldErrors}.
pub fn validation_error(fields: BTreeMap<String, String>) -> Response {
    error_with_fields(StatusCode::BAD_REQUEST, MSG_VALIDATION, fields)
}

fn error_with_fields(status: StatusCode, message: &str, field_errors: BTreeMap<String, String>) -> Response {
    let body = ErrorResponse {
        status: status.as_u16(),
        message: message.to_string(),
        field_errors,
        timestamp: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };
    (status, Json(body)).into_response()
}

/// 본문 검증 실패.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalid {
    /// 본문을 해석하지 못했다 (400 "잘못된 요청입니다.").
    Body,
    /// 필드 값이 잘못됐다 (400 validation failed + fieldErrors).
    Fields(BTreeMap<String, String>),
    /// 그 밖의 400 메시지.
    Message(&'static str),
}

impl IntoResponse for Invalid {
    fn into_response(self) -> Response {
        match self {
            Invalid::Body => error_json(StatusCode::BAD_REQUEST, MSG_BAD_REQUEST),
            Invalid::Fields(f) => validation_error(f),
            Invalid::Message(m) => error_json(StatusCode::BAD_REQUEST, m),
        }
    }
}

// ---- 검증 (DB 없이 테스트한다) ----

// 본문은 JSON 객체여야 한다 (serde 는 배열도 struct 로 읽으므로 먼저 확인한다).
fn json_object<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, Invalid> {
    match serde_json::from_slice(body) {
        Ok(v @ serde_json::Value::Object(_)) => serde_json::from_value(v).map_err(|_| Invalid::Body),
        _ => Err(Invalid::Body),
    }
}

/// 로그인 본문.
pub fn parse_login(body: &[u8]) -> Result<LoginRequest, Invalid> {
    let b: LoginBody = json_object(body)?;
    let email = b.email.trim().to_string();
    if email.is_empty() || b.password.is_empty() {
        return Err(Invalid::Message(MSG_INVALID_LOGIN));
    }
    Ok(LoginRequest {
        email,
        password: b.password,
    })
}

/// 회원가입 본문. name 이 비어 있으면 이메일 @ 앞부분 (basicproject_go 와 같다).
pub fn parse_signup(body: &[u8]) -> Result<SignupRequest, Invalid> {
    let b: SignupBody = json_object(body)?;
    let email = b.email.trim().to_string();
    let mut name = b.name.trim().to_string();
    let mut fields = BTreeMap::new();
    if email.is_empty() {
        fields.insert("email".to_string(), MSG_EMAIL_REQUIRED.to_string());
    } else if !valid_email(&email) {
        fields.insert("email".to_string(), MSG_EMAIL_INVALID.to_string());
    }
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&b.password.len()) {
        fields.insert("password".to_string(), MSG_PASSWORD_LEN.to_string());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        fields.insert("name".to_string(), MSG_NAME_TOO_LONG.to_string());
    }
    if !fields.is_empty() {
        return Err(Invalid::Fields(fields));
    }
    if name.is_empty() {
        name = email.split('@').next().unwrap_or_default().to_string();
    }
    Ok(SignupRequest {
        email,
        password: b.password,
        name,
    })
}

/// 간단한 형식 검사: local@domain (domain 에 . 포함), 공백 / 제어 문자 없음, 255 바이트 이하.
pub fn valid_email(email: &str) -> bool {
    if email.len() > MAX_EMAIL_LEN || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && !domain.contains('@') && domain.split('.').count() >= 2 && domain.split('.').all(|p| !p.is_empty())
        }
        None => false,
    }
}

// ---- 핸들러 ----

fn is_connect_error(err: &Error) -> bool {
    matches!(
        err,
        Error::Db(sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_) | sqlx::Error::Tls(_))
    )
}

/// DB 에러: 접속 문제면 503, 그 밖은 500 (자세한 내용은 로그로만).
fn db_error(err: Error) -> Response {
    tracing::error!(error = %err, "auth db error");
    if is_connect_error(&err) {
        error_json(StatusCode::SERVICE_UNAVAILABLE, MSG_DB_UNAVAILABLE)
    } else {
        error_json(StatusCode::INTERNAL_SERVER_ERROR, MSG_SERVER_ERROR)
    }
}

fn send_token(status: StatusCode, user: &User) -> Response {
    let ttl = auth::token_ttl_secs();
    match auth::make_token(user.id, &user.email, Some(i64::from(user.level))) {
        Some(token) => (status, Json(TokenResponse::new(user, token, ttl))).into_response(),
        None => {
            tracing::error!("token issue failed: JWT_SECRET is not set");
            error_json(StatusCode::INTERNAL_SERVER_ERROR, MSG_TOKEN_FAILED)
        }
    }
}

/// 이메일로 사용자 한 명을 찾는 조건 (u_email = ?, limit 1).
pub fn email_args(email: &str) -> [Arg; 2] {
    [
        Arg::Where(Where::eq("email", email.to_string())),
        Arg::Paging { page: 1, pagesize: 1 },
    ]
}

async fn find_by_email(pool: &MySqlPool, email: &str) -> Result<Option<User>, Error> {
    Ok(User::find(pool, &email_args(email)).await?.into_iter().next())
}

async fn verify_blocking(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || password::verify_passwd(&password, &hash))
        .await
        .unwrap_or(false)
}

fn body_of(body: BodyArg) -> Result<axum::body::Bytes, Invalid> {
    params::body_bytes(body).ok_or(Invalid::Body)
}

/// POST /auth/signup
async fn signup(State(pool): State<MySqlPool>, body: BodyArg) -> Response {
    let req = match body_of(body).and_then(|b| parse_signup(&b)) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };
    match find_by_email(&pool, &req.email).await {
        Ok(Some(_)) => return error_json(StatusCode::CONFLICT, MSG_DUPLICATE_EMAIL),
        Ok(None) => {}
        Err(e) => return db_error(e),
    }
    let Ok(hash) = password::generate_passwd_blocking(req.password).await else {
        return error_json(StatusCode::INTERNAL_SERVER_ERROR, MSG_PASSWORD_FAILED);
    };
    let mut user = User {
        email: req.email,
        passwd: hash,
        name: req.name,
        level: SIGNUP_LEVEL,
        ..Default::default()
    };
    match User::insert(&pool, &mut user).await {
        Ok(id) => user.id = id,
        // 동시에 같은 이메일로 가입한 경우 (uk_user_email)
        Err(Error::Db(sqlx::Error::Database(e))) if e.is_unique_violation() => {
            return error_json(StatusCode::CONFLICT, MSG_DUPLICATE_EMAIL);
        }
        Err(e) if is_connect_error(&e) => return db_error(e),
        Err(e) => {
            tracing::error!(error = %e, "signup insert failed");
            return error_json(StatusCode::INTERNAL_SERVER_ERROR, MSG_SIGNUP_FAILED);
        }
    }
    send_token(StatusCode::CREATED, &user)
}

/// POST /auth/login
async fn login(State(pool): State<MySqlPool>, body: BodyArg) -> Response {
    let req = match body_of(body).and_then(|b| parse_login(&b)) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };
    let user = match find_by_email(&pool, &req.email).await {
        Ok(u) => u,
        Err(e) => return db_error(e),
    };
    let hash = user.as_ref().map_or_else(|| DUMMY_HASH.to_string(), |u| u.passwd.clone());
    let matched = verify_blocking(req.password, hash).await;
    match user {
        Some(user) if matched => send_token(StatusCode::OK, &user),
        _ => error_json(StatusCode::UNAUTHORIZED, MSG_INVALID_LOGIN),
    }
}

/// GET /auth/me
async fn me(State(pool): State<MySqlPool>, session: Option<Session>) -> Response {
    let Some(s) = session else {
        return error_json(StatusCode::UNAUTHORIZED, MSG_AUTH_REQUIRED);
    };
    match User::get(&pool, s.id).await {
        Ok(Some(u)) => Json(UserResponse::from(&u)).into_response(),
        Ok(None) => error_json(StatusCode::NOT_FOUND, MSG_USER_NOT_FOUND),
        Err(e) => db_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fields(pairs: &[(&str, &str)]) -> Invalid {
        Invalid::Fields(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    #[test]
    fn login_validation() {
        assert_eq!(
            parse_login(br#"{"email":" a@b.c ","password":"pw"}"#),
            Ok(LoginRequest {
                email: "a@b.c".into(),
                password: "pw".into()
            })
        );
        assert_eq!(parse_login(b"not json"), Err(Invalid::Body));
        assert_eq!(parse_login(b"[]"), Err(Invalid::Body));
        assert_eq!(parse_login(br#"{"email":1,"password":"pw"}"#), Err(Invalid::Body));
        assert_eq!(parse_login(br#"{"password":"pw"}"#), Err(Invalid::Message(MSG_INVALID_LOGIN)));
        assert_eq!(parse_login(br#"{"email":"a@b.c"}"#), Err(Invalid::Message(MSG_INVALID_LOGIN)));
    }

    #[test]
    fn signup_validation() {
        let ok = parse_signup(r#"{"email":"a@b.c","password":"password1","name":" 홍길동 "}"#.as_bytes()).unwrap();
        assert_eq!(
            ok,
            SignupRequest {
                email: "a@b.c".into(),
                password: "password1".into(),
                name: "홍길동".into()
            }
        );
        // name 이 없으면 이메일 앞부분
        assert_eq!(parse_signup(br#"{"email":"kim@b.c","password":"password1"}"#).unwrap().name, "kim");
        assert_eq!(
            parse_signup(br#"{"password":"short"}"#),
            Err(fields(&[("email", MSG_EMAIL_REQUIRED), ("password", MSG_PASSWORD_LEN)]))
        );
        assert_eq!(
            parse_signup(br#"{"email":"ab.c","password":"password1"}"#),
            Err(fields(&[("email", MSG_EMAIL_INVALID)]))
        );
        let long = format!(r#"{{"email":"a@b.c","password":"{}"}}"#, "x".repeat(73));
        assert_eq!(parse_signup(long.as_bytes()), Err(fields(&[("password", MSG_PASSWORD_LEN)])));
        let ok72 = format!(r#"{{"email":"a@b.c","password":"{}"}}"#, "x".repeat(72));
        assert!(parse_signup(ok72.as_bytes()).is_ok());
        let long_name = format!(r#"{{"email":"a@b.c","password":"password1","name":"{}"}}"#, "가".repeat(101));
        assert_eq!(parse_signup(long_name.as_bytes()), Err(fields(&[("name", MSG_NAME_TOO_LONG)])));
        assert_eq!(parse_signup(b"{"), Err(Invalid::Body));
    }

    #[test]
    fn email_format() {
        assert!(valid_email("a@b.c"));
        for bad in ["", "a", "@b.c", "a@", "a@b", "a@b.", "a b@c.d", "a@b@c.d"] {
            assert!(!valid_email(bad), "{bad}");
        }
        assert!(!valid_email(&format!("{}@b.c", "a".repeat(252))));
    }

    #[test]
    fn token_response_shape() {
        let user = User {
            id: 42,
            email: "a@b.c".into(),
            passwd: "$2a$10$secret".into(),
            name: "n".into(),
            level: 1,
            createddate: "2026-01-01 09:00:00".into(),
            ..Default::default()
        };
        let v = serde_json::to_value(TokenResponse::new(&user, "t.o.k".into(), 86_400)).unwrap();
        assert_eq!(
            v,
            json!({
                "token": "t.o.k",
                "tokenType": "Bearer",
                "expiresInMs": 86_400_000,
                "user": {"id": 42, "email": "a@b.c", "name": "n", "level": 1, "createdAt": "2026-01-01 09:00:00"}
            })
        );
        assert!(!v.to_string().contains("secret"));
    }

    #[tokio::test]
    async fn error_shape() {
        use http_body_util::BodyExt;
        let res = error_json(StatusCode::CONFLICT, MSG_DUPLICATE_EMAIL);
        assert_eq!(res.status(), StatusCode::CONFLICT);
        let v: Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(
            (&v["status"], &v["message"], &v["fieldErrors"]),
            (&json!(409), &json!(MSG_DUPLICATE_EMAIL), &json!({}))
        );
        // 2006-01-02T15:04:05 형식
        assert_eq!(v["timestamp"].as_str().unwrap().len(), 19);
    }

    #[test]
    fn email_lookup_sql() {
        let qb = User::find_query(&email_args("a@b.c"));
        let sql = qb.sql().as_str().to_string();
        assert!(sql.contains("where 1=1 and u_email = ?"), "{sql}");
        assert!(sql.ends_with("limit ? offset ?"), "{sql}");
    }

    #[test]
    fn dummy_hash_is_valid() {
        // 형식이 잘못된 해시면 verify 가 바로 false 라서 시간 차이가 생긴다
        assert!(bcrypt::verify("go-secret-123", DUMMY_HASH).unwrap());
    }
}
