# Version Tracking Strategy: Hybrid Approach with Delta Storage

## Overview

This document outlines the recommended hybrid approach for version tracking in the Documents Platform, combining local database storage with intelligent optimization strategies to balance performance, storage efficiency, and reliability.

## Architecture Summary

The hybrid approach provides:
- **Hot storage** for recent versions (full content, fast access)
- **Delta storage** for older versions (compressed diffs, space efficient)
- **Content deduplication** to minimize storage overhead
- **Configurable retention** policies based on document importance
- **Optional GitHub fallback** for very old versions

## Database Schema

### Core Tables

```sql
-- Main document versions table
CREATE TABLE document_versions (
    id SERIAL PRIMARY KEY,
    document_id INTEGER NOT NULL,
    version INTEGER NOT NULL,
    content_hash VARCHAR(64),
    content TEXT,              -- NULL for delta-only versions
    content_compressed BYTEA,  -- NULL for uncompressed
    is_delta BOOLEAN DEFAULT FALSE,
    base_version_id INTEGER,   -- For delta versions
    delta_patch TEXT,          -- JSON patch for delta versions
    metadata JSONB,
    created_at TIMESTAMP DEFAULT NOW(),
    UNIQUE(document_id, version),
    FOREIGN KEY (document_id) REFERENCES documents(id),
    FOREIGN KEY (base_version_id) REFERENCES document_versions(id)
);

-- Content deduplication table
CREATE TABLE content_blocks (
    content_hash VARCHAR(64) PRIMARY KEY,
    content TEXT NOT NULL,
    size INTEGER NOT NULL,
    reference_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Version tracking configuration per document
CREATE TABLE document_version_config (
    document_id INTEGER PRIMARY KEY,
    hot_versions INTEGER DEFAULT 10,
    delta_versions INTEGER DEFAULT 50,
    total_versions INTEGER DEFAULT 100,
    compress_after_days INTEGER DEFAULT 7,
    archive_after_days INTEGER DEFAULT 90,
    importance_level VARCHAR(20) DEFAULT 'normal', -- 'critical', 'normal', 'low'
    FOREIGN KEY (document_id) REFERENCES documents(id)
);

-- Performance indexes
CREATE INDEX idx_document_versions_doc_id ON document_versions(document_id);
CREATE INDEX idx_document_versions_created_at ON document_versions(created_at);
CREATE INDEX idx_document_versions_is_delta ON document_versions(is_delta);
CREATE INDEX idx_content_blocks_size ON content_blocks(size);
CREATE INDEX idx_content_blocks_ref_count ON content_blocks(reference_count);
```


## Rust Implementation

### Core Version Manager

```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct VersionConfig {
    pub hot_versions: u32,
    pub delta_versions: u32,
    pub total_versions: u32,
    pub compress_after_days: u32,
    pub archive_after_days: u32,
    pub importance_level: ImportanceLevel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportanceLevel {
    Critical,  // README, API docs, etc.
    Normal,    // Regular documentation
    Low,       // Auto-generated, temporary docs
}

impl Default for VersionConfig {
    fn default() -> Self {
        Self {
            hot_versions: 10,
            delta_versions: 50,
            total_versions: 100,
            compress_after_days: 7,
            archive_after_days: 90,
            importance_level: ImportanceLevel::Normal,
        }
    }
}

pub struct VersionManager {
    pool: PgPool,
    config_cache: RwLock<HashMap<i64, VersionConfig>>,
    github_fallback: bool,
}

impl VersionManager {
    pub fn new(pool: PgPool, github_fallback: bool) -> Self {
        Self {
            pool,
            config_cache: RwLock::new(HashMap::new()),
            github_fallback,
        }
    }

    /// Store a new version of a document
    pub async fn store_version(
        &self,
        document_id: i64,
        content: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<i64, VersionError> {
        let mut tx = self.pool.begin().await?;
        
        // Get next version number
        let version = self.get_next_version(&mut tx, document_id).await?;
        
        // Check if content already exists (deduplication)
        let content_hash = self.calculate_hash(content);
        let existing_block = self.get_content_block(&mut tx, &content_hash).await?;
        
        let version_id = if let Some(block) = existing_block {
            // Content exists, just reference it
            self.increment_reference_count(&mut tx, &content_hash).await?;
            self.create_version_reference(&mut tx, document_id, version, &content_hash, metadata).await?
        } else {
            // New content, decide storage strategy
            let config = self.get_version_config(document_id).await?;
            
            if version <= config.hot_versions {
                // Store in hot storage (full content)
                self.store_hot_version(&mut tx, document_id, version, content, &content_hash, metadata).await?
            } else {
                // Store as delta
                self.store_delta_version(&mut tx, document_id, version, content, &content_hash, metadata).await?
            }
        };
        
        tx.commit().await?;
        
        // Background cleanup
        tokio::spawn(async move {
            let _ = self.cleanup_old_versions(document_id).await;
        });
        
        Ok(version_id)
    }

    /// Retrieve a specific version
    pub async fn get_version(&self, document_id: i64, version: i64) -> Result<String, VersionError> {
        // Try hot storage first
        if let Some(content) = self.get_hot_version(document_id, version).await? {
            return Ok(content);
        }
        
        // Try delta reconstruction
        if let Some(content) = self.reconstruct_from_deltas(document_id, version).await? {
            return Ok(content);
        }
        
        // Fallback to GitHub if enabled
        if self.github_fallback {
            return self.fetch_from_github(document_id, version).await;
        }
        
        Err(VersionError::NotFound)
    }

    /// Get version history for a document
    pub async fn get_version_history(
        &self,
        document_id: i64,
        limit: Option<u32>,
    ) -> Result<Vec<VersionInfo>, VersionError> {
        let limit = limit.unwrap_or(50);
        
        let versions = sqlx::query_as!(
            VersionInfo,
            r#"
            SELECT 
                id,
                version,
                content_hash,
                is_delta,
                metadata,
                created_at
            FROM document_versions 
            WHERE document_id = $1 
            ORDER BY version DESC 
            LIMIT $2
            "#,
            document_id,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(versions)
    }

    /// Compare two versions
    pub async fn compare_versions(
        &self,
        document_id: i64,
        version_a: i64,
        version_b: i64,
    ) -> Result<VersionDiff, VersionError> {
        let content_a = self.get_version(document_id, version_a).await?;
        let content_b = self.get_version(document_id, version_b).await?;
        
        let diff = self.generate_diff(&content_a, &content_b)?;
        
        Ok(VersionDiff {
            from_version: version_a,
            to_version: version_b,
            diff,
            stats: self.calculate_diff_stats(&content_a, &content_b),
        })
    }

    // Private implementation methods
    async fn get_hot_version(&self, document_id: i64, version: i64) -> Result<Option<String>, VersionError> {
        let result = sqlx::query!(
            r#"
            SELECT dv.content, cb.content as dedup_content
            FROM document_versions dv
            LEFT JOIN content_blocks cb ON dv.content_hash = cb.content_hash
            WHERE dv.document_id = $1 AND dv.version = $2 AND dv.is_delta = false
            "#,
            document_id,
            version
        )
        .fetch_optional(&self.pool)
        .await?;
        
        match result {
            Some(row) => {
                let content = row.content.or(row.dedup_content);
                Ok(content)
            }
            None => Ok(None),
        }
    }

    async fn reconstruct_from_deltas(
        &self,
        document_id: i64,
        target_version: i64,
    ) -> Result<Option<String>, VersionError> {
        // Find the base version (latest non-delta version before target)
        let base_version = self.find_base_version(document_id, target_version).await?;
        
        if let Some(base) = base_version {
            let mut content = self.get_hot_version(document_id, base.version).await?
                .ok_or(VersionError::BaseVersionNotFound)?;
            
            // Apply deltas in sequence
            let deltas = self.get_deltas_between(document_id, base.version, target_version).await?;
            
            for delta in deltas {
                content = self.apply_delta(&content, &delta.delta_patch)?;
            }
            
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }

    async fn cleanup_old_versions(&self, document_id: i64) -> Result<(), VersionError> {
        let config = self.get_version_config(document_id).await?;
        
        // Remove versions beyond retention limit
        let versions_to_remove = sqlx::query!(
            r#"
            SELECT id, content_hash
            FROM document_versions
            WHERE document_id = $1
            ORDER BY version DESC
            OFFSET $2
            "#,
            document_id,
            config.total_versions as i64
        )
        .fetch_all(&self.pool)
        .await?;
        
        for version in versions_to_remove {
            // Decrement reference count for content blocks
            if let Some(hash) = version.content_hash {
                self.decrement_reference_count(&hash).await?;
            }
            
            // Remove version record
            sqlx::query!(
                "DELETE FROM document_versions WHERE id = $1",
                version.id
            )
            .execute(&self.pool)
            .await?;
        }
        
        // Clean up unreferenced content blocks
        sqlx::query!(
            "DELETE FROM content_blocks WHERE reference_count <= 0"
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }

    fn calculate_hash(&self, content: &str) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionInfo {
    pub id: i64,
    pub version: i64,
    pub content_hash: Option<String>,
    pub is_delta: bool,
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionDiff {
    pub from_version: i64,
    pub to_version: i64,
    pub diff: String,
    pub stats: DiffStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiffStats {
    pub lines_added: u32,
    pub lines_removed: u32,
    pub lines_changed: u32,
    pub similarity_percent: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Version not found")]
    NotFound,
    #[error("Base version not found for delta reconstruction")]
    BaseVersionNotFound,
    #[error("Delta application failed: {0}")]
    DeltaApplicationFailed(String),
    #[error("GitHub API error: {0}")]
    GitHubError(String),
}
```


## Configuration

### Application Configuration

```toml
# config/documents.toml
[version_tracking]
enabled = true
github_fallback = true
default_hot_versions = 10
default_delta_versions = 50
default_total_versions = 100
compress_after_days = 7
archive_after_days = 90

[version_tracking.importance_levels]
critical = { hot_versions = 25, delta_versions = 100, total_versions = 200 }
normal = { hot_versions = 10, delta_versions = 50, total_versions = 100 }
low = { hot_versions = 3, delta_versions = 20, total_versions = 50 }

[storage]
compression_algorithm = "zstd"  # or "gzip"
max_content_size = "10MB"
deduplication_enabled = true
```


### Document-Specific Configuration

```rust
// Configure version tracking for specific documents
pub async fn configure_document_versioning(
    version_manager: &VersionManager,
    document_id: i64,
    config: VersionConfig,
) -> Result<(), VersionError> {
    sqlx::query!(
        r#"
        INSERT INTO document_version_config 
        (document_id, hot_versions, delta_versions, total_versions, compress_after_days, archive_after_days, importance_level)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (document_id) DO UPDATE SET
            hot_versions = $2,
            delta_versions = $3,
            total_versions = $4,
            compress_after_days = $5,
            archive_after_days = $6,
            importance_level = $7
        "#,
        document_id,
        config.hot_versions as i32,
        config.delta_versions as i32,
        config.total_versions as i32,
        config.compress_after_days as i32,
        config.archive_after_days as i32,
        config.importance_level.to_string()
    )
    .execute(&version_manager.pool)
    .await?;
    
    Ok(())
}
```


## Usage Examples

### Basic Version Operations

```rust
use version_manager::{VersionManager, VersionConfig, ImportanceLevel};

// Initialize version manager
let version_manager = VersionManager::new(pool, true);

// Store a new version
let version_id = version_manager.store_version(
    document_id,
    "# Updated Documentation\n\nThis is the new content.",
    Some(serde_json::json!({
        "author": "john@example.com",
        "commit_sha": "abc123",
        "branch": "main"
    }))
).await?;

// Retrieve a specific version
let content = version_manager.get_version(document_id, 5).await?;

// Get version history
let history = version_manager.get_version_history(document_id, Some(20)).await?;

// Compare versions
let diff = version_manager.compare_versions(document_id, 4, 5).await?;
println!("Changes: +{} -{} lines", diff.stats.lines_added, diff.stats.lines_removed);

// Configure version tracking for important documents
let critical_config = VersionConfig {
    hot_versions: 25,
    delta_versions: 100,
    total_versions: 200,
    importance_level: ImportanceLevel::Critical,
    ..Default::default()
};

configure_document_versioning(&version_manager, readme_doc_id, critical_config).await?;
```


## Benefits

1. **Performance**: Recent versions accessed instantly from hot storage
2. **Efficiency**: Delta compression reduces storage by 70-90% for text documents
3. **Deduplication**: Identical content stored only once across all documents
4. **Flexibility**: Configurable retention policies per document importance
5. **Reliability**: No dependency on external services for core functionality
6. **Scalability**: Storage grows linearly with actual changes, not versions

## Migration Strategy

1. **Phase 1**: Implement basic version storage with hot storage only
2. **Phase 2**: Add delta compression for older versions
3. **Phase 3**: Implement content deduplication
4. **Phase 4**: Add GitHub fallback for historical versions
5. **Phase 5**: Add advanced features (compression, archival)

This approach provides a robust foundation that can scale with your documentation platform while maintaining excellent performance and storage efficiency.
