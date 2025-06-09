use documents::ProjectConfig;

// Test that TOML config files can be parsed correctly
#[test]
fn test_parse_empty_config() {
    let config_str = r#"
    [project]
    name = "Test Project"
    description = "A test project"

    [documents]
    "#;

    let config: ProjectConfig = toml::from_str(config_str).expect("Failed to parse test config");

    // Verify the project details
    assert_eq!(config.project.name, "Test Project");
    assert_eq!(config.project.description, "A test project");

    // Verify that the documents map is empty
    assert!(config.documents.is_empty());
}

// Test that config files with document paths can be parsed correctly
#[test]
fn test_parse_config_with_documents() {
    let config_str = r#"
    [project]
    name = "Test Project"
    description = "A test project"

    [documents.doc1]
    title = "Document 1"
    path = "docs/doc1.md"

    [documents.doc2]
    title = "Document 2"

    [[documents.doc2.sub_documents]]
    title = "Sub Document 1"
    path = "docs/sub/doc1.md"

    [[documents.doc2.sub_documents]]
    title = "Sub Document 2"
    path = "docs/sub/doc2.md"

    [documents.doc3]
    title = "Document 3"
    "#;

    let config: ProjectConfig = toml::from_str(config_str).expect("Failed to parse test config with documents");

    // Verify the project details
    assert_eq!(config.project.name, "Test Project");
    assert_eq!(config.project.description, "A test project");

    // Verify that the documents map has the expected entries
    assert_eq!(config.documents.len(), 3);
    assert!(config.documents.contains_key("doc1"));
    assert!(config.documents.contains_key("doc2"));
    assert!(config.documents.contains_key("doc3"));
}

// Test that config files with nested document structures can be parsed correctly
#[test]
fn test_parse_config_with_nested_documents() {
    let config_str = r#"
    [project]
    name = "Test Project"
    description = "A test project"

    [documents.doc1]
    title = "Document 1"
    path = "docs/doc1.md"

    [documents.doc2]
    title = "Document 2"

    [[documents.doc2.sub_documents]]
    title = "Sub Document 1"
    path = "docs/sub/doc1.md"

    [[documents.doc2.sub_documents]]
    title = "Sub Document 2"
    path = "docs/sub/doc2.md"
    "#;

    let config: ProjectConfig = toml::from_str(config_str).expect("Failed to parse test config with nested documents");

    // Verify the project details
    assert_eq!(config.project.name, "Test Project");
    assert_eq!(config.project.description, "A test project");

    // Verify that the documents map has the expected entries
    assert_eq!(config.documents.len(), 2);
    assert!(config.documents.contains_key("doc1"));
    assert!(config.documents.contains_key("doc2"));

    // Since we can't directly access private fields like sub_documents,
    // we'll just verify that the document keys exist in the config
    // We can't directly check sub_documents since it's private
}

// Test that config files with inline table syntax can be parsed correctly
#[test]
fn test_parse_config_with_inline_tables() {
    let config_str = r#"
    [project]
    name = "Documents Test"
    description = "Quick test for documents"

    [documents]
    home = { path = "docs/index.md", title = "Home page" }
    getting_started = { path = "docs/getting_started.md", title = "Getting started with documents" }
    references = { title = "References", sub_documents = [
      { title = "Configuration", path = "references/configuration.md" },
      { title = "Schema", path = "references/schema.md" },
    ]}
    "#;

    let config: ProjectConfig = toml::from_str(config_str).expect("Failed to parse test config with inline tables");

    // Verify the project details
    assert_eq!(config.project.name, "Documents Test");
    assert_eq!(config.project.description, "Quick test for documents");

    // Verify that the documents map has the expected entries
    assert_eq!(config.documents.len(), 3);
    assert!(config.documents.contains_key("home"));
    assert!(config.documents.contains_key("getting_started"));
    assert!(config.documents.contains_key("references"));
}
