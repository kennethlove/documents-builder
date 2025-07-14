Based on the product requirements document, I'll create a comprehensive MVP action plan that focuses on the core
functionality needed to get the Documents platform up and running. Here's the revised action plan:

# Documents Platform MVP - Action Plan

## Overview

Building a minimum viable product for the Documents platform that enables organizations to discover, process, and serve
documentation from their GitHub repositories. The MVP focuses on core functionality: repository scanning, content
processing, and basic documentation serving with search capabilities.

## MVP Scope Definition 🎯

### Core MVP Features

- **Repository Discovery**: Scan GitHub organization repositories for `documents.toml` files
- **Content Processing**: Extract and process markdown documentation files
- **Basic Search**: Full-text search capabilities across all documentation
- **Documentation Serving**: Static site generation with browsable interface
- **Real-time Updates**: GitHub webhook integration for automatic updates
- **CLI Interface**: Command-line tools for manual operations and testing

### MVP Exclusions (Phase 2+)

- Advanced quality scoring algorithms
- LLM integration and semantic search
- MCP server implementation
- Branch-specific documentation
- Version/tag browsing
- Advanced UI features

## Phase 1: Foundation & Core Infrastructure 🏗️

### 1.1 Database Schema & Setup

**Priority: Critical**

- [ ] Add PostgreSQL dependency (`sqlx`) to Cargo.toml
- [ ] Design core database schema:

```sql
-- Organizations table
CREATE TABLE organizations
(
    id                SERIAL PRIMARY KEY,
    name              VARCHAR(255) NOT NULL UNIQUE,
    github_token_hash VARCHAR(255) NOT NULL,
    created_at        TIMESTAMP DEFAULT NOW(),
    updated_at        TIMESTAMP DEFAULT NOW()
);

-- Repositories table
CREATE TABLE repositories
(
    id                   SERIAL PRIMARY KEY,
    org_id               INTEGER REFERENCES organizations (id),
    name                 VARCHAR(255) NOT NULL,
    full_name            VARCHAR(255) NOT NULL UNIQUE,
    has_documents_config BOOLEAN   DEFAULT FALSE,
    last_processed_at    TIMESTAMP,
    last_commit_sha      VARCHAR(40),
    is_active            BOOLEAN   DEFAULT TRUE,
    created_at           TIMESTAMP DEFAULT NOW(),
    updated_at           TIMESTAMP DEFAULT NOW()
);

-- Documents table
CREATE TABLE documents
(
    id           SERIAL PRIMARY KEY,
    repo_id      INTEGER REFERENCES repositories (id),
    file_path    VARCHAR(500) NOT NULL,
    title        VARCHAR(500),
    content      TEXT         NOT NULL,
    content_hash VARCHAR(64)  NOT NULL,
    metadata     JSONB,
    created_at   TIMESTAMP DEFAULT NOW(),
    updated_at   TIMESTAMP DEFAULT NOW(),
    UNIQUE (repo_id, file_path)
);

-- Processing jobs table
CREATE TABLE processing_jobs
(
    id            SERIAL PRIMARY KEY,
    job_type      VARCHAR(50) NOT NULL,
    target_type   VARCHAR(50) NOT NULL, -- 'organization' or 'repository'
    target_id     INTEGER     NOT NULL,
    status        VARCHAR(50) DEFAULT 'pending',
    started_at    TIMESTAMP,
    completed_at  TIMESTAMP,
    error_message TEXT,
    created_at    TIMESTAMP   DEFAULT NOW()
);
```

- [ ] Implement database migrations system
- [ ] Add database connection pooling
- [ ] Create database health checks

### 1.2 Enhanced CLI Foundation

**Priority: High**

- [x] Basic CLI structure with `scan`, `list-all`, and `serve` commands
- [ ] Add `scan-org` command for full organization scanning
- [ ] Add `process-repo <repo-name>` command for single repository processing
- [ ] Add `search <query>` command for testing search functionality
- [ ] Add `status` command for system health and processing status
- [ ] Add `validate-config <repo-name>` command for testing repository configurations
- [ ] Implement progress bars and better user feedback for all commands

### 1.3 Configuration Management

**Priority: High**

- [x] Environment variable configuration (`GITHUB_TOKEN`, `GITHUB_ORGANIZATION`)
- [ ] Add `DATABASE_URL` environment variable support
- [ ] Add `MEILISEARCH_URL` and `MEILISEARCH_KEY` configuration
- [ ] Create configuration validation system
- [ ] Add configuration file support for local development
- [ ] Implement configuration precedence (env vars > config file > defaults)

## Phase 2: Repository Discovery & Processing 🔍

### 2.1 GitHub Organization Scanning

**Priority: Critical**

- [ ] Implement GitHub API client with rate limiting
- [ ] Create organization repository enumeration:

```rust
pub async fn scan_organization(
    github_token: &str,
    org_name: &str,
) -> Result<Vec<Repository>, ScanError> {
    // Discover all repositories in organization
    // Filter out archived repositories
    // Check for documents.toml in each repository
    // Return list of repositories with documentation
}
```

- [ ] Add repository filtering (public/private, archived, fork status)
- [ ] Implement `documents.toml` detection across all repositories
- [ ] Add repository metadata extraction (description, topics, language)
- [ ] Create repository access permission validation

### 2.2 Documents.toml Processing

**Priority: Critical**

- [ ] Define `documents.toml` schema specification:

```toml
[project]
name = "Project Name"
description = "Project description"

[sources]
# Glob patterns for markdown files
include = ["docs/**/*.md", "README.md", "CHANGELOG.md"]
exclude = ["docs/internal/**"]

[settings]
# Optional: custom title patterns, metadata extraction
title_from_frontmatter = true
auto_title_from_filename = true
```

- [ ] Implement TOML parsing with validation
- [ ] Add glob pattern matching for file discovery
- [ ] Create file path normalization and validation
- [ ] Add error handling for malformed configurations

### 2.3 Document Content Processing

**Priority: Critical**

- [ ] Implement markdown file fetching from GitHub API
- [ ] Add frontmatter parsing (YAML/TOML) for metadata extraction
- [ ] Create markdown content preprocessing:
    - Link normalization
    - Image reference handling
    - Code block language detection
- [ ] Implement content validation and sanitization
- [ ] Add document title extraction (from frontmatter or filename)
- [ ] Create content change detection using file hashes

## Phase 3: Search Integration 🔍

### 3.1 Meilisearch Integration

**Priority: High**

- [ ] Add Meilisearch client dependency
- [ ] Create search index configuration:

```rust
pub struct DocumentSearchIndex {
    pub id: String,
    pub title: String,
    pub content: String,
    pub file_path: String,
    pub repository_name: String,
    pub organization_name: String,
    pub metadata: HashMap<String, String>,
}
```

- [ ] Implement document indexing pipeline
- [ ] Add search index updates on document changes
- [ ] Create search query interface with filters
- [ ] Add search result ranking and relevance tuning

### 3.2 Search API Implementation

**Priority: High**

- [ ] Create `GET /api/search` endpoint:

```rust
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub repository_filter: Option<String>,
}

pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total_hits: u32,
    pub processing_time_ms: u32,
}
```

- [ ] Implement search result highlighting
- [ ] Add search suggestions and autocomplete
- [ ] Create search analytics tracking
- [ ] Add search result caching

## Phase 4: Basic Documentation Serving 📖

### 4.1 Static Site Generation

**Priority: Medium**

- [ ] Design HTML template system for documentation browsing
- [ ] Create repository navigation structure
- [ ] Implement document rendering with markdown parsing
- [ ] Add cross-repository navigation
- [ ] Create responsive design for mobile/desktop
- [ ] Add basic search interface integration

### 4.2 Documentation API

**Priority: Medium**

- [ ] Create documentation serving endpoints:

```rust
// GET /api/docs/repos - List all repositories
// GET /api/docs/repos/{repo}/documents - List documents in repository
// GET /api/docs/repos/{repo}/documents/{path} - Get specific document
// GET /api/docs/repos/{repo}/navigation - Get repository navigation
```

- [ ] Implement proper HTTP caching headers
- [ ] Add CORS support for frontend integration
- [ ] Create API documentation with OpenAPI/Swagger
- [ ] Add rate limiting for API endpoints

### 4.3 Basic Web Interface

**Priority: Low (MVP)**

- [ ] Create simple HTML interface for browsing documentation
- [ ] Add basic search form integration
- [ ] Implement document viewing with syntax highlighting
- [ ] Create repository and document navigation
- [ ] Add responsive design basics

## Phase 5: Real-time Updates & Webhooks 🔄

### 5.1 GitHub Webhook Integration

**Priority: High**

- [ ] Implement GitHub webhook signature verification
- [ ] Create webhook endpoint (`POST /api/webhooks/github`)
- [ ] Add webhook payload parsing and validation
- [ ] Filter for relevant events (push to main/default branch)
- [ ] Extract changed files from webhook payload
- [ ] Implement webhook secret management

### 5.2 Async Job Processing

**Priority: High**

- [ ] Design job queue system using `tokio` channels
- [ ] Create job processor with error handling and retries
- [ ] Add job status tracking in database
- [ ] Implement processing timeouts and cancellation
- [ ] Create job prioritization system
- [ ] Add job monitoring and metrics

### 5.3 Delta Processing

**Priority: Medium**

- [ ] Implement changed file detection from webhook payloads
- [ ] Add selective document processing (only changed files)
- [ ] Create processing state management
- [ ] Add concurrent processing limits
- [ ] Implement processing job deduplication
- [ ] Add processing completion notifications

## Phase 6: HTTP Server & API Infrastructure 🌐

### 6.1 Enhanced HTTP Server

**Priority: High**

- [x] Basic HTTP server with health check endpoint
- [x] Request logging and error handling middleware
- [x] CORS configuration
- [ ] Add authentication middleware (API keys)
- [ ] Implement request rate limiting
- [ ] Add request/response compression
- [ ] Create comprehensive error response formatting

### 6.2 System Status & Monitoring

**Priority: Medium**

- [ ] Create system health endpoints:

```rust
// GET /api/health - Basic health check
// GET /api/health/detailed - Database, search, GitHub connectivity
// GET /api/status/processing - Current processing jobs
// GET /api/status/repositories - Repository processing status
```

- [ ] Add metrics collection (request counts, processing times)
- [ ] Implement structured logging with request tracing
- [ ] Create processing status dashboard data
- [ ] Add error rate monitoring and alerting

## MVP Testing Strategy 🧪

### 6.3 Unit Testing

**Priority: High**

- [ ] Test GitHub API integration and error handling
- [ ] Test `documents.toml` parsing and validation
- [ ] Test document content processing pipeline
- [ ] Test search indexing and querying
- [ ] Test webhook signature verification
- [ ] Test database operations and migrations

### 6.4 Integration Testing

**Priority: Medium**

- [ ] Test full organization scanning workflow
- [ ] Test webhook-triggered processing end-to-end
- [ ] Test search integration with real data
- [ ] Test API endpoints with various scenarios
- [ ] Test error handling and recovery
- [ ] Test concurrent processing scenarios

### 6.5 Performance Testing

**Priority: Low (MVP)**

- [ ] Benchmark organization scanning performance
- [ ] Test webhook processing under load
- [ ] Measure search response times
- [ ] Test database performance with large datasets
- [ ] Validate memory usage during processing

## MVP Success Metrics 📊

### Core Functionality Metrics

- [ ] **Repository Discovery**: Successfully discover >95% of organization repositories
- [ ] **Document Processing**: Process repositories with `documents.toml` in <30 seconds
- [ ] **Search Performance**: Return search results in <500ms
- [ ] **Webhook Processing**: Process webhook events in <10 seconds
- [ ] **System Uptime**: Maintain >99% uptime during testing

### User Experience Metrics

- [ ] **CLI Usability**: All CLI commands complete successfully with clear feedback
- [ ] **API Reliability**: API endpoints return correct responses with proper error handling
- [ ] **Search Accuracy**: Search results return relevant documents based on query
- [ ] **Documentation Accessibility**: Generated documentation is browsable and navigable

## MVP Deployment Preparation 🚀

### 6.6 Containerization

**Priority: Medium**

- [ ] Create Dockerfile for the application
- [ ] Add docker-compose for local development (app + PostgreSQL + Meilisearch)
- [ ] Create deployment scripts for production
- [ ] Add environment variable configuration for containers
- [ ] Create health check endpoints for container orchestration

### 6.7 Documentation

**Priority: Medium**

- [ ] Write comprehensive README with setup instructions
- [ ] Create API documentation
- [ ] Add CLI command documentation
- [ ] Create deployment guide
- [ ] Add troubleshooting guide
- [ ] Create `documents.toml` specification documentation

---

## MVP Development Timeline (Estimated: 8-12 weeks)

### Weeks 1-2: Foundation

- Database setup and migrations
- Enhanced CLI commands
- Configuration management
- Basic testing framework

### Weeks 3-4: Repository Processing

- GitHub API integration
- Organization scanning
- Document content processing
- `documents.toml` handling

### Weeks 5-6: Search Integration

- Meilisearch integration
- Search API implementation
- Document indexing pipeline
- Search testing

### Weeks 7-8: HTTP Server & APIs

- Enhanced HTTP server
- Documentation serving APIs
- System status endpoints
- API testing

### Weeks 9-10: Webhooks & Real-time Updates

- GitHub webhook integration
- Async job processing
- Delta processing
- Integration testing

### Weeks 11-12: Polish & Deployment

- Basic web interface
- Containerization
- Documentation
- Performance testing
- MVP deployment

## Post-MVP Roadmap 🔮

### Phase 2 Features (Months 2-3)

- Advanced quality scoring
- Cross-repository link resolution
- Enhanced web interface
- Advanced search features

### Phase 3 Features (Months 4-6)

- LLM integration
- MCP server implementation
- Branch-specific documentation
- Version/tag browsing

### Phase 4 Features (Months 6+)

- Advanced UI/UX
- Enterprise features
- Performance optimization
- Scaling improvements

This MVP action plan focuses on delivering core functionality that provides immediate value to users while establishing
a solid foundation for future enhancements. The plan prioritizes essential features that enable organizations to scan,
process, and serve their documentation effectively.
