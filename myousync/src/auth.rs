use std::sync::LazyLock;

use axum::{
    body::Body,
    extract::Request,
    http::{self, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, TokenData, Validation};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::dbdata;

static SECRET: LazyLock<Box<str>> = LazyLock::new(|| get_server_secret().into_boxed_str());

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub exp: usize,   // Expiry time of the token
    pub iat: usize,   // Issued at time of the token
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
    let user = match dbdata::DB.get_user(&user_data.username) {
        Some(user) => user, // User found, proceed with authentication
        None => {
            return Err(AuthError {
                message: "User not found".to_string(),
                status_code: StatusCode::UNAUTHORIZED,
            })
        } // User not found, return unauthorized status
    };
    if user.password != user_data.password {
        return Err(AuthError {
            message: "Invalid password".to_string(),
            status_code: StatusCode::UNAUTHORIZED,
        });
    }
    let token = encode_jwt(user.username).map_err(|_| AuthError {
        message: "Internal token error".to_string(),
        status_code: StatusCode::INTERNAL_SERVER_ERROR,
    })?; // Handle JWT encoding errors
    Ok(Json(token))
}

pub fn encode_jwt(email: String) -> Result<String, StatusCode> {
    let secret: String = SECRET.to_string();
    let now = Utc::now();
    let expire: chrono::TimeDelta = Duration::hours(24);
    let exp: usize = (now + expire).timestamp() as usize;
    let iat: usize = now.timestamp() as usize;
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
            })
        }
    };
    let mut header = auth_header.split_whitespace();
    let (bearer, token) = (header.next(), header.next());
    let token_data = match decode_jwt(token.unwrap()) {
        Ok(data) => data,
        Err(_) => {
            return Err(AuthError {
                message: "Unable to decode token".to_string(),
                status_code: StatusCode::UNAUTHORIZED,
            })
        }
    };
    // Fetch the user details from the database
    let current_user = match dbdata::DB.get_user(&token_data.claims.user) {
        Some(user) => user,
        None => {
            return Err(AuthError {
                message: "You are not an authorized user".to_string(),
                status_code: StatusCode::UNAUTHORIZED,
            })
        }
    };
    Ok(next.run(req).await)
}

pub fn get_server_secret() -> String {
    dbdata::DB.get_key("auth_server_secret").unwrap_or_else(|| {
        let secret = Alphanumeric.sample_string(&mut rand::rng(), 16);
        dbdata::DB.set_key("auth_server_secret", &secret);
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
