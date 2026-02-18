use crate::application::use_cases::auth::AuthUseCase;
use crate::bootstrap::config::AppConfig;
use crate::infrastructure::db::repositories::auth_repository::SqlxAuthRepository;
use crate::infrastructure::db::sqlx::pool::{create_pg_pool, run_migrations};
use crate::infrastructure::jwt::access_token::JwtAdapter;
use crate::infrastructure::jwt::refresh_token::RefreshTokenAdapter;
use crate::infrastructure::password::argon2_password::Argon2PasswordAdapter;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: AppConfig,
    db_pool: PgPool,
    auth_use_case: AuthUseCase,
}

impl AppState {
    /// Initializes application shared state and runtime adapters.
    ///
    /// # Parameters
    /// - `config`: Runtime configuration loaded from environment.
    pub async fn new(config: AppConfig) -> anyhow::Result<Self> {
        let db_pool = create_pg_pool(&config.database_url, config.db_max_connections).await?;
        run_migrations(&db_pool).await?;

        let auth_repo = Arc::new(SqlxAuthRepository::new(db_pool.clone()));
        let auth_use_case = AuthUseCase::new(
            auth_repo.clone(),
            auth_repo,
            Arc::new(Argon2PasswordAdapter::new()),
            Arc::new(JwtAdapter::new(&config.jwt_secret)),
            Arc::new(RefreshTokenAdapter::new(&config.refresh_token_pepper)),
        );

        Ok(Self {
            inner: Arc::new(AppStateInner {
                config,
                db_pool,
                auth_use_case,
            }),
        })
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn auth_use_case(&self) -> &AuthUseCase {
        &self.inner.auth_use_case
    }

    #[allow(dead_code)]
    pub fn db_pool(&self) -> &PgPool {
        &self.inner.db_pool
    }
}
