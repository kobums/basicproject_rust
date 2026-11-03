// 환경변수 해석 (직접 관리하는 파일). basicproject_go global/config 와 같은 이름 / 기본값이다.
// 환경을 직접 읽지 않고 값을 받아서 테스트할 수 있게 한다.

use axum::http::HeaderValue;

/// 기본 포트 (basicproject_go 8007, Spring 8081 과 겹치지 않게).
pub const DEFAULT_PORT: u16 = 8008;

/// 개발 기본 CORS 허용 출처 (basicproject_go DefaultCorsOrigins 와 같다).
pub const DEFAULT_CORS_ORIGINS: &str = "http://localhost:5173,http://localhost:3000,http://localhost:9007";

/// JWT 비밀 키 최소 길이 (HS256, 바이트). basicproject_go 와 같다.
pub const MIN_JWT_SECRET_LEN: usize = 32;

/// APP_MODE=production 인지 (그 밖은 develop).
pub fn is_production(mode: Option<&str>) -> bool {
    mode.is_some_and(|m| m.trim().eq_ignore_ascii_case("production"))
}

/// DB 접속 URL. DATABASE_URL 이 있으면 그대로, 없으면 DB_* 값으로 만든다
/// (DB_HOST 기본 127.0.0.1, DB_PORT 기본 3306, DB_USER 필수, DB_PASSWORD, DB_NAME 없으면 MARIADB_DATABASE).
pub fn database_url(env: impl Fn(&str) -> Option<String>) -> Result<String, String> {
    let get = |k: &str| env(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    if let Some(url) = get("DATABASE_URL") {
        return Ok(url);
    }
    let user = get("DB_USER").ok_or("DATABASE_URL or DB_USER / DB_PASSWORD / DB_NAME is not set (cp .env.example .env)")?;
    let password = env("DB_PASSWORD").unwrap_or_default();
    let host = get("DB_HOST").unwrap_or_else(|| "127.0.0.1".into());
    let port = get("DB_PORT").unwrap_or_else(|| "3306".into());
    port.parse::<u16>().map_err(|_| format!("DB_PORT is not a port number: {port}"))?;
    let name = get("DB_NAME")
        .or_else(|| get("MARIADB_DATABASE"))
        .ok_or("DB_NAME is not set; set DB_HOST/DB_PORT/DB_USER/DB_PASSWORD/DB_NAME in .env or the environment")?;
    Ok(format!(
        "mysql://{}:{}@{host}:{port}/{}",
        encode(&user),
        encode(&password),
        encode(&name)
    ))
}

/// PORT (없으면 DEFAULT_PORT).
pub fn port(value: Option<&str>) -> Result<u16, String> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(DEFAULT_PORT),
        Some(v) => v.parse().map_err(|_| format!("PORT is not a port number: {v}")),
    }
}

/// CORS 설정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cors {
    /// CORS 헤더를 붙이지 않는다 (같은 origin 만, production 에서 CORS_ORIGINS 가 비었을 때).
    Off,
    /// 모든 origin ("*", credentials 없음).
    Any,
    /// 이 origin 들만 (credentials 허용).
    List(Vec<HeaderValue>),
}

/// CORS_ORIGINS (쉼표 구분). 비어 있으면 develop 은 DEFAULT_CORS_ORIGINS, production 은 Off. "*" 가 있으면 Any.
pub fn cors(value: Option<&str>, production: bool) -> Cors {
    let raw = value.map(str::trim).filter(|v| !v.is_empty());
    let raw = match raw {
        Some(v) => v,
        None if production => return Cors::Off,
        None => DEFAULT_CORS_ORIGINS,
    };
    let items: Vec<&str> = raw.split(',').map(str::trim).filter(|o| !o.is_empty()).collect();
    if items.contains(&"*") {
        return Cors::Any;
    }
    let list: Vec<HeaderValue> = items.into_iter().filter_map(|o| HeaderValue::from_str(o).ok()).collect();
    if list.is_empty() { Cors::Off } else { Cors::List(list) }
}

/// JWT_SECRET 확인 (basicproject_go config.Validate 와 같다): 32 바이트 이상, production 에서는 .env.example 값(change-me...) 거부.
pub fn validate_jwt_secret(secret: Option<&str>, production: bool) -> Result<(), String> {
    let secret = secret.unwrap_or_default();
    if secret.len() < MIN_JWT_SECRET_LEN {
        return Err(format!(
            "JWT_SECRET must be at least {MIN_JWT_SECRET_LEN} bytes (got {}); set it in .env or the environment",
            secret.len()
        ));
    }
    if production && secret.starts_with("change-me") {
        return Err("JWT_SECRET is still the .env.example placeholder; generate one (openssl rand -base64 48)".into());
    }
    Ok(())
}

// URL userinfo / path 에 넣을 값의 percent-encoding (unreserved 문자만 그대로).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn url_from_database_url() {
        let url = database_url(env(&[("DATABASE_URL", "mysql://a:b@h:1/x"), ("DB_USER", "ignored")])).unwrap();
        assert_eq!(url, "mysql://a:b@h:1/x");
    }

    #[test]
    fn url_from_db_vars() {
        let url = database_url(env(&[
            ("DB_USER", "project"),
            ("DB_PASSWORD", "p@ss:w/rd"),
            ("MARIADB_DATABASE", "proj"),
        ]))
        .unwrap();
        assert_eq!(url, "mysql://project:p%40ss%3Aw%2Frd@127.0.0.1:3306/proj");
        // DB_NAME 이 MARIADB_DATABASE 보다 우선 (basicproject_go 와 같다)
        let url = database_url(env(&[
            ("DB_USER", "u"),
            ("DB_HOST", "db"),
            ("DB_PORT", "3307"),
            ("DB_NAME", "n"),
            ("MARIADB_DATABASE", "m"),
        ]))
        .unwrap();
        assert_eq!(url, "mysql://u:@db:3307/n");
        // 만든 URL 은 sqlx 가 해석할 수 있다
        assert!(crate::support::state::connect_options(&url).is_ok());
    }

    #[test]
    fn url_errors() {
        assert!(database_url(env(&[])).is_err());
        assert!(database_url(env(&[("DB_USER", "u")])).is_err());
        assert!(database_url(env(&[("DB_USER", "u"), ("DB_NAME", "n"), ("DB_PORT", "x")])).is_err());
    }

    #[test]
    fn port_values() {
        assert_eq!(port(None), Ok(DEFAULT_PORT));
        assert_eq!(port(Some(" ")), Ok(DEFAULT_PORT));
        assert_eq!(port(Some("9000")), Ok(9000));
        assert!(port(Some("abc")).is_err());
    }

    #[test]
    fn cors_values() {
        assert_eq!(
            cors(Some("http://localhost:5173, ,https://example.com"), true),
            Cors::List(vec![
                HeaderValue::from_static("http://localhost:5173"),
                HeaderValue::from_static("https://example.com")
            ])
        );
        let Cors::List(dev) = cors(None, false) else {
            panic!("develop default")
        };
        assert_eq!(dev.len(), 3);
        assert_eq!(cors(Some(""), true), Cors::Off);
        assert_eq!(cors(Some("*"), true), Cors::Any);
        assert!(is_production(Some("Production")));
        assert!(!is_production(Some("develop")));
        assert!(!is_production(None));
    }

    #[test]
    fn jwt_secret_rules() {
        assert!(validate_jwt_secret(None, false).is_err());
        assert!(validate_jwt_secret(Some("short"), false).is_err());
        let placeholder = "change-me-to-a-long-random-string-32chars";
        assert!(validate_jwt_secret(Some(placeholder), false).is_ok());
        assert!(validate_jwt_secret(Some(placeholder), true).is_err());
        assert!(validate_jwt_secret(Some(&"x".repeat(32)), true).is_ok());
    }
}
