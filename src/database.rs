use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Database {
    pool: PgPool,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Repository {
    pub id: Uuid,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub is_private: bool,
    pub is_archived: bool,
    pub is_fork: bool,
    pub has_documents_config: bool,
    pub documents_config: Option<String>, // JSON string of documents.toml content
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub last_processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Document {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub file_path: String,
    pub title: String,
    pub content: String,
    pub content_hash: String,
    pub metadata: Option<String>, // JSON string of frontmatter/metadata
    pub file_size: i64,
    pub last_modified_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProcessingJob {
    pub id: Uuid,
    pub repository_id: Option<Uuid>,
    pub job_type: String, // "scan_organization", "process_repository", "process_document"
    pub status: String,   // "pending", "running", "completed", "failed"
    pub parameters: Option<String>, // JSON string of job parameters
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Database connection error: {0}")]
    Connection(#[from] sqlx::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Repository not found: {0}")]
    RepositoryNotFound(String),
    #[error("Document not found: {0}")]
    DocumentNotFound(String),
}

impl Database {
    /// Create a new database instance with a connection pool.
    pub async fn new(database_url: &str) -> Result<Self, DatabaseError> {
        let pool = PgPool::connect(database_url).await?;
        Ok(Self { pool })
    }

    /// Run database migrations.
    pub async fn migrate(&self) -> Result<(), DatabaseError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| DatabaseError::Migration(e.to_string()))?;
        Ok(())
    }

    /// Health check for database connection.
    pub async fn health_check(&self) -> Result<(), DatabaseError> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await?;
        Ok(())
    }

    /// Get a connection pool for advanced usage.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // Repository operations
    pub async fn upsert_repository(&self, repo: &Repository) -> Result<Repository, DatabaseError> {
        let result = sqlx::query_as!(
            Repository,
            r#"
INSERT INTO repositories (
    id, name, full_name, description, default_branch, is_private, is_archived, is_fork,
    has_documents_config, documents_config, last_scanned_at, last_processed_at, created_at, updated_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
ON CONFLICT (full_name)
DO UPDATE SET
    name = EXCLUDED.name,
    description = EXCLUDED.description,
    default_branch = EXCLUDED.default_branch,
    is_private = EXCLUDED.is_private,
    is_archived = EXCLUDED.is_archived,
    is_fork = EXCLUDED.is_fork,
    has_documents_config = EXCLUDED.has_documents_config,
    documents_config = EXCLUDED.documents_config,
    last_scanned_at = EXCLUDED.last_scanned_at,
    last_processed_at = EXCLUDED.last_processed_at,
    updated_at = EXCLUDED.updated_at
RETURNING *
            "#,
            repo.id,
            repo.name,
            repo.full_name,
            repo.description,
            repo.default_branch,
            repo.is_private,
            repo.is_archived,
            repo.is_fork,
            repo.has_documents_config,
            repo.documents_config.as_deref(),
            repo.last_scanned_at,
            repo.last_processed_at,
            repo.created_at,
            repo.updated_at
        )
            .fetch_one(&self.pool)
            .await?;

        Ok(result)
    }

    pub async fn get_repository_by_full_name(
        &self,
        full_name: &str,
    ) -> Result<Repository, DatabaseError> {
        let result = sqlx::query_as!(
            Repository,
            "SELECT * FROM repositories WHERE full_name = $1",
            full_name
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|_| DatabaseError::DocumentNotFound(full_name.to_string()))?;

        Ok(result)
    }

    pub async fn list_repositories_with_documents(&self) -> Result<Vec<Repository>, DatabaseError> {
        let repos = sqlx::query_as!(
            Repository,
            "SELECT * FROM repositories WHERE has_documents_config = true ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(repos)
    }

    // Document operations
    pub async fn upsert_document(&self, doc: &Document) -> Result<Document, DatabaseError> {
        let result = sqlx::query_as!(
            Document,
            r#"
INSERT INTO documents (
    id, repository_id, file_path, title, content, content_hash, metadata,
    file_size, last_modified_at, created_at, updated_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
ON CONFLICT (repository_id, file_path)
DO UPDATE SET
    title = EXCLUDED.title,
    content = EXCLUDED.content,
    content_hash = EXCLUDED.content_hash,
    metadata = EXCLUDED.metadata,
    file_size = EXCLUDED.file_size,
    last_modified_at = EXCLUDED.last_modified_at,
    updated_at = EXCLUDED.updated_at
RETURNING *
            "#,
            doc.id,
            doc.repository_id,
            doc.file_path,
            doc.title,
            doc.content,
            doc.content_hash,
            doc.metadata.as_deref(),
            doc.file_size,
            doc.last_modified_at,
            doc.created_at,
            doc.updated_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn get_documents_by_repository(
        &self,
        repository_id: Uuid,
    ) -> Result<Vec<Document>, DatabaseError> {
        let docs = sqlx::query_as!(
            Document,
            "SELECT * FROM documents WHERE repository_id = $1 ORDER BY file_path",
            repository_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(docs)
    }

    pub async fn get_document_by_path(
        &self,
        repository_id: Uuid,
        file_path: &str,
    ) -> Result<Document, DatabaseError> {
        let result = sqlx::query_as!(
            Document,
            "SELECT * FROM documents WHERE repository_id = $1 AND file_path = $2",
            repository_id,
            file_path
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|_| DatabaseError::DocumentNotFound(format!("{}:{}", repository_id, file_path)))?;

        Ok(result)
    }

    // Processing job operations
    pub async fn create_processing_job(
        &self,
        job: &ProcessingJob,
    ) -> Result<ProcessingJob, DatabaseError> {
        let result = sqlx::query_as!(
            ProcessingJob,
            r#"
            INSERT INTO processing_jobs (
            id, repository_id, job_type, status, parameters, error_message,
            started_at, completed_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING *
            "#,
            job.id,
            job.repository_id,
            job.job_type,
            job.status,
            job.parameters.as_deref(),
            job.error_message.as_deref(),
            job.started_at,
            job.completed_at,
            job.created_at,
            job.updated_at
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn update_job_status(
        &self,
        job_id: Uuid,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), DatabaseError> {
        let now = Utc::now();
        let completed_at = if status == "completed" || status == "failed" {
            Some(now)
        } else {
            None
        };

        sqlx::query!(
            r#"
            UPDATE processing_jobs
            SET status = $2, error_message = $3, completed_at = $4, updated_at = $5
            WHERE id = $1
            "#,
            job_id,
            status,
            error_message,
            completed_at,
            now
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_active_jobs(&self) -> Result<Vec<ProcessingJob>, DatabaseError> {
        let jobs = sqlx::query_as!(
            ProcessingJob,
            "SELECT * FROM processing_jobs WHERE status IN ('pending', 'running') ORDER BY created_at"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(jobs)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::Row;
    use super::*;

    async fn create_test_db() -> Database {
        let db_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://test_user:test_password@localhost:5432/documents_test".to_string()
        });

        let db = Database::new(&db_url)
            .await
            .expect("Failed to create test database");
        db.migrate().await.expect("Failed to migrate test database");
        db
    }

    async fn reset_test_db() -> Result<(), sqlx::Error> {
        let db_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://test_user:test_password@localhost:5432/documents_test".to_string()
        });

        let pool = PgPool::connect(&db_url).await?;
        let tables: Vec<String> = sqlx::query("SELECT tablename FROM pg_tables WHERE schemaname = 'public'")
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect();

        for table in tables {
            sqlx::query(&format!("TRUNCATE TABLE {} RESTART IDENTITY CASCADE", table))
                .execute(&pool)
                .await?;
        }

        pool.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_database_health_check() {
        let db = create_test_db().await;
        assert!(db.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_repository_operations() {
        let db = create_test_db().await;

        let repo = Repository {
            id: Uuid::new_v4(),
            name: "test-repo".to_string(),
            full_name: "test-org/test-repo".to_string(),
            description: Some("A test repository".to_string()),
            default_branch: "main".to_string(),
            is_private: false,
            is_archived: false,
            is_fork: false,
            has_documents_config: true,
            documents_config: Some(r#"{"project": {"name": "Test"}}"#.to_string()),
            last_scanned_at: Some(Utc::now()),
            last_processed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let saved_repo = db
            .upsert_repository(&repo)
            .await
            .expect("Failed to upsert repository");
        assert_eq!(saved_repo.name, repo.name);

        let retrieved_repo = db
            .get_repository_by_full_name(&repo.full_name)
            .await
            .expect("Failed to retrieve repository");

        assert_eq!(retrieved_repo.id, saved_repo.id);

        reset_test_db().await.expect("Failed to reset database");
    }
}
