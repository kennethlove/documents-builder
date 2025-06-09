# documents-builder

Project to collect and build centralized documentation for all projects in an organization.

# Usage

Add a `documents.toml` file to the root of your project with the following structure:

```toml
[project]
name = "my-project"
description = "A brief description of my project"

[documents]
home = { path = "docs/index.md", title = "Home" }
getting_started = { path = "docs/getting_started.md", title = "Getting Started" }
references = { title = "References", sub_documents = [
    { path = "references/configuration.md", title = "Reference Configuration" },
    { path = "references/schemas.md", title = "Reference Schemas" }
] }
```

Then write documents in Markdown format in the specified paths. Each document will be processed
and included in the final output.

# Validating the Configuration

To validate the configuration of your project, run:

```bash
documents-builder validate <repository-name>
```

This will check the `documents.toml` file for correctness.

Use `--check-files` to ensure that all specified documents exist.

# Generating HTML Fragments

To generate HTML fragments from the Markdown documents, run:

```bash
documents-builder process-repo <repository-name>
```
