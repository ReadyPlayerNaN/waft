## ADDED Requirements

### Requirement: Icon String Classification

The test suite SHALL verify that `Icon::from_str` correctly classifies icon strings as either `FilePath` or `Themed` variants based on path indicators.

#### Scenario: Absolute path detected

- **WHEN** `Icon::from_str` receives a string containing "/"
- **THEN** it returns `Icon::FilePath` variant
- **AND** the PathBuf contains the full string

#### Scenario: Relative path with dot detected

- **WHEN** `Icon::from_str` receives a string starting with "."
- **THEN** it returns `Icon::FilePath` variant

#### Scenario: Home directory path detected

- **WHEN** `Icon::from_str` receives a string starting with "~"
- **THEN** it returns `Icon::FilePath` variant

#### Scenario: Themed icon name detected

- **WHEN** `Icon::from_str` receives a string without path indicators
- **THEN** it returns `Icon::Themed` variant
- **AND** the themed name matches the input string

#### Scenario: Whitespace trimming

- **WHEN** `Icon::from_str` receives a string with leading/trailing whitespace
- **THEN** whitespace is trimmed before classification
- **AND** classification rules apply to the trimmed string
