// buildtool 이 한 번만 만드는 사용자 파일이다 (있으면 덮어쓰지 않는다).
// 요청에서 로그인 세션을 꺼낸다. Go / Spring 백엔드와 같은 통합 JWT 형식이다:
// Authorization: Bearer <token>, HS256, 비밀 키는 환경변수 JWT_SECRET (32 바이트 이상),
// 유효 시간은 JWT_EXPIRATION_MS (밀리초, 기본 86400000 = 24시간),
// claim {"sub": "<id 문자열>", "id": 1, "email": "a@b.c", "level": 9, "iat": 초, "exp": 초}
// (level 은 없어도 된다, exp 는 있으면 검증한다, id 가 없으면 sub 를 숫자로 읽는다).
// 다른 토큰 형식을 쓰려면 **이 파일만** 고친다.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use super::session::Session;

/// 토큰 claim (통합 JWT). 읽을 때는 없는 claim 을 허용하고, 만들 때는 level 이 없으면 뺀다.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Claims {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(default)]
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
}

/// 토큰 유효 시간 (초, 1일). JWT_EXPIRATION_MS 가 없을 때의 기본값.
pub const TOKEN_TTL_SECS: i64 = 86_400;

/// JWT_SECRET 최소 길이 (바이트, Go / Spring 템플릿과 같다).
pub const MIN_SECRET_LEN: usize = 32;

/// 발급 토큰의 유효 시간 (초). JWT_EXPIRATION_MS 가 없거나 잘못되면 TOKEN_TTL_SECS.
pub fn token_ttl_secs() -> i64 {
    static TTL: OnceLock<i64> = OnceLock::new();
    *TTL.get_or_init(|| ttl_from_ms(std::env::var("JWT_EXPIRATION_MS").ok().as_deref()))
}

/// JWT_EXPIRATION_MS 값을 초로 바꾼다 (1 초 미만은 1 초, 양의 정수가 아니면 경고하고 기본값).
pub fn ttl_from_ms(value: Option<&str>) -> i64 {
    match value.map(str::trim).filter(|v| !v.is_empty()).map(str::parse::<i64>) {
        None => TOKEN_TTL_SECS,
        Some(Ok(ms)) if ms > 0 => (ms / 1000).max(1),
        Some(_) => {
            tracing::warn!("JWT_EXPIRATION_MS is not a positive integer, using {TOKEN_TTL_SECS} seconds");
            TOKEN_TTL_SECS
        }
    }
}

/// JWT_SECRET 값을 확인한다. 없거나 MIN_SECRET_LEN 바이트보다 짧으면 Err (main.rs 가 시작할 때 부른다).
pub fn check_secret(value: Option<&str>) -> Result<&str, String> {
    match value {
        None | Some("") => Err("JWT_SECRET is not set".to_string()),
        Some(s) if s.len() < MIN_SECRET_LEN => Err(format!("JWT_SECRET must be at least {MIN_SECRET_LEN} bytes (got {})", s.len())),
        Some(s) => Ok(s),
    }
}

fn secret() -> Option<&'static str> {
    static SECRET: OnceLock<Option<String>> = OnceLock::new();
    SECRET
        .get_or_init(|| {
            let value = std::env::var("JWT_SECRET").ok();
            match check_secret(value.as_deref()) {
                Ok(s) => Some(s.to_string()),
                Err(e) => {
                    // 짧은 키는 쓰지 않는다: 모든 요청이 비로그인, 토큰을 만들지 않는다.
                    tracing::error!("{e}: every request is treated as logged out");
                    None
                }
            }
        })
        .as_deref()
}

// JWT_SECRET 이 없거나 짧으면 모든 요청이 비로그인이다 (에러는 한 번만 찍는다).
fn decoding_key() -> Option<&'static DecodingKey> {
    static KEY: OnceLock<Option<DecodingKey>> = OnceLock::new();
    KEY.get_or_init(|| secret().map(|s| DecodingKey::from_secret(s.as_bytes())))
        .as_ref()
}

/// Authorization: Bearer <token> 의 토큰.
fn bearer(parts: &Parts) -> Option<&str> {
    let value = parts.headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.trim().split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

/// 토큰을 검증하고 세션으로 바꾼다. 잘못된 토큰이나 id 가 0 이하이면 None.
pub fn session_from_token(token: &str, key: &DecodingKey) -> Option<Session> {
    let mut validation = Validation::new(Algorithm::HS256);
    // exp 는 필수가 아니고, 있으면 만료를 검증한다.
    validation.required_spec_claims.clear();
    let claims = decode::<Claims>(token, key, &validation).ok()?.claims;
    let id = claims.id.or_else(|| claims.sub.as_deref()?.trim().parse().ok())?;
    if id <= 0 {
        return None;
    }
    Some(Session {
        id,
        email: claims.email,
        level: claims.level,
        ..Default::default()
    })
}

/// 통합 JWT 를 만든다 (로그인 핸들러용). key 는 JWT_SECRET 바이트.
pub fn token_with_key(id: i64, email: &str, level: Option<i64>, ttl_secs: i64, key: &[u8]) -> Option<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let claims = Claims {
        sub: Some(id.to_string()),
        id: Some(id),
        email: email.to_string(),
        level,
        iat: Some(now),
        exp: Some(now + ttl_secs),
    };
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(key)).ok()
}

/// JWT_SECRET 으로 token_ttl_secs() 동안 유효한 토큰을 만든다. JWT_SECRET 이 없거나 짧으면 None.
pub fn make_token(id: i64, email: &str, level: Option<i64>) -> Option<String> {
    token_with_key(id, email, level, token_ttl_secs(), secret()?.as_bytes())
}

/// 요청의 세션 (Option<Session> 추출기가 부른다). 토큰이 없거나 잘못되면 None (Go 의 nil Session).
/// DB 에서 사용자를 다시 읽는 등 비동기 작업이 필요하면 여기서 한다.
pub async fn session_from_parts(parts: &Parts) -> Option<Session> {
    session_from_token(bearer(parts)?, decoding_key()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn token(claims: serde_json::Value, secret: &[u8]) -> String {
        encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret)).unwrap()
    }

    #[test]
    fn tokens() {
        let key = DecodingKey::from_secret(b"secret");
        let exp = 4_102_444_800i64; // 2100-01-01
        let s = session_from_token(
            &token(
                json!({"sub": "7", "id": 7, "email": "a@b.c", "level": 9, "iat": 1, "exp": exp}),
                b"secret",
            ),
            &key,
        )
        .unwrap();
        assert_eq!((s.id, s.email.as_str(), s.level), (7, "a@b.c", Some(9)));
        let s = session_from_token(&token(json!({"id": 8, "exp": exp}), b"secret"), &key).unwrap();
        assert_eq!((s.id, s.level), (8, None));
        // id 가 없으면 sub 를 읽는다
        assert_eq!(
            session_from_token(&token(json!({"sub": "12", "exp": exp}), b"secret"), &key).map(|s| s.id),
            Some(12)
        );
        assert!(session_from_token(&token(json!({"sub": "x", "exp": exp}), b"secret"), &key).is_none());
        assert!(session_from_token(&token(json!({"id": 0, "exp": exp}), b"secret"), &key).is_none());
        assert!(session_from_token(&token(json!({"id": 7, "exp": exp}), b"other"), &key).is_none());
        assert!(session_from_token(&token(json!({"id": 7, "exp": 1}), b"secret"), &key).is_none());
        // exp 가 없는 토큰도 받는다, 서명은 검증한다
        assert_eq!(session_from_token(&token(json!({"id": 7}), b"secret"), &key).map(|s| s.id), Some(7));
        assert!(session_from_token(&token(json!({"id": 7}), b"other"), &key).is_none());
        assert!(session_from_token("garbage", &key).is_none());
        // HS256 이외 알고리즘은 받지 않는다
        let hs512 = encode(
            &Header::new(Algorithm::HS512),
            &json!({"id": 7}),
            &EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        assert!(session_from_token(&hs512, &key).is_none());
    }

    #[test]
    fn issue() {
        let t = token_with_key(5, "u@x.y", None, 60, b"secret").unwrap();
        let mut v = Validation::new(Algorithm::HS256);
        v.required_spec_claims.clear();
        let raw = decode::<serde_json::Value>(&t, &DecodingKey::from_secret(b"secret"), &v)
            .unwrap()
            .claims;
        assert_eq!((&raw["sub"], &raw["id"], &raw["email"]), (&json!("5"), &json!(5), &json!("u@x.y")));
        assert!(raw.get("level").is_none());
        assert_eq!(raw["exp"].as_i64().unwrap() - raw["iat"].as_i64().unwrap(), 60);
        let s = session_from_token(
            &token_with_key(5, "u@x.y", Some(3), 60, b"secret").unwrap(),
            &DecodingKey::from_secret(b"secret"),
        )
        .unwrap();
        assert_eq!((s.id, s.email.as_str(), s.level), (5, "u@x.y", Some(3)));
    }

    #[test]
    fn secret_and_ttl() {
        assert!(check_secret(None).is_err());
        assert!(check_secret(Some("")).is_err());
        assert!(check_secret(Some("short")).unwrap_err().contains("at least 32 bytes"));
        assert_eq!(check_secret(Some(&"k".repeat(32))), Ok("k".repeat(32).as_str()));
        assert_eq!(ttl_from_ms(None), TOKEN_TTL_SECS);
        assert_eq!(ttl_from_ms(Some("3600000")), 3600);
        assert_eq!(ttl_from_ms(Some("500")), 1);
        assert_eq!(ttl_from_ms(Some("-1")), TOKEN_TTL_SECS);
        assert_eq!(ttl_from_ms(Some("abc")), TOKEN_TTL_SECS);
    }

    #[test]
    fn bearer_header() {
        let req = axum::http::Request::builder()
            .header("authorization", "bearer abc")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        assert_eq!(bearer(&parts), Some("abc"));
        let req = axum::http::Request::builder()
            .header("authorization", "Basic abc")
            .body(())
            .unwrap();
        assert_eq!(bearer(&req.into_parts().0), None);
    }
}
