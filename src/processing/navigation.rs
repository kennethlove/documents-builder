use std::collections::HashMap;
use html_escape::encode_text;
use serde::{Deserialize, Serialize};
use crate::DocumentConfig;
use crate::processing::ProcessedDocument;

/// A display/navigation node (section or document link)
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NavNode {
    pub title: String,
    pub path: Option<String>, // URL; None for purely structural sections
    pub children: Vec<NavNode>,
}

impl NavNode {
    pub fn new(title: String, path: Option<String>) -> Self {
        Self { title, path, children: Vec::new() }
    }
}

/// An artifact that can be stored and served
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NavigationArtifact{
    pub tree: NavNode,
    pub html: String,
}

/// A simplified view of the configuration node we care about for navigation
#[derive(Clone, Debug)]
pub struct ConfigDocNode<'a> {
    pub title: &'a str,
    pub path: Option<&'a str>,
    pub sub_documents: &'a [DocumentConfig], // nested, in-order
}

/// Builder that creates navigation from the ordered config structure.
pub struct NavigationFromConfigBuilder {
    pub url_prefix: String,
    pub include_headings_max_level: Option<u8>,  // e.g., Some(3) to include H2 and H3 headings
}

impl Default for NavigationFromConfigBuilder {
    fn default() -> Self {
        Self {
            url_prefix: String::new(),
            include_headings_max_level: None,
        }
    }
}

impl NavigationFromConfigBuilder {
    /// Build navigation artifact from the ordered config tree and processed docs.
    ///
    /// - config_roots must be in the exact order defined in documents.toml.
    /// - processed_docs is a map keyed by repo-relative file path (ProcessedDocument.file_path).
    pub fn build(
        &self,
        ordered_roots: &[DocumentConfig],
        processed_docs: &HashMap<String, ProcessedDocument>,
    ) -> NavigationArtifact {
        let mut root = NavNode::new("root".into(), None);

        for cfg in ordered_roots {
            let node = self.build_node(&as_cfg_node(cfg), processed_docs);
            root.children.push(node);
        }

        let html = render_html_nav(&root);
        NavigationArtifact { tree: root, html }
    }

    fn build_node(
        &self,
        cfg: &ConfigDocNode<'_>,
        processed_docs: &HashMap<String, ProcessedDocument>,
    ) -> NavNode {
        let url = cfg.path.map(|p| self.url_for(p));
        let mut nav = NavNode::new(cfg.title.to_string(), url.clone());

        // Optional heading children for document nodes
        if let (Some(max_level), Some(path)) = (self.include_headings_max_level, cfg.path) {
            if let Some(doc) = processed_docs.get(path) {
                for h in doc.headings.iter().filter(|h| h.level >= 2 && h.level <= max_level) {
                    nav.children.push(NavNode::new(
                        h.text.clone(),
                        Some(format!("{}#{}", url.as_ref().unwrap_or(&String::new()), h.anchor))
                    ));
                }
            }
        }

        // Recurse into each sub-document
        for child_cfg in cfg.sub_documents {
            let child = self.build_node(&as_cfg_node(child_cfg), processed_docs);
            nav.children.push(child);
        }

        nav
    }

    fn url_for(&self, repo_rel_path: &str) -> String {
        // Convert "dir/index.md" -> "/dir/" and "dir/page.md" -> "/dir/page"
        let mut url = String::new();
        url.push_str(&self.url_prefix);

        let without_extension = repo_rel_path.trim_end_matches(".md");
        if without_extension.ends_with("/index") || repo_rel_path == "index.md" {
            url.push('/');
            url.push_str(without_extension.trim_end_matches("/index"));
            if !url.ends_with('/') {
                url.push('/');
            }
        } else {
            url.push('/');
            url.push_str(without_extension);
        }

        // Normalize doubles
        while url.contains("//") {
            url = url.replace("//", "/");
        }

        if !url.starts_with('/') {
            url.insert(0, '/');
        }

        url
    }
}

fn as_cfg_node(cfg: &DocumentConfig) -> ConfigDocNode<'_> {
    let subs: &[DocumentConfig] = cfg.sub_documents.as_deref().unwrap_or(&[]);
    ConfigDocNode {
        title: &cfg.title,
        path: cfg.path.as_deref().unwrap().to_str(),
        sub_documents: subs,
    }
}

fn render_html_nav(root: &NavNode) -> String {
    // Root is artificial; render its children as the top-level list
    let mut out = String::new();
    // TODO: Replace with a proper template engine
    out.push_str("<nav class=\"docs-nav\">\n");
    out.push_str("  <ul class=\"docs-nav__list\">\n");
    for child in &root.children {
        render_node(child, 2, &mut out);
    }
    out.push_str("  </ul>\n");
    out.push_str("</nav>\n");
    out
}

fn render_node(node: &NavNode, indent: usize, out: &mut String) {
    let indent_str = "  ".repeat(indent);
    let title = encode_text(&node.title);
    // TODO: Replace with a proper template engine
    match &node.path {
        Some(href) => {
            let href_enc = encode_text(href);
            out.push_str(&format!(
                "{indent_str}<li class=\"docs-nav__item\"><a class=\"docs-nav__link\" href=\">{href_enc}\">{title}</a>\n"
            ));
        }
        None => {
            out.push_str(&format!(
                "{indent_str}<li class=\"docs-nav__section\"><span class=\"docs-nav__section-title\">{title}</span>\n"
            ));
        }
    }

    if !node.children.is_empty() {
        out.push('\n');
        out.push_str(&format!("{indent_str}  <ul class=\"docs-nav__list\">\n"));
        for child in &node.children {
            render_node(child, indent + 2, out);
        }
        out.push_str(&format!("{indent_str}  </ul>\n"));
        out.push_str("{indent_str}</li>\n");
    } else {
        out.push_str("</li>\n");
    }
}
