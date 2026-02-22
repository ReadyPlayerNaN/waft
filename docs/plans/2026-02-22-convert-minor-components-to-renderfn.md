# Convert Minor Components to RenderFn

## Why

Five widgets in `waft-ui-gtk` contain stateful behaviour that was previously
treated as incompatible with `RenderFn`. In each case the state can be moved
one level up — either into the vdom reconciler entry or into the existing
Component wrapper — leaving the visual rendering as a pure function of props.
Converting them reduces stored widget references, enables reconciler diffing,
and aligns more of the codebase with the virtual-DOM direction.

`OrderedListWidget` is the only genuine exception: its drag-and-drop mechanics
require gesture controllers attached to dynamically created child widgets, which
needs a `VDraggable` primitive that does not yet exist. It is deferred.

**Execution note:** Tasks in Group C and D depend on `VRevealer` from the
separate `2026-02-22-implement-vrevealer.md` plan. Execute that plan first, or
merge those tasks in.

## What Changes

- `AppResultRowWidget` → pure `RenderFn` (no state; straightforward)
- `SearchResultListWidget` → `RenderFn`; selection state lifts to
  `SearchPaneWidget` (the existing parent)
- New `VProgressBar` vdom primitive added to `waft-ui-gtk`
- `CountdownBarWidget` split: `CountdownBarRender` handles the visual bar as a
  `RenderFn`; the existing Component wrapper retains the timer
- New `VScale` vdom primitive with interaction tracking baked into its
  `ReconcilerEntry` (interacting flag, debounce, signal blocking)
- `SliderWidget` → `RenderFn` using `VCustomButton` + `VScale` + `VRevealer`;
  callers (`audio_sliders.rs`, `brightness_sliders.rs`) updated to prop-based
  API
- `NotificationCard` → `RenderFn` using `VRevealer` + child
  `CountdownBarRender`; `hidden` flag eliminated; callers
  (`notification_group.rs`, `toast_manager.rs`) updated
- `OrderedListWidget` deferred (needs `VDraggable` primitive)

## Affected Files

**Group A — no dependencies:**
- `crates/waft-ui-gtk/src/widgets/app_result_row.rs` — convert to `RenderFn`,
  add `Clone + PartialEq` to props
- `crates/waft-ui-gtk/src/widgets/search_result_list.rs` — convert to
  `RenderFn`; selection state removed
- `crates/waft-ui-gtk/src/widgets/search_pane.rs` — gains owned
  `selected: Rc<Cell<usize>>`; `select_next/prev` mutate it and call
  `result_list.update(props)`

**Group B — VProgressBar:**
- `crates/waft-ui-gtk/src/vdom/primitives.rs` — add `VProgressBar`
- `crates/waft-ui-gtk/src/vdom/vnode.rs` — add `VNodeKind::ProgressBar` +
  `VNode::progress_bar()`
- `crates/waft-ui-gtk/src/vdom/reconciler.rs` — add build + update for
  `ProgressBar`
- `crates/waft-ui-gtk/src/widgets/countdown_bar.rs` — create
  `CountdownBarRender` + `CountdownBarProps`; refactor Component to call
  `reconciler.reconcile()` on each tick

**Group C — VScale + VRevealer (VRevealer from separate plan):**
- `crates/waft-ui-gtk/src/vdom/primitives.rs` — add `VScale`
- `crates/waft-ui-gtk/src/vdom/vnode.rs` — add `VNodeKind::Scale` +
  `VNode::scale()`
- `crates/waft-ui-gtk/src/vdom/reconciler.rs` — add `ReconcilerEntry::Scale`,
  build + update for `Scale`
- `crates/waft-ui-gtk/src/widgets/slider.rs` — replace with
  `SliderRender`/`SliderRenderProps`/`SliderRenderOutput`; type alias
  `SliderWidget = RenderComponent<SliderRender>`
- `crates/overview/src/components/audio_sliders.rs` — update to prop-based
  `SliderWidget` API
- `crates/overview/src/components/brightness_sliders.rs` — update to
  prop-based `SliderWidget` API

**Group D — VRevealer + CountdownBarRender:**
- `crates/waft-ui-gtk/src/widgets/notification_card.rs` — replace with
  `NotificationCardRender`/`NotificationCardProps`; type alias
  `NotificationCard = RenderComponent<NotificationCardRender>`
- `crates/overview/src/components/notification_group.rs` — update to
  prop-based `NotificationCard` API (pass `revealed: bool` instead of
  calling `show()`/`hide_and_remove()`)
- `crates/toasts/src/toast_manager.rs` — same prop-based update

## Tasks

### 1. Convert `AppResultRowWidget` to `RenderFn`

- [ ] 1.1 Add `Clone, PartialEq` derives to `AppResultRowProps` in
      `crates/waft-ui-gtk/src/widgets/app_result_row.rs`
- [ ] 1.2 Rename the existing struct to `AppResultRowRender` and implement
      `RenderFn for AppResultRowRender` returning a `VNode::vbox(...)` with
      `VIcon` (48px) and two `VLabel` children (name + optional description).
      No `Output` enum needed.
- [ ] 1.3 Add type alias `pub type AppResultRowWidget =
      RenderComponent<AppResultRowRender>;` replacing the old struct export
- [ ] 1.4 Run `cargo build -p waft-ui-gtk` to confirm no regressions

### 2. Convert `SearchResultListWidget` to `RenderFn`

- [ ] 2.1 Define `SearchResultListProps { items: Vec<AppResultRowProps>,
      selected: usize }` in `search_result_list.rs` with `Clone + PartialEq`
- [ ] 2.2 Implement `SearchResultListRender` as `RenderFn`: render a
      `VNode::vbox` containing one `VCustomButton` per item (css class
      `app-result-btn`, `selected` added when index == props.selected); each
      button's child is `AppResultRowWidget`; `on_click` emits
      `SearchResultListOutput::Activated(index)`
- [ ] 2.3 Remove `SearchResultListState`, `select_next`, `select_prev`,
      `selected_index`, `set_items` from the old struct; add type alias
      `pub type SearchResultListWidget = RenderComponent<SearchResultListRender>;`
- [ ] 2.4 In `search_pane.rs`: add `selected: Rc<Cell<usize>>` field to
      `SearchPaneWidget`; update `set_results()` to reset selected to `0` and
      call `self.result_list.update(SearchResultListProps { items, selected: 0
      })`; update `select_next()` and `select_prev()` to mutate `selected` and
      call `self.result_list.update(props)`; remove `selected_index()` or
      delegate to the owned cell
- [ ] 2.5 Run `cargo build --workspace` to confirm no regressions

### 3. Add `VProgressBar` primitive

- [ ] 3.1 Add `VProgressBar { fraction: f64, css_classes: Vec<String>,
      visible: bool }` struct + builder methods (`.fraction`, `.css_class`,
      `.visible`) to `crates/waft-ui-gtk/src/vdom/primitives.rs`
- [ ] 3.2 Add `VNodeKind::ProgressBar(VProgressBar)` variant to
      `crates/waft-ui-gtk/src/vdom/vnode.rs` and add
      `VNode::progress_bar(v: VProgressBar) -> Self` constructor
- [ ] 3.3 Add `KindTag::ProgressBar` variant + match arm to `reconciler.rs`
- [ ] 3.4 Add `ReconcilerEntry::ProgressBar { widget: gtk::ProgressBar }` to
      `reconciler.rs`
- [ ] 3.5 Implement `build_progress_bar_entry`: create `gtk::ProgressBar`,
      apply `fraction`, `css_classes`, `visible`
- [ ] 3.6 Implement update match arm: apply changed `fraction`, `css_classes`,
      `visible`
- [ ] 3.7 Run `cargo test -p waft-ui-gtk` to confirm existing tests pass

### 4. Split `CountdownBarWidget` into render + timer wrapper

- [ ] 4.1 Define `CountdownBarProps { fraction: f64, paused: bool }` with
      `Clone + PartialEq` in `countdown_bar.rs`
- [ ] 4.2 Implement `CountdownBarRender` as `RenderFn`: render
      `VNode::progress_bar(VProgressBar::new(props.fraction).css_class(
      "notification-progress").css_class_if(props.paused, "paused"))`.
      No `Output` enum needed (elapsed is handled by the wrapper).
- [ ] 4.3 Add type alias `type CountdownBarComponent =
      RenderComponent<CountdownBarRender>;`
- [ ] 4.4 Refactor `CountdownBarWidget`: replace `root: gtk::ProgressBar` with
      `inner: CountdownBarComponent`; add `paused: Arc<AtomicBool>` shared with
      hover controller; on each timer tick call
      `self.inner.update(CountdownBarProps { fraction, paused:
      self.paused.load(Ordering::SeqCst) })`. Keep `start`, `stop`, `pause`,
      `resume`, `connect_output`, `running_handle`, `Drop` unchanged.
- [ ] 4.5 Update `CountdownBarWidget::root` accessor to return
      `self.inner.widget()` (keep `pub root` as a `gtk::Widget` getter so
      `NotificationCard` callers compile unchanged)
- [ ] 4.6 Run `cargo build --workspace`

### 5. Add `VScale` primitive with interaction tracking

- [ ] 5.1 Add `VScale { value: f64, css_classes: Vec<String>,
      on_value_change: Option<Rc<dyn Fn(f64)>>,
      on_value_commit: Option<Rc<dyn Fn(f64)>> }` struct + builder methods
      (`.value`, `.css_class`, `.on_value_change`, `.on_value_commit`) to
      `primitives.rs`
- [ ] 5.2 Add `VNodeKind::Scale(VScale)` + `VNode::scale()` constructor to
      `vnode.rs`
- [ ] 5.3 Add `KindTag::Scale` to `reconciler.rs`
- [ ] 5.4 Add `ReconcilerEntry::Scale { widget: gtk::Scale,
      scale_wrapper: gtk::Box, interacting: Rc<RefCell<bool>>,
      pointer_down: Rc<RefCell<bool>>,
      debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
      handler_id: glib::SignalHandlerId,
      on_value_change: Option<Rc<dyn Fn(f64)>>,
      on_value_commit: Option<Rc<dyn Fn(f64)>> }` to `reconciler.rs`
- [ ] 5.5 Implement `build_scale_entry(vs: VScale) -> ReconcilerEntry`:
      extract the `new()` body from the current `SliderWidget` that builds
      the `gtk::Scale`, connects the `value-changed` signal, attaches
      `GestureClick` (press/release/cancel) and `EventControllerScroll`; store
      all interaction state in the entry fields
- [ ] 5.6 Implement update match arm for `Scale`: if `*interacting.borrow()`
      skip the value update; otherwise block signal, call `scale.set_value(v *
      100.0)`, unblock; update `on_value_change` / `on_value_commit` callbacks
      in place
- [ ] 5.7 Run `cargo test -p waft-ui-gtk`

### 6. Convert `SliderWidget` to `RenderFn`

*Depends on task 5 (VScale) and the `implement-vrevealer` plan (VRevealer).*

- [ ] 6.1 Define `SliderRenderProps { icon: String, value: f64, disabled:
      bool, expandable: bool, menu_id: Option<String> }` with `Clone +
      PartialEq` and `SliderRenderOutput { ValueChanged(f64),
      ValueCommit(f64), IconClick }` in `slider.rs`
- [ ] 6.2 Implement `SliderRenderRender` as `RenderFn`:
      - root: `VNode::vbox(VBox::vertical(0).css_class("slider-row")
        .css_class_if(props.disabled, "disabled"))` with children:
        - controls box: `VBox::horizontal(8)` containing
          - icon button: `VCustomButton::new(VNode::icon(...)).css_class(
            "slider-icon").on_click(emit ValueChanged / IconClick)`
          - scale wrapper box: `VBox::horizontal(0).hexpand(true)` containing
            `VNode::scale(VScale::new(props.value).on_value_change(...).
            on_value_commit(...))`
          - expand revealer: `VNode::revealer(VRevealer::new(expand_btn).
            reveal(props.expandable).transition_type(SlideLeft).
            transition_duration(200))`
- [ ] 6.3 Add type alias `pub type SliderWidget = RenderComponent<SliderRenderRender>;`
- [ ] 6.4 Remove the old `SliderWidget` struct, `SliderProps`, all `set_*`
      methods, and the `schedule_interaction_end` helper (now in VScale
      reconciler entry)
- [ ] 6.5 Run `cargo build -p waft-ui-gtk`

### 7. Update `SliderWidget` callers

*Depends on task 6.*

- [ ] 7.1 In `audio_sliders.rs`: replace `SliderWidget::new(SliderProps {..})`
      with `SliderWidget::build(&SliderRenderProps {..})`; replace
      `slider.set_value(v)` with `slider.update(props_with_new_value)`; replace
      `slider.connect_value_change(cb)` / `slider.connect_value_commit(cb)` /
      `slider.connect_icon_click(cb)` with a single `slider.connect_output(|o|
      match o { .. })`; remove `widget` call (use `Component::widget()`)
- [ ] 7.2 Same changes in `brightness_sliders.rs`
- [ ] 7.3 Run `cargo build --workspace`

### 8. Convert `NotificationCard` to `RenderFn`

*Depends on the `implement-vrevealer` plan (VRevealer) and task 4
(CountdownBarRender).*

- [ ] 8.1 Define `NotificationCardProps { urn: Urn, title: String, description:
      String, icon_hints: Vec<NotificationIconHint>, actions:
      Vec<NotificationAction>, toast_ttl: Option<u64>, revealed: bool }` with
      `Clone + PartialEq` and `NotificationCardOutput { ActionClick(Urn,
      String), Close(Urn), TimedOut(Urn) }` in `notification_card.rs`
- [ ] 8.2 Implement `NotificationCardRender` as `RenderFn`:
      - outer VBox containing `VNode::revealer(VRevealer::new(card_content).
        reveal(props.revealed).transition_type(SlideDown).
        transition_duration(200))`
      - `card_content`: card box with header (VIcon 32px, title VLabel,
        description VLabel, close VCustomButton), conditional actions row,
        conditional CountdownBarComponent child
      - close button / right-click gesture / left-click gesture all emit
        the corresponding `NotificationCardOutput` via `emit`
      - No `hidden` flag needed: the parent controls `revealed` and ignores
        any output received after it sets `revealed: false`
- [ ] 8.3 Add type alias `pub type NotificationCard =
      RenderComponent<NotificationCardRender>;`
- [ ] 8.4 Remove old `NotificationCard` struct, `hidden` field, `show()`,
      `hide_and_remove()`, `update()`, `revealer()` methods
- [ ] 8.5 Run `cargo build -p waft-ui-gtk`

### 9. Update `NotificationCard` callers

*Depends on task 8.*

- [ ] 9.1 In `notification_group.rs`: replace `NotificationCard::new(..)`
      with `NotificationCard::build(&NotificationCardProps { .., revealed:
      true })`; replace `card.show()` with `card.update(props_with_revealed_true)`;
      replace `card.hide_and_remove()` with
      `card.update(props_with_revealed_false)`; replace `card.update(t, d)`
      with prop update; remove `card.revealer()` call (window resize callback
      moves into VRevealer's `on_transition_end` if supported, otherwise keep
      as a direct `gtk::Revealer` signal on the built widget)
- [ ] 9.2 Same changes in `crates/toasts/src/toast_manager.rs`
- [ ] 9.3 Run `cargo build --workspace` and `cargo test --workspace`

### 10. Deferred: `OrderedListWidget`

- [ ] 10.1 No action. Defer until a `VDraggable` primitive is designed.
      `OrderedListWidget` stays as an imperative `Component`.
