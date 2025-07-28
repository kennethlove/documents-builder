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

    let config: ProjectConfig =
        toml::from_str(config_str).expect("Failed to parse test config with documents");

    // Verify the project details
    assert_eq!(config.project.name, "Test Project");
    assert_eq!(config.project.description, "A test project");

    // Verify that the documents map has the expected entries
    assert_eq!(config.documents.len(), 3);
    assert!(config.documents.contains_key("doc1"));
    assert!(config.documents.contains_key("doc2"));
    assert!(config.documents.contains_key("doc3"));

    // Now that fields are public, we can directly verify document properties
    let doc1 = &config.documents["doc1"];
    assert_eq!(doc1.title, "Document 1");
    assert_eq!(
        doc1.path.as_ref().unwrap().to_str().unwrap(),
        "docs/doc1.md"
    );
    assert!(doc1.sub_documents.is_none());

    let doc2 = &config.documents["doc2"];
    assert_eq!(doc2.title, "Document 2");
    assert!(doc2.path.is_none());
    assert!(doc2.sub_documents.is_some());

    // Verify sub-documents
    let sub_docs = doc2.sub_documents.as_ref().unwrap();
    assert_eq!(sub_docs.len(), 2);
    assert_eq!(sub_docs[0].title, "Sub Document 1");
    assert_eq!(
        sub_docs[0].path.as_ref().unwrap().to_str().unwrap(),
        "docs/sub/doc1.md"
    );
    assert_eq!(sub_docs[1].title, "Sub Document 2");
    assert_eq!(
        sub_docs[1].path.as_ref().unwrap().to_str().unwrap(),
        "docs/sub/doc2.md"
    );

    let doc3 = &config.documents["doc3"];
    assert_eq!(doc3.title, "Document 3");
    assert!(doc3.path.is_none());
    assert!(doc3.sub_documents.is_none());
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

    let config: ProjectConfig =
        toml::from_str(config_str).expect("Failed to parse test config with nested documents");

    // Verify the project details
    assert_eq!(config.project.name, "Test Project");
    assert_eq!(config.project.description, "A test project");

    // Verify that the documents map has the expected entries
    assert_eq!(config.documents.len(), 2);
    assert!(config.documents.contains_key("doc1"));
    assert!(config.documents.contains_key("doc2"));

    // Now that fields are public, we can directly verify the nested document structure
    let doc1 = &config.documents["doc1"];
    assert_eq!(doc1.title, "Document 1");
    assert_eq!(
        doc1.path.as_ref().unwrap().to_str().unwrap(),
        "docs/doc1.md"
    );
    assert!(doc1.sub_documents.is_none());

    let doc2 = &config.documents["doc2"];
    assert_eq!(doc2.title, "Document 2");
    assert!(doc2.path.is_none());

    // Verify the sub-documents structure
    let sub_docs = doc2
        .sub_documents
        .as_ref()
        .expect("doc2 should have sub_documents");
    assert_eq!(sub_docs.len(), 2);

    // Verify first sub-document
    assert_eq!(sub_docs[0].title, "Sub Document 1");
    assert_eq!(
        sub_docs[0].path.as_ref().unwrap().to_str().unwrap(),
        "docs/sub/doc1.md"
    );
    assert!(sub_docs[0].sub_documents.is_none());

    // Verify second sub-document
    assert_eq!(sub_docs[1].title, "Sub Document 2");
    assert_eq!(
        sub_docs[1].path.as_ref().unwrap().to_str().unwrap(),
        "docs/sub/doc2.md"
    );
    assert!(sub_docs[1].sub_documents.is_none());
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

    let config: ProjectConfig =
        toml::from_str(config_str).expect("Failed to parse test config with inline tables");

    // Verify the project details
    assert_eq!(config.project.name, "Documents Test");
    assert_eq!(config.project.description, "Quick test for documents");

    // Verify that the documents map has the expected entries
    assert_eq!(config.documents.len(), 3);
    assert!(config.documents.contains_key("home"));
    assert!(config.documents.contains_key("getting_started"));
    assert!(config.documents.contains_key("references"));

    // Now that fields are public, we can directly verify the inline table structure

    // Verify home document
    let home = &config.documents["home"];
    assert_eq!(home.title, "Home page");
    assert_eq!(
        home.path.as_ref().unwrap().to_str().unwrap(),
        "docs/index.md"
    );
    assert!(home.sub_documents.is_none());

    // Verify getting_started document
    let getting_started = &config.documents["getting_started"];
    assert_eq!(getting_started.title, "Getting started with documents");
    assert_eq!(
        getting_started.path.as_ref().unwrap().to_str().unwrap(),
        "docs/getting_started.md"
    );
    assert!(getting_started.sub_documents.is_none());

    // Verify references document with its sub-documents
    let references = &config.documents["references"];
    assert_eq!(references.title, "References");
    assert!(references.path.is_none());

    // Verify sub-documents in references
    let sub_docs = references
        .sub_documents
        .as_ref()
        .expect("references should have sub_documents");
    assert_eq!(sub_docs.len(), 2);

    // Verify first sub-document (Configuration)
    assert_eq!(sub_docs[0].title, "Configuration");
    assert_eq!(
        sub_docs[0].path.as_ref().unwrap().to_str().unwrap(),
        "references/configuration.md"
    );
    assert!(sub_docs[0].sub_documents.is_none());

    // Verify second sub-document (Schema)
    assert_eq!(sub_docs[1].title, "Schema");
    assert_eq!(
        sub_docs[1].path.as_ref().unwrap().to_str().unwrap(),
        "references/schema.md"
    );
    assert!(sub_docs[1].sub_documents.is_none());
}
