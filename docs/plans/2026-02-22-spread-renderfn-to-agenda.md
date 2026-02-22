# Spread RenderFn to Agenda

## Why

The `agenda_ui` module contains 6 stateless presentational widgets that manually construct GTK widget trees in imperative code. Converting them to `RenderFn` declarative style:
- Eliminates stored widget references and manual property-setting boilerplate
- Enables automatic diffing via the Reconciler (props unchanged = no GTK calls)
- Aligns the agenda UI with the project's virtual DOM direction (already used by `FeatureToggleMenuInfoRow`, `BluetoothDeviceRow`, etc.)

Additionally, the module is named `agenda_ui` which is redundant -- it should be `agenda` (the smart container is `components::agenda`, the dumb widgets are `ui::agenda`).

## What Changes

1. **Rename** `crates/overview/src/components/agenda_ui/` to `crates/overview/src/ui/agenda/`
2. **Move** pure formatting functions (`format.rs`, `meeting_links.rs`) into the new `ui::agenda` module unchanged
3. **Convert** 5 dumb widgets to `RenderFn`: `AgendaDetails`, `AttendeeList`, `AttendeeRow`, `MeetingButton`, `AgendaCard`
4. **Keep** `AgendaCard` as a full `Component` (it has `set_expanded()` mutation and output events) rather than `RenderFn`
5. **Update** `components::agenda.rs` (the smart container) to import from `ui::agenda` instead of `components::agenda_ui`

## Affected Files

- `crates/overview/src/components/agenda_ui/mod.rs` -- deleted (moved to ui::agenda)
- `crates/overview/src/components/agenda_ui/agenda_card.rs` -- moved to `crates/overview/src/ui/agenda/agenda_card.rs`
- `crates/overview/src/components/agenda_ui/agenda_details.rs` -- moved + converted to RenderFn
- `crates/overview/src/components/agenda_ui/attendee_list.rs` -- moved + converted to RenderFn
- `crates/overview/src/components/agenda_ui/attendee_row.rs` -- moved + converted to RenderFn
- `crates/overview/src/components/agenda_ui/meeting_button.rs` -- moved (kept as imperative, uses popover)
- `crates/overview/src/components/agenda_ui/meeting_links.rs` -- moved to `crates/overview/src/ui/agenda/meeting_links.rs` (pure logic, unchanged)
- `crates/overview/src/components/agenda_ui/format.rs` -- moved to `crates/overview/src/ui/agenda/format.rs` (pure logic, unchanged)
- `crates/overview/src/ui/mod.rs` -- add `pub mod agenda;`
- `crates/overview/src/components/mod.rs` -- remove `pub mod agenda_ui;`
- `crates/overview/src/components/agenda.rs` -- update imports from `agenda_ui` to `ui::agenda`

## Tasks

### 1. Create `crates/overview/src/ui/agenda/mod.rs`

Create the new module with submodule declarations:

```rust
pub mod agenda_card;
pub mod agenda_details;
pub mod attendee_list;
pub mod attendee_row;
pub mod format;
pub mod meeting_button;
pub mod meeting_links;
```

### 2. Move `format.rs` to `crates/overview/src/ui/agenda/format.rs`

Copy file unchanged. Contains `format_time_range`, `format_timestamp`, `strip_html_tags` -- pure functions, no GTK dependencies beyond glib::DateTime.

### 3. Move `meeting_links.rs` to `crates/overview/src/ui/agenda/meeting_links.rs`

Copy file unchanged. Contains `MeetingProvider`, `MeetingLink`, `extract_meeting_links` -- pure data types and logic.

### 4. Convert `attendee_row.rs` to RenderFn at `crates/overview/src/ui/agenda/attendee_row.rs`

Convert `AttendeeRow` from imperative struct to `RenderFn`:

- Define `AttendeeRowProps { name: Option<String>, email: String, status: AttendeeStatus }` (Clone + PartialEq)
- Implement `RenderFn` returning `VNode::vbox(VBox::horizontal(4).child(VNode::icon(...)).child(VNode::label(...)))`
- Use `attendee_status_icon_name()` helper (keep as standalone fn in this file)
- Export `type AttendeeRowComponent = RenderComponent<AttendeeRowRender>;`

### 5. Convert `attendee_list.rs` to RenderFn at `crates/overview/src/ui/agenda/attendee_list.rs`

Convert `AttendeeList` from imperative struct to `RenderFn`:

- Define `AttendeeListProps { attendees: Vec<(String, Option<String>, AttendeeStatus)> }` (Clone + PartialEq)
- Implement `RenderFn` returning `VNode::vbox(VBox::horizontal(8).child(icon).child(list_box))`
- Each attendee rendered as `VNode::new::<AttendeeRowComponent>(props).key(email)`
- Export `type AttendeeListComponent = RenderComponent<AttendeeListRender>;`

### 6. Convert `agenda_details.rs` to RenderFn at `crates/overview/src/ui/agenda/agenda_details.rs`

Convert `AgendaDetails` from imperative struct to `RenderFn`:

- Define `AgendaDetailsProps { location: Option<String>, attendees: Vec<(String, Option<String>, AttendeeStatus)>, description: Option<String> }` (Clone + PartialEq)
- Implement `RenderFn` building a VBox::vertical(4) with conditional location row, attendee list component, and description row
- HTML stripping and truncation logic stays in the render function
- Export `type AgendaDetailsComponent = RenderComponent<AgendaDetailsRender>;`

### 7. Move `meeting_button.rs` to `crates/overview/src/ui/agenda/meeting_button.rs`

Move file with minimal changes. `MeetingButton` uses `gtk::Popover` and `gio::AppInfo::launch_default_for_uri` which have no VNode primitives -- keep as imperative widget. Update import paths for `meeting_links` to use `super::meeting_links`.

### 8. Move `agenda_card.rs` to `crates/overview/src/ui/agenda/agenda_card.rs`

Move `AgendaCard` with updated imports:
- Change `use super::agenda_details::AgendaDetails;` to `use super::agenda_details::AgendaDetailsComponent;`
- Change `use super::format::format_time_range;` to `use super::format::format_time_range;` (same)
- Change `use super::meeting_button::MeetingButton;` to `use super::meeting_button::MeetingButton;` (same)
- Change `use super::meeting_links::extract_meeting_links;` to `use super::meeting_links::extract_meeting_links;` (same)
- Update `AgendaDetails::new(event)` call to use `AgendaDetailsComponent::build(&props)` with the new props struct

### 9. Register `ui::agenda` in `crates/overview/src/ui/mod.rs`

Add `pub mod agenda;` to the module declarations.

### 10. Remove `components::agenda_ui` from `crates/overview/src/components/mod.rs`

Remove the line `pub mod agenda_ui;`.

### 11. Update imports in `crates/overview/src/components/agenda.rs`

Replace:
```rust
use crate::components::agenda_ui::agenda_card::{AgendaCard, AgendaCardOutput};
```
with:
```rust
use crate::ui::agenda::agenda_card::{AgendaCard, AgendaCardOutput};
```

### 12. Delete old `crates/overview/src/components/agenda_ui/` directory

Remove the entire old directory after confirming the new `ui::agenda` module compiles.

### 13. Run `cargo build --workspace` and `cargo test --workspace`

Verify everything compiles and tests pass.
