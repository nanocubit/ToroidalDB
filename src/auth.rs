use bcrypt::{hash, verify, DEFAULT_COST};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,              // subject (user id)
    pub exp: usize,               // expiration time
    pub iat: usize,               // issued at time
    pub roles: Vec<String>,       // user roles
    pub permissions: Vec<String>, // user permissions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub created_at: u64,
    pub last_login: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthPayload {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: usize,
}

pub struct AuthService {
    jwt_secret: String,
    users: RwLock<HashMap<String, User>>, // Thread-safe user storage
}

impl AuthService {
    pub fn new(jwt_secret: String) -> Self {
        let auth_service = AuthService {
            jwt_secret,
            users: RwLock::new(HashMap::new()),
        };

        // Create a default admin user
        auth_service.create_default_admin();

        auth_service
    }

    fn create_default_admin(&self) {
        let admin_password =
            std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin-password-123".to_string());

        let password_hash = hash(&admin_password, DEFAULT_COST).unwrap_or_else(|_| {
            // Fallback to simple hash if bcrypt fails
            "admin_hash_fallback".to_string()
        });

        let admin_user = User {
            id: "admin".to_string(),
            username: "admin".to_string(),
            password_hash,
            roles: vec!["admin".to_string()],
            permissions: vec![
                "read:all".to_string(),
                "write:all".to_string(),
                "execute:tql".to_string(),
                "manage:users".to_string(),
            ],
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            last_login: None,
        };

        if let Ok(mut users) = self.users.write() {
            users.insert("admin".to_string(), admin_user);
        }
    }

    pub fn register_user(
        &self,
        username: String,
        password: String,
        roles: Vec<String>,
    ) -> Result<String, String> {
        let password_hash =
            hash(password, DEFAULT_COST).map_err(|e| format!("Password hashing error: {}", e))?;

        let user_id = Uuid::new_v4().to_string();
        let mut user = User {
            id: user_id.clone(),
            username: username.clone(),
            password_hash,
            roles: roles.clone(),
            permissions: self.roles_to_permissions(&roles),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            last_login: None,
        };

        if let Ok(mut users) = self.users.write() {
            if users.contains_key(&username) {
                return Err("User already exists".to_string());
            }
            users.insert(username, user);
            Ok(user_id)
        } else {
            Err("Failed to acquire write lock".to_string())
        }
    }

    pub fn authenticate(&self, username: &str, password: &str) -> Result<TokenResponse, String> {
        let user = {
            let users = self
                .users
                .read()
                .map_err(|_| "Failed to acquire read lock")?;
            users.get(username).ok_or("User not found")?.clone()
        };

        if !verify(password, &user.password_hash).map_err(|_| "Verification error")? {
            return Err("Invalid credentials".to_string());
        }

        // Update last login
        let mut user = user;
        user.last_login = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );

        if let Ok(mut users) = self.users.write() {
            users.insert(username.to_string(), user.clone());
        } else {
            return Err("Failed to update user".to_string());
        }

        let expiration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
            + 3600; // 1 hour

        let claims = Claims {
            sub: username.to_string(),
            exp: expiration,
            iat: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as usize,
            roles: user.roles.clone(),
            permissions: user.permissions.clone(),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_ref()),
        )
        .map_err(|e| format!("JWT encoding error: {}", e))?;

        Ok(TokenResponse {
            access_token: token,
            token_type: "Bearer".to_string(),
            expires_in: 3600,
        })
    }

    pub fn validate_token(&self, token: &str) -> Result<Claims, String> {
        let validation = Validation::default();
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_ref()),
            &validation,
        )
        .map_err(|e| format!("JWT decoding error: {}", e))?;

        Ok(token_data.claims)
    }

    pub fn has_permission(&self, claims: &Claims, permission: &str) -> bool {
        claims.permissions.contains(&permission.to_string())
    }

    pub fn has_role(&self, claims: &Claims, role: &str) -> bool {
        claims.roles.contains(&role.to_string())
    }

    fn roles_to_permissions(&self, roles: &[String]) -> Vec<String> {
        let mut permissions = Vec::new();

        for role in roles {
            match role.as_str() {
                "admin" => {
                    permissions.extend_from_slice(&[
                        "read:all".to_string(),
                        "write:all".to_string(),
                        "execute:tql".to_string(),
                        "manage:users".to_string(),
                        "manage:system".to_string(),
                    ]);
                }
                "user" => {
                    permissions.extend_from_slice(&[
                        "read:own".to_string(),
                        "write:own".to_string(),
                        "execute:tql".to_string(),
                    ]);
                }
                "analyst" => {
                    permissions.extend_from_slice(&[
                        "read:all".to_string(),
                        "execute:tql".to_string(),
                        "read:analytics".to_string(),
                    ]);
                }
                _ => {
                    // Unknown role, no permissions
                }
            }
        }

        // Remove duplicates
        permissions.sort();
        permissions.dedup();

        permissions
    }

    pub fn get_user_by_username(&self, username: &str) -> Option<User> {
        let users = self.users.read().ok()?;
        users.get(username).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_jwt_authentication() {
        let auth_service = AuthService::new("test-secret-key".to_string());

        // Register a test user
        let user_id = auth_service
            .register_user(
                "testuser".to_string(),
                "password123".to_string(),
                vec!["user".to_string()],
            )
            .unwrap();

        assert!(!user_id.is_empty());

        // Authenticate user
        let token_response = auth_service
            .authenticate("testuser", "password123")
            .unwrap();
        assert!(!token_response.access_token.is_empty());

        // Validate token
        let claims = auth_service
            .validate_token(&token_response.access_token)
            .unwrap();
        assert_eq!(claims.sub, "testuser");
        assert!(auth_service.has_permission(&claims, "execute:tql"));
    }

    #[test]
    fn test_permissions() {
        let auth_service = AuthService::new("test-secret-key".to_string());

        // Test admin permissions
        let admin_claims = Claims {
            sub: "admin".to_string(),
            exp: 9999999999, // far future
            iat: 0,
            roles: vec!["admin".to_string()],
            permissions: vec![
                "read:all".to_string(),
                "write:all".to_string(),
                "execute:tql".to_string(),
            ],
        };

        assert!(auth_service.has_permission(&admin_claims, "execute:tql"));
        assert!(auth_service.has_permission(&admin_claims, "read:all"));
        assert!(auth_service.has_permission(&admin_claims, "write:all"));

        // Test user permissions
        let user_claims = Claims {
            sub: "user".to_string(),
            exp: 9999999999, // far future
            iat: 0,
            roles: vec!["user".to_string()],
            permissions: vec![
                "read:own".to_string(),
                "write:own".to_string(),
                "execute:tql".to_string(),
            ],
        };

        assert!(auth_service.has_permission(&user_claims, "execute:tql"));
        assert!(auth_service.has_permission(&user_claims, "read:own"));
        assert!(!auth_service.has_permission(&user_claims, "read:all")); // user shouldn't have this
    }
}
