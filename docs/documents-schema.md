# documents.toml Schema Documentation

This document describes the schema for the `documents.toml` configuration file used by the documents-builder project.

## Overview

The `documents.toml` file is a TOML configuration file that defines the structure and metadata of your project's documentation. It should be placed at the root of your project repository. The configuration specifies:

1. Basic project information
2. A collection of documents with their titles, paths, and hierarchical relationships

## Schema Structure

### Top-level Sections

The `documents.toml` file must contain the following top-level sections:

```toml
[project]
# Project details

[documents]
# Document definitions
```

### Project Section

The `project` section contains basic information about your project:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | String | Yes | The name of your project |
| `description` | String | Yes | A brief description of your project |

Example:
```toml
[project]
name = "My Project"
description = "A comprehensive documentation for My Project"
```

### Documents Section

The `documents` section contains a map of document IDs to document configurations. Each document has the following structure:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `title` | String | Yes | The title of the document |
| `path` | String | No | The relative path to the Markdown file (if omitted, this is treated as a category/section) |
| `sub_documents` | Array | No | An array of sub-documents (for creating a hierarchy) |

## Document Definition Formats

Documents can be defined in several ways:

### 1. Using Nested Tables

```toml
[documents.home]
title = "Home Page"
path = "docs/index.md"

[documents.getting_started]
title = "Getting Started"
path = "docs/getting-started.md"

[documents.references]
title = "References"
# No path means this is a category/section

[[documents.references.sub_documents]]
title = "Configuration Reference"
path = "docs/references/configuration.md"

[[documents.references.sub_documents]]
title = "API Reference"
path = "docs/references/api.md"
```

### 2. Using Inline Tables

```toml
[documents]
home = { title = "Home Page", path = "docs/index.md" }
getting_started = { title = "Getting Started", path = "docs/getting-started.md" }
# For complex structures, it's better to use standard table syntax instead of inline tables
# The following is NOT recommended for complex nested structures:
# references = { title = "References", sub_documents = [{ title = "Configuration Reference", path = "docs/references/configuration.md" }, { title = "API Reference", path = "docs/references/api.md" }] }
```

## Nesting Depth

Documents can be nested to create a hierarchical structure. Each document can have `sub_documents`, which is an array of document configurations. The nesting can be arbitrarily deep, though for practical purposes, it's recommended to limit nesting to 3-4 levels for better readability.

## Complete Example

Here's a complete example of a `documents.toml` file:

```toml
[project]
name = "My Project"
description = "A comprehensive documentation for My Project"

[documents]
# Home page
home = { title = "Home", path = "docs/index.md" }

# Getting started section
getting_started = { title = "Getting Started", path = "docs/getting-started.md" }

# User guide section with sub-documents (using standard table syntax for clarity)
[documents.user_guide]
title = "User Guide"

[[documents.user_guide.sub_documents]]
title = "Installation"
path = "docs/user-guide/installation.md"

[[documents.user_guide.sub_documents]]
title = "Configuration"
path = "docs/user-guide/configuration.md"

[[documents.user_guide.sub_documents]]
title = "Usage"
path = "docs/user-guide/usage.md"

# API reference section with sub-documents
[documents.api]
title = "API Reference"

[[documents.api.sub_documents]]
title = "Authentication"
path = "docs/api/authentication.md"

[[documents.api.sub_documents]]
title = "Endpoints"
path = "docs/api/endpoints.md"

# Advanced topics with nested sub-documents
[documents.advanced]
title = "Advanced Topics"

[[documents.advanced.sub_documents]]
title = "Performance Tuning"
path = "docs/advanced/performance.md"

[[documents.advanced.sub_documents]]
title = "Integrations"

# Nested sub-documents using array of tables syntax
[[documents.advanced.sub_documents.sub_documents]]
title = "Database Integration"
path = "docs/advanced/integrations/database.md"

[[documents.advanced.sub_documents.sub_documents]]
title = "Third-party Services"
path = "docs/advanced/integrations/third-party.md"
```

## Best Practices

1. **Use meaningful document IDs**: The keys in the `documents` map should be descriptive and reflect the content of the document.
2. **Keep paths consistent**: Use a consistent directory structure for your documentation files.
3. **Balance between nesting and flat structure**: Too much nesting can make the configuration hard to read, while a completely flat structure might not represent the logical organization of your documentation.
4. **Use sections for grouping**: Documents without a `path` can serve as sections/categories to group related documents.
5. **Be consistent with syntax**: Choose either nested tables or inline tables and stick with it for better readability.

## Validation

The documents-builder tool will validate your `documents.toml` file when processing your repository. You can also manually validate it using:

```bash
documents-builder validate-config <path-to-documents.toml>
```

This will check for schema compliance and report any errors.
