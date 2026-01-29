## 1. Create test file structure

- [x] 1.1 Create `src/ui/icon_tests.rs` with module declaration
- [x] 1.2 Add `#[cfg(test)] mod icon_tests;` to `src/ui/icon.rs`
- [x] 1.3 Import necessary types (`Icon`, `resolve_themed_icon`, `Arc`, `PathBuf`)

## 2. Implement Icon::from_str tests

- [x] 2.1 Test absolute path detection (contains "/")
- [x] 2.2 Test relative path detection (starts with ".")
- [x] 2.3 Test home directory path (starts with "~")
- [x] 2.4 Test themed icon name (no path indicators)
- [x] 2.5 Test whitespace trimming behavior

## 3. Implement resolve_themed_icon tests

- [x] 3.1 Test exact name match (using standard icon like "dialog-information")
- [x] 3.2 Test symbolic variant fallback
- [x] 3.3 Test lowercase fallback
- [x] 3.4 Test lowercase-symbolic fallback
- [x] 3.5 Test no match scenario (non-existent icon name)

## 4. Verify

- [x] 4.1 Run tests with `cargo test icon_tests`
- [x] 4.2 Verify all tests pass
- [x] 4.3 Check test output for any GTK warnings
