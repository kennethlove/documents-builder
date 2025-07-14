# Documents

This document outlines the requirements for the Documents product. The Documents application is designed to allow an
organization to create and manage the documentation for all of their projects smartly and efficiently. Each project keeps
its documentation wherever it is most convenient for the project, and Documents guarantees that those documents will end up
in an easy-to-browse, easy-to-search, LLM-friendly central location. The Documents application is made up of several
components, each of which is described in detail below. Ultimately, an organization using Documents will be able to keep
all of their documentation up to date, reasonably located, searchable, and browsable.

## Applications

Within Documents, there are several applications that work together to provide the functionality described above. Each
application has its own set of requirements and functionality, but they all work together to provide a seamless experience
for the user. The applications are as follows:

### Scanner

The Scanner application is responsible for scanning the repositories of an organization to find documentation files. It
will scan through all the repositories that belong to a GitHub organization, probing each one for a `documents.toml` file
in the root directory. If it finds one, it will read the file to determine which files should be used to generate the
documentation for that repository. The Scanner will capture the contents of the files specified and store them in a
database for later use. The Scanner will also be responsible for updating the database when files are added, removed, or
modified in the repositories.

The Scanner will be triggered by a webhook from GitHub, which will notify it when a pull request is merged. This will
keep the documentation up to date with the latest changes in the repositories. The Scanner will also be able to run on a
schedule, allowing it to periodically check for changes in the repositories and update the documentation accordingly.
Finally, the Scanner can be run manually, allowing users to trigger a scan at any time.

### Indexer

The Indexer application is responsible for indexing the documentation files that are captured by the Scanner. It will
index the contents of the files, making them searchable, both by a search engine and an LLM. The Indexer will also be
responsible for updating the index when files are added, removed, or modified in the repositories. The Indexer will
ensure that the indexed content is optimized for search engines and LLMs, making it easy to find and understand.

The Indexer will use a set of algorithms to analyze the content of the files and extract relevant information, such as
keywords, summaries, and other metadata. It will also be responsible for generating a search index that can be used by
search engines and LLMs to quickly find relevant content. The Indexer will be triggered by the Scanner when files are
captured, and it will also be able to run on a schedule to ensure that the index is always up to date. Additionally, the
Indexer will provide an API that allows users to search for content and retrieve relevant results.

As part of the indexing process, the Indexer will also evaluate the content of the documentation files to determine an
overall quality score. This score will be based on factors such a clarity, completeness, and relevance of the content.
This quality score will be used to provide feedback to the authors of the documentation, helping them to spot areas that
would benefit from improvement.

### Builder

The Builder application is responsible for building the documentation files into a format that is browsable and searchable.
It will take the indexed content from the Indexer and build it into a format that is straightforward to navigate, such
as a static website. The Builder will also be responsible for updating the static website when documentation files are
added, removed, or modified in the repositories. The Builder will ensure that intra-document links are preserved, making
it easy for users to navigate between related documents even if they are stored in different repositories.

The Builder will use a set of templates to generate the static website. The templates will be designed for navigation
and search, with a focus on usability and accessibility. The Builder will also be responsible for generating a sitemap
and other metadata that will help search engines and LLMs understand the structure and content of the documentation. The
Builder will be triggered by the Indexer when files are indexed, and it will also be able to run on a schedule to ensure
that the static website is always up to date. Additionally, the Builder will provide an API that allows users to browse
the documentation and retrieve relevant content.

## Other Planned Features

### Search

The Documents application will provide a search feature that allows users to search for content within the entire
organization's documentation. The search feature will be provided by a third-party search engine, such as Meilisearch.

### LLM Integration

The Documents application will integrate with a large language model (LLM) to provide advanced search capabilities and
natural language processing features. The LLM will be used to analyze the content of the documentation and provide
relevant results based on user queries. The LLM will also be used to generate summaries and other metadata that will
help users understand the content of the documentation. The LLM will be integrated with the Indexer and Builder
applications, allowing it to access the indexed content and generate relevant results based on user queries.

### MCP Server

The Documents application will provide a server that allows users to access the documentation and search features via
an MCP (Model Context Protocol) interface. This will allow other LLMs and applications to access the documentation.

### Tagging

The Documents application will provide a tagging feature that allows users to browse previous versions of the
documentation. This will allow users to see how the documentation has changed over time and to access previous versions
when working on legacy projects. The versioning feature will be provided by the Builder application, which will
generate a static website that includes previous versions of the documentation.

While all tags will be stored in the database, the Builder will generate a static website that includes only the latest
version of the documentation by default. Users will be able to access previous versions of the documentation by
selecting a specific tag in the user interface. The Builder will also provide an API that allows users to retrieve
previous versions of the documentation based on the selected tag. This will allow users to access the documentation
for legacy projects and to see how the documentation has changed over time.

### Branching

Along with versioning, the Documents application will support branching of documentation. This will allow users
to maintain separate versions of the documentation for different branches of a project. The Scanner will capture the
documentation files for each branch, and the Indexer will index them separately. The Builder will then generate a
static website that includes the documentation for each branch, allowing users to browse and search the documentation
for each branch separately. This will be particularly useful for projects that have multiple active branches, such as
feature branches or release branches. Users will be able to switch between branches in the user interface, allowing
them to view the documentation for the branch they are currently working on.

Not all branches will be indexed by default. The Scanner will only capture the documentation files for branches that
are indicated in the `documents.toml` file for each repository. This will allow users to control which branches
are included in the documentation and which branches are excluded.

## User Interface

The Documents application will provide a user interface that allows users to browse and search the documentation. The
user interface will be designed to be intuitive and easy to use, with a focus on usability and accessibility. The user
interface will provide a search bar that allows users to search for content within the documentation, as well as a
navigation menu that allows users to browse the documentation by category or repository. The user interface will also
provide a way for users to view the quality score of the documentation, allowing them to see how well the documentation
is written and where it can be improved. The user interface will be responsive, allowing users to access the documentation
on a variety of devices, including desktops, laptops, tablets, and smartphones. The user interface will also provide a
way for users to browse previous versions of the documentation, allowing them to see how the documentation has changed.
Navigation between related documents will be seamless, with intra-document links preserved even if the documents are
stored in different repositories.

## Phases

### Phase 1: Basic Functionality (MVP)

1. **Scanner**: Implement the Scanner application to scan repositories for `documents.toml` files and capture
   documentation files specified within them. Ensure it can run on a schedule and be triggered by GitHub webhooks.
2. **Indexer**: Implement the Indexer application to index the captured documentation files, making them searchable by 
    basic full-text search. Ensure it can update the index when files are added, removed, or modified.
3. **Builder**: Implement the Builder application to generate a static website from the indexed documentation files.
   Ensure it preserves intra-document links and provides a basic user interface for browsing the documentation.

### Phase 2: Enhanced Functionality

1. **Builder**: Enhance the Builder to handle intra-repository links, ensuring that links between documents
   in different repositories are preserved.
2. **Indexer**: Implement a quality score evaluation for documentation files, providing feedback on clarity,
   completeness, and relevance.
3. **User Interface**: Develop a user interface that allows users to browse and search the documentation,
   view quality scores, and navigate between related documents seamlessly.

### Phase 3: Search and LLM Integration

1. **Search Engine Integration**: Integrate a third-party search engine (e.g., Meilisearch) to provide
   advanced search capabilities for the documentation.
2. **LLM Integration**: Integrate a large language model (LLM) to enhance search capabilities and
   provide natural language processing features, such as generating summaries and relevant results based on user queries.
3. **MCP Server**: Implement an MCP server to allow other LLMs and applications to access the documentation
   and search features via an MCP interface.

### Phase 4: Branching and Versioning

1. **Tagging**: Implement a tagging feature that allows users to browse previous versions of the documentation.
   Ensure that the Builder generates a static website that includes only the latest version by default, with an option
   to access previous versions based on selected tags.
2. **Branching**: Implement branching support for documentation, allowing users to maintain separate versions
   of the documentation for different branches of a project.
3. **User Interface Enhancements**: Enhance the user interface to support browsing previous versions of the
   documentation and switching between branches. Ensure that navigation between related documents remains seamless.
4. **Comparison Feature**: Implement a feature that allows users to compare documentation files across different
   branches or versions, highlighting differences and changes made over time.
5. **Scanner**: Enhance the Scanner to capture documentation files for branches specified in the `documents.toml` file,
   allowing users to control which branches are included in the documentation.
6. **Builder**: Ensure that the Builder generates a static website that includes documentation for each branch,
   allowing users to browse and search the documentation for each branch separately.
