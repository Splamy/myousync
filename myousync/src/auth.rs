use std::{
    sync::LazyLock,
    time::{Duration, SystemTime},
};

use axum::{
    Json,
    body::Body,
    extract::Request,
    http::{self, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, TokenData, Validation};
use log::info;
use rand::{
    Rng,
    distr::{Alphanumeric, SampleString},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    dbdata::{self, DB},
    util,
};
use pbkdf2::{
    Pbkdf2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, Salt, SaltString},
};

const AUTH_SECRET_KEY: &str = "auth_server_secret";
static SECRET: LazyLock<Box<str>> = LazyLock::new(|| get_server_secret().into_boxed_str());

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub exp: u64,     // Expiry time of the token
    pub iat: u64,     // Issued at time of the token
    pub user: String, // Email associated with the token
}

#[derive(Deserialize)]
pub struct SignInData {
    pub username: String, // Email entered during sign-in
    pub password: String, // Password entered during sign-in
}

pub async fn sign_in(
    Json(user_data): Json<SignInData>, // JSON payload containing sign-in data
) -> Result<impl IntoResponse, AuthError> {
    info!("Got login request for {}", &user_data.username);

    let user = dbdata::DB
        .get_user(&user_data.username)
        // User not found, return unauthorized status
        .ok_or_else(|| AuthError {
            message: "User not found".to_string(),
            status_code: StatusCode::UNAUTHORIZED,
        })?;

    if verify_password(&user.password, &user_data.password) {
        return Err(AuthError {
            message: "Invalid password".to_string(),
            status_code: StatusCode::UNAUTHORIZED,
        });
    }
    let token = encode_jwt(user.username)
        // Handle JWT encoding errors
        .map_err(|_| AuthError {
            message: "Internal token error".to_string(),
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
        })?;
    Ok(Json(token))
}

pub fn encode_jwt(email: String) -> Result<String, StatusCode> {
    let secret: String = SECRET.to_string();
    let now = SystemTime::now();
    let expire: Duration = Duration::from_secs(24 * 60 * 60);
    let exp = util::time::to_timestamp(now + expire);
    let iat = util::time::to_timestamp(now);
    let claim = Claims {
        iat,
        exp,
        user: email,
    };

    jsonwebtoken::encode(
        &Header::default(),
        &claim,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub fn decode_jwt(jwt_token: &str) -> Result<TokenData<Claims>, StatusCode> {
    let secret = SECRET.to_string();
    let result: Result<TokenData<Claims>, StatusCode> = jsonwebtoken::decode(
        jwt_token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
    result
}

pub fn hash_password(password: &str) -> String {
    let mut rng = rand::rng();
    let mut bytes = [0u8; Salt::RECOMMENDED_LENGTH];
    rng.fill(&mut bytes);
    let salt = SaltString::encode_b64(&bytes).unwrap();

    let params = pbkdf2::Params {
        rounds: 1000,
        ..Default::default()
    };
    Pbkdf2
        .hash_password_customized(password.as_bytes(), None, None, params, &salt)
        .unwrap()
        .to_string()
}

pub fn verify_password(stored: &str, password: &str) -> bool {
    let parsed_hash = PasswordHash::new(stored).unwrap();
    Pbkdf2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub struct AuthError {
    pub message: String,
    pub status_code: StatusCode,
}

pub async fn auth(req: Request, next: Next) -> Result<Response, AuthError> {
    if req.method() == http::Method::OPTIONS {
        return Ok(next.run(req).await);
    }

    let auth_header = req.headers().get(http::header::AUTHORIZATION);
    let auth_header = match auth_header {
        Some(header) => header.to_str().map_err(|_| AuthError {
            message: "Empty header is not allowed".to_string(),
            status_code: StatusCode::FORBIDDEN,
        })?,
        None => {
            return Err(AuthError {
                message: "Please add the JWT token to the header".to_string(),
                status_code: StatusCode::FORBIDDEN,
            });
        }
    };
    let mut header = auth_header.split_whitespace();
    let (_bearer, token) = (header.next(), header.next());
    let token_data = decode_jwt(token.unwrap()).map_err(|_| AuthError {
        message: "Unable to decode token".to_string(),
        status_code: StatusCode::UNAUTHORIZED,
    })?;
    // Fetch the user details from the database
    let _current_user = dbdata::DB
        .get_user(&token_data.claims.user)
        .ok_or_else(|| AuthError {
            message: "You are not an authorized user".to_string(),
            status_code: StatusCode::UNAUTHORIZED,
        })?;
    Ok(next.run(req).await)
}

pub fn get_server_secret() -> String {
    DB.get_key(AUTH_SECRET_KEY).unwrap_or_else(|| {
        let secret = Alphanumeric.sample_string(&mut rand::rng(), 16);
        DB.set_key(AUTH_SECRET_KEY, &secret);
        secret
    })
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response<Body> {
        let body = Json(json!({
            "error": self.message,
        }));

        (self.status_code, body).into_response()
    }
}
