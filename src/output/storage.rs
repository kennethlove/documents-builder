use crate::output::fragment::{Fragment, FragmentCollection};
use crate::output::{OutputConfig, OutputError, OutputFormat, Result};
use async_trait::async_trait;
use futures_lite::stream::StreamExt;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait Storage: Send + Sync {
    async fn save_fragment(&self, fragment: &Fragment) -> Result<()>;
    async fn load_fragment(&self, repository: &str, id: &str) -> Result<Option<Fragment>>;
    async fn save_collection(&self, collection: &FragmentCollection) -> Result<()>;
    async fn load_collection(&self, repository: &str) -> Result<Option<FragmentCollection>>;
    async fn list_repositories(&self) -> Result<Vec<String>>;
    async fn list_fragments(&self, repository: &str) -> Result<Vec<String>>;
    async fn delete_fragment(&self, repository: &str, id: &str) -> Result<()>;
    async fn delete_collection(&self, repository: &str) -> Result<()>;
    async fn fragment_exists(&self, repository: &str, id: &str) -> Result<bool>;
}

#[derive(Debug)]
pub struct FileSystemStorage {
    base_path: PathBuf,
    config: OutputConfig,
}

impl FileSystemStorage {
    pub fn new(base_path: PathBuf, config: OutputConfig) -> Result<Self> {
        // Create the base path if it doesn't exist
        if !base_path.exists() {
            std::fs::create_dir_all(&base_path)?;
        }

        Ok(Self { base_path, config })
    }

    fn get_repository_path(&self, repository: &str) -> PathBuf {
        let safe_repo_name = repository.replace(['/', '\\', ':'], "_");
        self.base_path.join(safe_repo_name)
    }

    fn get_fragment_path(&self, repository: &str, id: &str, format: &str) -> PathBuf {
        let repo_path = self.get_repository_path(repository);
        repo_path.join(format!("fragments/{}.{}", id, format))
    }

    fn get_collection_path(&self, repository: &str, format: &str) -> PathBuf {
        let repo_path = self.get_repository_path(repository);
        repo_path.join(format!("collection.{}", format))
    }

    async fn ensure_directory(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                async_fs::create_dir_all(parent).await?;
            }
        }
        Ok(())
    }

    async fn save_as_json<T: serde::Serialize>(&self, path: &Path, data: &T) -> Result<()> {
        self.ensure_directory(path).await?;
        let json = serde_json::to_string(data)?;
        async_fs::write(path, json).await?;
        Ok(())
    }

    async fn save_as_html(&self, path: &Path, content: &str) -> Result<()> {
        self.ensure_directory(path).await?;
        async_fs::write(path, content).await?;
        Ok(())
    }

    async fn load_from_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &Path,
    ) -> Result<Option<T>> {
        if !path.exists() {
            return Ok(None);
        }
        let data = async_fs::read_to_string(path).await?;
        let parsed = serde_json::from_str(&data)?;
        Ok(Some(parsed))
    }
}

#[async_trait]
impl Storage for FileSystemStorage {
    async fn save_fragment(&self, fragment: &Fragment) -> Result<()> {
        fragment.validate()?;

        match self.config.format {
            OutputFormat::Json => {
                let path = self.get_fragment_path(&fragment.repository, &fragment.id, "json");
                self.save_as_json(&path, fragment).await?;
            }
            OutputFormat::Html => {
                let path = self.get_fragment_path(&fragment.repository, &fragment.id, "html");
                self.save_as_html(&path, &fragment.content).await?;
            }
            OutputFormat::Both => {
                let json_path = self.get_fragment_path(&fragment.repository, &fragment.id, "json");
                self.save_as_json(&json_path, fragment).await?;

                let html_path = self.get_fragment_path(&fragment.repository, &fragment.id, "html");
                self.save_as_html(&html_path, &fragment.content).await?;
            }
        }

        Ok(())
    }

    async fn load_fragment(&self, repository: &str, id: &str) -> Result<Option<Fragment>> {
        let json_path = self.get_fragment_path(repository, id, "json");
        self.load_from_json(&json_path).await
    }

    async fn save_collection(&self, collection: &FragmentCollection) -> Result<()> {
        collection.validate()?;

        match self.config.format {
            OutputFormat::Json => {
                let path = self.get_collection_path(&collection.repository, "json");
                self.save_as_json(&path, collection).await?;
            }
            OutputFormat::Html => {
                for fragment in &collection.fragments {
                    self.save_fragment(fragment).await?;
                }

                let index_html = self.generate_collection_index_html(&collection);
                let index_path = self.get_collection_path(&collection.repository, "html");
                self.save_as_html(&index_path, &index_html).await?;
            }
            OutputFormat::Both => {
                let json_path = self.get_collection_path(&collection.repository, "json");
                self.save_as_json(&json_path, collection).await?;

                for fragment in &collection.fragments {
                    self.save_fragment(fragment).await?;
                }

                let index_html = self.generate_collection_index_html(&collection);
                let index_path = self.get_collection_path(&collection.repository, "html");
                self.save_as_html(&index_path, &index_html).await?;
            }
        }

        Ok(())
    }

    async fn load_collection(&self, repository: &str) -> Result<Option<FragmentCollection>> {
        let path = self.get_collection_path(repository, "json");
        self.load_from_json(&path).await
    }

    async fn list_repositories(&self) -> Result<Vec<String>> {
        let mut repositories = Vec::new();

        if !self.base_path.exists() {
            return Ok(repositories);
        }

        let mut entries = async_fs::read_dir(&self.base_path).await?;
        while let Some(entry) = entries.try_next().await? {
            if entry.file_type().await?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    let repo_name = name.replace('_', "/");
                    repositories.push(repo_name);
                }
            }
        }
        Ok(repositories)
    }

    async fn list_fragments(&self, repository: &str) -> Result<Vec<String>> {
        let fragments_path = self.get_repository_path(repository).join("fragments");
        let mut fragment_ids = Vec::new();

        if !fragments_path.exists() {
            return Ok(fragment_ids);
        }

        let mut entries = async_fs::read_dir(&fragments_path).await?;
        while let Some(entry) = entries.try_next().await? {
            if entry.file_type().await?.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        let id = name.trim_end_matches(".json");
                        fragment_ids.push(id.to_string());
                    }
                }
            }
        }

        Ok(fragment_ids)
    }

    async fn delete_fragment(&self, repository: &str, id: &str) -> Result<()> {
        let json_path = self.get_fragment_path(repository, id, "json");
        let html_path = self.get_fragment_path(repository, id, "html");

        if json_path.exists() {
            async_fs::remove_file(&json_path).await?;
        }
        if html_path.exists() {
            async_fs::remove_file(&html_path).await?;
        }

        Ok(())
    }

    async fn delete_collection(&self, repository: &str) -> Result<()> {
        let repo_path = self.get_repository_path(repository);
        if repo_path.exists() {
            async_fs::remove_dir_all(&repo_path).await?;
        }
        Ok(())
    }

    async fn fragment_exists(&self, repository: &str, id: &str) -> Result<bool> {
        let json_path = self.get_fragment_path(repository, id, "json");
        Ok(json_path.exists())
    }
}

impl FileSystemStorage {
    fn generate_collection_index_html(&self, collection: &FragmentCollection) -> String {
        let mut html = String::new();

        html.push_str(&format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{} - Documentation</title>
</head>
<body>
    <header>
        <h1>{}</h1>
        <p>Generated on {} - Version {}</p>
    </header>
    <section class="stats">
        <dl class="stat">
            <dd>{}</dd>
            <dt>Total Fragments</dt>
            <dd>{}</dd>
            <dt>Total Size (bytes)</dt>
        </dl>
    </section>
    <section class="fragments">
"#,
            html_escape::encode_text(&collection.repository),
            html_escape::encode_text(&collection.repository),
            collection.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
            html_escape::encode_text(&collection.version),
            collection.metadata.total_fragments,
            collection.metadata.total_size,
        ));

        for fragment in &collection.fragments {
            html.push_str(&format!(
                r#"
            <article>
                <h2>{}</h2>
                <p>{:?}</p>
                <dl>
                    <dt>ID</dt>
                    <dd>{}</dd>
                    <dt>Path</dt>
                    <dd>{}</dd>
                    <dt>Size</dt>
                    <dd>{} bytes</dd>
                    <dt>Updated At</dt>
                    <dd>{}</dd>
                </dl>
            </article>
"#,
                html_escape::encode_text(&fragment.title),
                &fragment.fragment_type,
                html_escape::encode_text(&fragment.id),
                html_escape::encode_text(&fragment.metadata.path),
                &fragment.metadata.size,
                &fragment.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
            ));
        }

        html.push_str(
            r#"
        </section>
    </body>
</html>
"#,
        );

        html
    }
}

pub struct StorageManager {
    storage: Box<dyn Storage>,
}

#[allow(dead_code)]
impl StorageManager {
    pub fn new(config: OutputConfig) -> Result<Self> {
        let storage: Box<dyn Storage> = match config.storage_type {
            crate::output::StorageType::FileSystem => {
                let base_path = config
                    .base_path
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("./output"));
                Box::new(FileSystemStorage::new(base_path, config)?)
            }
            crate::output::StorageType::Database => {
                // Placeholder for database storage implementation
                unimplemented!("Database storage is not yet implemented");
            }
            crate::output::StorageType::Hybrid => {
                // Placeholder for hybrid storage implementation
                unimplemented!("Hybrid storage is not yet implemented");
            }
        };
        Ok(Self { storage })
    }

    pub async fn save_fragment(&self, fragment: &Fragment) -> Result<()> {
        self.storage.save_fragment(fragment).await
    }

    pub async fn load_fragment(&self, repository: &str, id: &str) -> Result<Option<Fragment>> {
        self.storage.load_fragment(repository, id).await
    }

    pub async fn save_collection(&self, collection: &FragmentCollection) -> Result<()> {
        self.storage.save_collection(collection).await
    }

    pub async fn load_collection(&self, repository: &str) -> Result<Option<FragmentCollection>> {
        self.storage.load_collection(repository).await
    }

    pub async fn list_repositories(&self) -> Result<Vec<String>> {
        self.storage.list_repositories().await
    }

    pub async fn list_fragments(&self, repository: &str) -> Result<Vec<String>> {
        self.storage.list_fragments(repository).await
    }

    pub async fn delete_fragment(&self, repository: &str, id: &str) -> Result<()> {
        self.storage.delete_fragment(repository, id).await
    }

    pub async fn delete_collection(&self, repository: &str) -> Result<()> {
        self.storage.delete_collection(repository).await
    }

    pub async fn fragment_exists(&self, repository: &str, id: &str) -> Result<bool> {
        self.storage.fragment_exists(repository, id).await
    }

    pub async fn export_collection(&self, repository: &str, export_path: &Path) -> Result<()> {
        let collection = self.load_collection(repository).await?.ok_or_else(|| {
            OutputError::Storage(format!(
                "Collection not found for repository: {}",
                repository
            ))
        })?;

        let export_data = serde_json::to_string_pretty(&collection)?;
        async_fs::write(export_path, &export_data).await?;

        Ok(())
    }

    pub async fn import_collection(&self, import_path: &Path) -> Result<()> {
        let content = async_fs::read_to_string(import_path).await?;
        let collection: FragmentCollection = serde_json::from_str(&content)?;

        collection.validate()?;
        self.save_collection(&collection).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::output::fragment::{Fragment, FragmentCollection, FragmentType};
    use crate::output::storage::{FileSystemStorage, Storage, StorageManager};
    use crate::output::{OutputConfig, OutputFormat};

    fn create_test_config(temp_dir: &tempfile::TempDir) -> OutputConfig {
        OutputConfig {
            storage_type: crate::output::StorageType::FileSystem,
            base_path: Some(temp_dir.path().to_path_buf()),
            format: OutputFormat::Both,
            enable_versioning: true,
            enable_compression: false,
        }
    }

    #[tokio::test]
    async fn test_filesystem_storage() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = create_test_config(&temp_dir);
        let storage = FileSystemStorage::new(temp_dir.path().to_path_buf(), config).unwrap();

        let fragment = Fragment::new(
            "test-fragment".to_string(),
            "test/repo".to_string(),
            FragmentType::Content,
            "Test Fragment".to_string(),
            "<h1>Test content</h1>".to_string(),
            "/test.md".to_string(),
        );

        // Test save and load
        storage.save_fragment(&fragment).await.unwrap();
        let loaded = storage
            .load_fragment("test/repo", "test-fragment")
            .await
            .unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, "test-fragment");

        // Test fragment exists
        assert!(
            storage
                .fragment_exists("test/repo", "test-fragment")
                .await
                .unwrap()
        );
        assert!(
            !storage
                .fragment_exists("test/repo", "nonexistent")
                .await
                .unwrap()
        );

        // Test list fragments
        let fragments = storage.list_fragments("test/repo").await.unwrap();
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0], "test-fragment");
    }

    #[tokio::test]
    async fn test_storage_manager() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = create_test_config(&temp_dir);
        let manager = StorageManager::new(config).unwrap();

        let fragments = vec![
            Fragment::new(
                "fragment-1".to_string(),
                "test/repo".to_string(),
                FragmentType::Content,
                "Fragment 1".to_string(),
                "<h1>Fragment 1 content</h1>".to_string(),
                "/doc-1.md".to_string(),
            ),
            Fragment::new(
                "fragment-2".to_string(),
                "test/repo".to_string(),
                FragmentType::Navigation,
                "Fragment 2".to_string(),
                "<nav>Fragment 2 content</nav>".to_string(),
                "/nav.md".to_string(),
            ),
        ];

        let collection = FragmentCollection::new("test/repo".to_string(), fragments);

        // Test save and load collection
        manager.save_collection(&collection).await.unwrap();
        let loaded_collection = manager.load_collection("test/repo").await.unwrap();
        assert!(loaded_collection.is_some());
        assert_eq!(loaded_collection.unwrap().fragments.len(), 2);

        // Test list repositories
        let repos = manager.list_repositories().await.unwrap();
        assert_eq!(repos.len(), 1);
        assert!(repos.contains(&"test/repo".to_string()));

        // Test export
        let export_path = temp_dir.path().join("exported_collection.json");
        manager
            .export_collection("test/repo", &export_path)
            .await
            .unwrap();
        assert!(export_path.exists());

        // Delete and re-import
        manager.delete_collection("test/repo").await.unwrap();
        assert!(
            manager
                .load_collection("test/repo")
                .await
                .unwrap()
                .is_none()
        );

        manager.import_collection(&export_path).await.unwrap();
        let reimported = manager.load_collection("test/repo").await.unwrap();
        assert!(reimported.is_some());
        assert_eq!(reimported.unwrap().fragments.len(), 2);
    }
}
