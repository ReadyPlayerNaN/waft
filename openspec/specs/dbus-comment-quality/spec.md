## ADDED Requirements

### Requirement: Comment Redundancy Removal

DBus module comments SHALL be minimal and avoid explaining basic Rust or DBus concepts.

#### Scenario: Basic concept explanations removed

- **WHEN** comments explain what a HashMap is, how async works, or basic DBus concepts
- **THEN** those comments are removed
- **AND** the code is assumed to be readable by developers familiar with Rust and DBus

#### Scenario: Obvious parameter explanations removed

- **WHEN** comments repeat parameter names or types already clear from function signature
- **THEN** those comments are removed
- **AND** doc comments focus on behavior and invariants

#### Scenario: Implementation detail comments reduced

- **WHEN** comments describe line-by-line implementation steps
- **THEN** those comments are reduced to essential design decisions only

### Requirement: Outdated Comment Updates

DBus module comments SHALL be current and accurate.

#### Scenario: TODO comments for completed work removed

- **WHEN** a TODO comment describes work that has been completed
- **THEN** the comment is removed

#### Scenario: Incorrect comments corrected

- **WHEN** a comment describes behavior that no longer matches the implementation
- **THEN** the comment is updated to match current behavior
- **OR** the comment is removed if no longer necessary

#### Scenario: Deprecated API comments updated

- **WHEN** a comment references deprecated APIs or old patterns
- **THEN** the comment is updated to reference current APIs

### Requirement: Design Decision Documentation

DBus module comments SHALL preserve explanations of non-obvious design decisions and workarounds.

#### Scenario: Non-obvious workarounds documented

- **WHEN** code contains a workaround for a bug or limitation
- **THEN** the comment explains why the workaround is necessary
- **AND** includes context about the underlying issue

#### Scenario: Design trade-offs documented

- **WHEN** code makes a non-obvious choice between alternatives
- **THEN** the comment explains the rationale for the decision

#### Scenario: Safety invariants documented

- **WHEN** code requires specific preconditions or maintains invariants
- **THEN** the comment documents these requirements

### Requirement: Doc Comment Usage

Public API functions in DBus modules SHALL use doc comments (`///`) consistently.

#### Scenario: Public functions have doc comments

- **WHEN** a function is `pub`
- **THEN** it has a doc comment describing its purpose
- **AND** the doc comment is concise (1-3 sentences)

#### Scenario: Internal functions use regular comments

- **WHEN** a function is not `pub`
- **THEN** it uses regular comments (`//`) if comments are needed
- **AND** internal helper functions may have no comments if self-documenting

#### Scenario: Doc comments describe errors

- **WHEN** a public function returns `Result`
- **THEN** the doc comment describes common error conditions

### Requirement: Example Comment Style

Comments SHALL follow consistent style guidelines across all DBus modules.

#### Scenario: Concise property getter docs

- **WHEN** documenting a property getter function
- **THEN** format: "Get [property] via [interface].[method]. Returns [default] if [condition]."
- **AND** avoid repeating parameter names or types from signature

#### Scenario: Signal listener docs

- **WHEN** documenting a signal listener function
- **THEN** format: "Listen for [signal] from [interface]. Calls callback with [data]."
- **AND** mention background execution if applicable

#### Scenario: Value converter docs

- **WHEN** documenting a value conversion helper
- **THEN** format: "Extract [type] from OwnedValue. Returns None for incompatible types."
- **AND** keep to one line if possible
