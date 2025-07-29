use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum FragmentType {
    Content,
    Navigation,
    Metadata,
    Index,
    SearchResult,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fragment {
    pub id: String,
    pub repository: String,
    pub fragment_type: FragmentType,
    pub title: String,
    pub content: String,
    pub metadata: FragmentMetadata,
    pub dependencies: Vec<String>,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FragmentMetadata {
    pub path: String,
    pub size: usize,
    pub checksum: String,
    pub tags: Vec<String>,
    pub attributes: HashMap<String, String>,
    pub links: Vec<FragmentLink>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FragmentLink {
    pub target: String,
    pub link_type: LinkType,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum LinkType {
    Internal,
    External,
    Reference,
    Asset,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FragmentCollection {
    pub repository: String,
    pub fragments: Vec<Fragment>,
    pub metadata: CollectionMetadata,
    pub version: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CollectionMetadata {
    pub total_fragments: usize,
    pub fragment_types: HashMap<FragmentType, usize>,
    pub total_size: usize,
    pub last_processed: DateTime<Utc>,
    pub processing_duration: Option<std::time::Duration>,
}

#[allow(dead_code)]
impl Fragment {
    pub fn new(
        id: String,
        repository: String,
        fragment_type: FragmentType,
        title: String,
        content: String,
        path: String,
    ) -> Self {
        let now = Utc::now();
        let checksum = Self::calculate_checksum(&content);

        Self {
            id,
            repository,
            fragment_type,
            title,
            content: content.clone(),
            metadata: FragmentMetadata {
                path,
                size: content.len(),
                checksum,
                tags: Vec::new(),
                attributes: HashMap::new(),
                links: Vec::new(),
            },
            dependencies: Vec::new(),
            version: "1.0.0".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn calculate_checksum(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    pub fn validate(&self) -> Result<(), super::OutputError> {
        if self.id.is_empty() {
            return Err(super::OutputError::Validation(
                "Fragment ID cannot be empty".to_string(),
            ));
        }

        if self.repository.is_empty() {
            return Err(super::OutputError::Validation(
                "Repository cannot be empty".to_string(),
            ));
        }

        if self.content.is_empty() {
            return Err(super::OutputError::Validation(
                "Content cannot be empty".to_string(),
            ));
        }

        let calculated_checksum = Self::calculate_checksum(&self.content);
        if calculated_checksum != self.metadata.checksum {
            return Err(super::OutputError::Validation(
                "Checksum mismatch".to_string(),
            ));
        }

        if self.content.len() != self.metadata.size {
            return Err(super::OutputError::Validation(
                "Content size does not match metadata size".to_string(),
            ));
        }

        Ok(())
    }

    pub fn has_changed(&self, other: &Fragment) -> bool {
        self.metadata.checksum != other.metadata.checksum
            || self.content != other.content
            || self.metadata.size != other.metadata.size
    }

    pub fn add_dependency(&mut self, dependency_id: String) {
        if !self.dependencies.contains(&dependency_id) {
            self.dependencies.push(dependency_id);
        }
    }

    pub fn add_link(&mut self, target: String, link_type: LinkType, title: Option<String>) {
        let link = FragmentLink {
            target,
            link_type,
            title,
        };

        if !self
            .metadata
            .links
            .iter()
            .any(|l| l.target == link.target && l.link_type == link.link_type)
        {
            self.metadata.links.push(link);
        }
    }

    pub fn set_attribute(&mut self, key: String, value: String) {
        self.metadata.attributes.insert(key, value);
    }

    pub fn add_tag(&mut self, tag: String) {
        if !self.metadata.tags.contains(&tag) {
            self.metadata.tags.push(tag);
        }
    }
}

#[allow(dead_code)]
impl FragmentCollection {
    pub fn new(repository: String, fragments: Vec<Fragment>) -> Self {
        let now = Utc::now();
        let total_fragments = fragments.len();
        let total_size = fragments.iter().map(|f| f.metadata.size).sum();

        let mut fragment_types = HashMap::new();
        for fragment in &fragments {
            *fragment_types
                .entry(fragment.fragment_type.clone())
                .or_insert(0) += 1;
        }

        Self {
            repository,
            fragments,
            metadata: CollectionMetadata {
                total_fragments,
                fragment_types,
                total_size,
                last_processed: now,
                processing_duration: None,
            },
            version: "1.0.0".to_string(),
            created_at: now,
        }
    }

    pub fn validate(&self) -> Result<(), super::OutputError> {
        for fragment in &self.fragments {
            fragment.validate()?;
        }

        if self.fragments.len() != self.metadata.total_fragments {
            return Err(super::OutputError::Validation(
                "Total fragments count does not match metadata".to_string(),
            ));
        }

        let calculated_size: usize = self.fragments.iter().map(|f| f.metadata.size).sum();
        if calculated_size != self.metadata.total_size {
            return Err(super::OutputError::Validation(
                "Total size does not match metadata".to_string(),
            ));
        }

        Ok(())
    }

    pub fn get_fragments_by_type(&self, fragment_type: FragmentType) -> Vec<&Fragment> {
        self.fragments
            .iter()
            .filter(|f| f.fragment_type == fragment_type)
            .collect()
    }

    pub fn find_fragment(&self, id: &str) -> Option<&Fragment> {
        self.fragments.iter().find(|f| f.id == id)
    }

    pub fn update_fragment(
        &mut self,
        updated_fragment: Fragment,
    ) -> Result<(), super::OutputError> {
        updated_fragment.validate()?;

        if let Some(pos) = self
            .fragments
            .iter()
            .position(|f| f.id == updated_fragment.id)
        {
            let old_size = self.fragments[pos].metadata.size;
            self.fragments[pos] = updated_fragment;

            self.metadata.total_size =
                self.metadata.total_size - old_size + self.fragments[pos].metadata.size;
            self.metadata.last_processed = Utc::now();
        } else {
            return Err(super::OutputError::Validation(format!(
                "Fragment with ID {} not found",
                updated_fragment.id
            )));
        }

        Ok(())
    }

    pub fn add_fragment(&mut self, fragment: Fragment) -> Result<(), super::OutputError> {
        fragment.validate()?;

        if self.find_fragment(&fragment.id).is_some() {
            return Err(super::OutputError::Validation(format!(
                "Fragment with ID {} already exists",
                fragment.id
            )));
        }

        self.metadata.total_size += fragment.metadata.size;
        self.metadata.total_fragments += 1;
        *self
            .metadata
            .fragment_types
            .entry(fragment.fragment_type.clone())
            .or_insert(0) += 1;

        self.fragments.push(fragment);
        self.metadata.last_processed = Utc::now();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fragment_creation() {
        let fragment = Fragment::new(
            "test-id".to_string(),
            "test/repo".to_string(),
            FragmentType::Content,
            "Test Fragment".to_string(),
            "Test content".to_string(),
            "/test/path.md".to_string(),
        );

        assert_eq!(fragment.id, "test-id");
        assert_eq!(fragment.repository, "test/repo");
        assert_eq!(fragment.fragment_type, FragmentType::Content);
        assert_eq!(fragment.content, "Test content");
        assert_eq!(fragment.metadata.size, 12);
        assert!(!fragment.metadata.checksum.is_empty());
    }

    #[test]
    fn test_fragment_validation() {
        let fragment = Fragment::new(
            "test-id".to_string(),
            "test/repo".to_string(),
            FragmentType::Content,
            "Test Fragment".to_string(),
            "Test content".to_string(),
            "/test/path.md".to_string(),
        );

        assert!(fragment.validate().is_ok());
    }

    #[test]
    fn test_fragment_has_changed() {
        let fragment1 = Fragment::new(
            "test-id".to_string(),
            "test/repo".to_string(),
            FragmentType::Content,
            "Test Fragment".to_string(),
            "Test content".to_string(),
            "/test/path.md".to_string(),
        );

        let fragment2 = Fragment::new(
            "test-id".to_string(),
            "test/repo".to_string(),
            FragmentType::Content,
            "Test Fragment".to_string(),
            "Different content".to_string(),
            "/test/path.md".to_string(),
        );

        assert!(fragment1.has_changed(&fragment2));
        assert!(!fragment1.has_changed(&fragment1));
    }

    #[test]
    fn test_fragment_collection() {
        let fragments = vec![
            Fragment::new(
                "test-1".to_string(),
                "test/repo".to_string(),
                FragmentType::Content,
                "Test 1".to_string(),
                "Content 1".to_string(),
                "/test/1.md".to_string(),
            ),
            Fragment::new(
                "test-2".to_string(),
                "test/repo".to_string(),
                FragmentType::Navigation,
                "Test 2".to_string(),
                "Content 2".to_string(),
                "/test/2.md".to_string(),
            ),
        ];

        let collection = FragmentCollection::new("test/repo".to_string(), fragments);

        assert_eq!(collection.metadata.total_fragments, 2);
        assert_eq!(collection.metadata.total_size, 18); // "Content 1" + "Content 2"
        assert!(collection.validate().is_ok());
    }
}
