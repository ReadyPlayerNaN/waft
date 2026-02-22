# Spread RenderFn to Calendar Grid

## Why

The `calendar_grid` module has two dumb widgets (`MonthGrid` and `DayCell`) that manually build GTK widget trees with imperative code. Converting `DayCell` to `RenderFn`:
- Eliminates manual CSS class toggling and widget reference storage
- Enables automatic reconciler diffing (unchanged props = no GTK calls)
- Aligns with the project's virtual DOM direction

The module rename (`calendar_grid` to `calendar`) makes it consistent with the `agenda` rename and the project's semantic naming convention.

`MonthGrid` is harder to convert because it uses a `gtk::Grid` (42 cells in 7x7 layout) which has no VNode primitive. It stays imperative but benefits from `DayCell` becoming a `RenderComponent`.

## What Changes

1. **Rename** `crates/overview/src/components/calendar_grid/` to `crates/overview/src/ui/calendar/`
2. **Convert** `DayCell` to `RenderFn`
3. **Keep** `MonthGrid` imperative (uses gtk::Grid, no VNode primitive for grid layout)
4. **Keep** `CalendarComponent` in `components/` as the smart container, update imports
5. **Move** pure helper `days_in_month` and its tests with `MonthGrid`

## Affected Files

- `crates/overview/src/components/calendar_grid/mod.rs` -- deleted (moved)
- `crates/overview/src/components/calendar_grid/day_cell.rs` -- moved to `crates/overview/src/ui/calendar/day_cell.rs`, converted to RenderFn
- `crates/overview/src/components/calendar_grid/month_grid.rs` -- moved to `crates/overview/src/ui/calendar/month_grid.rs`, updated DayCell import
- `crates/overview/src/components/calendar_grid/calendar_component.rs` -- moved to `crates/overview/src/components/calendar.rs` (smart container stays in components)
- `crates/overview/src/ui/mod.rs` -- add `pub mod calendar;`
- `crates/overview/src/components/mod.rs` -- replace `pub mod calendar_grid;` with `pub mod calendar;`
- Any file importing `components::calendar_grid::CalendarComponent` -- update import path

## Tasks

### 1. Create `crates/overview/src/ui/calendar/mod.rs`

Create the new module:

```rust
pub mod day_cell;
pub mod month_grid;
```

### 2. Convert `day_cell.rs` to RenderFn at `crates/overview/src/ui/calendar/day_cell.rs`

Convert `DayCell` from imperative struct to `RenderFn`:

- Define `DayCellProps { day: u32, current_month: bool, today: bool, selected: bool, event_count: usize }` -- already exists, add `Clone + PartialEq` derives
- Define `DayCellOutput::Clicked(u32)` -- already exists
- Implement `RenderFn` for `DayCellRender`:
  - Build a `VNode::custom_button(VCustomButton::new(content_vbox).css_classes(classes).on_click(...))`
  - Content: VBox::vertical(2) with day label and dots row
  - CSS classes computed from props: `"calendar-day-cell"`, conditional `"today"`, `"selected"`, `"other-month"`, `"has-events"`
  - Dots: VBox::horizontal(2) with up to 3 small VBox dot elements (each 4x4 with `"calendar-event-dot"` class) -- note: VBox lacks width_request/height_request; use css min-width/min-height instead, or keep dots as imperative detail inside the render
  - On click: clone the `emit` callback and fire `DayCellOutput::Clicked(day)`
- Export `type DayCellComponent = RenderComponent<DayCellRender>;`

**Design decision on dots:** The dot boxes need `width_request(4)` and `height_request(4)` which VBox doesn't support. Two options: (a) add those as CSS `min-width`/`min-height` in the stylesheet, or (b) keep DayCell imperative and only do the rename. Choose (a) -- add CSS rules `.calendar-event-dot { min-width: 4px; min-height: 4px; }` to the stylesheet. The existing `width_request`/`height_request` calls can be removed.

### 3. Move `month_grid.rs` to `crates/overview/src/ui/calendar/month_grid.rs`

Move with updated imports:
- Change `use super::day_cell::{DayCell, DayCellOutput, DayCellProps};` to `use super::day_cell::{DayCellComponent, DayCellOutput, DayCellProps};` or keep the `DayCell` alias
- Update `DayCell::new(&cell_props)` to `DayCellComponent::build(&cell_props)`
- Update `cell.connect_output(...)` pattern
- `cell.root` changes to `Component::widget(&cell)`
- Keep `days_in_month` function and its tests in this file

### 4. Move `calendar_component.rs` to `crates/overview/src/components/calendar.rs`

This is the smart container -- stays in `components/`:
- Update `use super::month_grid::...` to `use crate::ui::calendar::month_grid::...`
- Keep `month_name` function in this file
- Rename module from `calendar_component` to just `calendar`

### 5. Register `ui::calendar` in `crates/overview/src/ui/mod.rs`

Add `pub mod calendar;` to the module declarations.

### 6. Update `crates/overview/src/components/mod.rs`

Replace `pub mod calendar_grid;` with `pub mod calendar;`.

### 7. Update imports throughout the codebase

Search for `calendar_grid::CalendarComponent` and update to `calendar::CalendarComponent`. Known locations:
- `crates/overview/src/components/events.rs` (if it uses CalendarComponent)
- Any layout renderer file that references the calendar

### 8. Add CSS rules for calendar event dots

In the overview stylesheet, ensure `.calendar-event-dot` has `min-width: 4px; min-height: 4px;` so the RenderFn version doesn't need `width_request`/`height_request`.

### 9. Delete old `crates/overview/src/components/calendar_grid/` directory

Remove the entire old directory.

### 10. Run `cargo build --workspace` and `cargo test --workspace`

Verify compilation succeeds and `days_in_month` tests pass.
