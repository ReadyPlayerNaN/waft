## ADDED Requirements

### Requirement: Icon Theme Resolution Fallback

The test suite SHALL verify that `resolve_themed_icon` attempts icon name resolution in the correct fallback sequence against the GTK icon theme.

#### Scenario: Exact name match

- **WHEN** `resolve_themed_icon` receives an icon name that exists in the theme
- **THEN** it returns `Some(name)` with the exact name

#### Scenario: Symbolic variant fallback

- **WHEN** the exact name is not found
- **AND** a "-symbolic" variant exists in the theme
- **THEN** it returns `Some(name-symbolic)`

#### Scenario: Lowercase fallback

- **WHEN** the exact name and symbolic variant are not found
- **AND** a lowercase variant exists in the theme
- **THEN** it returns `Some(lowercase_name)`

#### Scenario: Lowercase symbolic fallback

- **WHEN** exact, symbolic, and lowercase variants are not found
- **AND** a lowercase-symbolic variant exists in the theme
- **THEN** it returns `Some(lowercase-symbolic)`

#### Scenario: No match found

- **WHEN** none of the fallback variants exist in the theme
- **THEN** it returns `None`

#### Scenario: No display available

- **WHEN** GTK display is not available (headless environment without X)
- **THEN** it returns `None` gracefully without panicking
