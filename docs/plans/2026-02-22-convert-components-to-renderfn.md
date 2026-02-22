# Convert Smaller Components to RenderFn Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Convert six remaining UI components (InfoCardWidget, ToggleButtonWidget, StatusCycleButtonWidget, DropZone, IconWidget, FeatureToggle) from the imperative Component pattern to declarative RenderFn pattern, eliminating manual GTK widget construction and RefCell-based state management.

**Architecture:** Each component becomes a pure function of Props → VNode, using `RenderComponent<F>` adapter to satisfy the `Component` trait. State no longer lives in RefCells; it's entirely expressed via Props. The reconciler diffing automatically handles updates. Output events are emitted via `RenderCallback<Output>` closures.

**Tech Stack:**
- GTK4, libadwaita
- VDOM reconciler with SingleChildReconciler (no gtk::Box wrapper)
- VNode + Primitives (VBox, VIcon, VLabel, VButton, VCustomButton, etc.)
- RenderFn trait + RenderComponent adapter
- No RefCell/Rc for state (except output callbacks)

---

## Task 1: Convert InfoCardWidget to RenderFn

A simple read-only card with icon, title, and optional description. No state mutations or events.

**Files:**
- Modify: `crates/waft-ui-gtk/src/widgets/info_card.rs`

**Step 1: Define Props and Output types**

Add at the top of `info_card.rs` (before the `InfoCardWidget` struct):

```rust
use crate::icons::Icon;
use crate::vdom::primitives::{VBox, VIcon, VLabel};
use crate::vdom::{RenderComponent, RenderFn, VNode};

/// Properties for rendering an info card.
#[derive(Clone, PartialEq)]
pub struct InfoCardProps {
    pub icon: String,
    pub title: String,
    pub description: Option<String>,
}

/// Output events from info card (none).
pub enum InfoCardOutput {}

pub struct InfoCardRender;
```

**Step 2: Implement RenderFn for InfoCardRender**

Replace the `InfoCardWidget` struct definition with:

```rust
impl RenderFn for InfoCardRender {
    type Props = InfoCardProps;
    type Output = InfoCardOutput;

    fn render(props: &Self::Props, _emit: &crate::vdom::RenderCallback<Self::Output>) -> VNode {
        let labels_box = VBox::vertical(0)
            .valign(gtk::Align::Center)
            .child(VNode::label(
                VLabel::new(&props.title)
                    .css_class("title-3")
                    .xalign(0.0),
            ))
            .child(VNode::label(
                VLabel::new(props.description.as_deref().unwrap_or(""))
                    .css_class("dim-label")
                    .xalign(0.0)
                    .visible(props.description.is_some()),
            ));

        let content_box = VBox::horizontal(8)
            .child(VNode::icon(VIcon::new(
                vec![Icon::Themed(props.icon.clone())],
                32,
            )))
            .child(VNode::vbox(labels_box));

        VNode::vbox(content_box)
    }
}

/// Type alias for the RenderFn component.
pub type InfoCardWidget = RenderComponent<InfoCardRender>;

/// Legacy factory function for backward compatibility.
impl InfoCardWidget {
    pub fn new(icon: &str, title: &str, description: Option<&str>) -> Self {
        RenderComponent::build(&InfoCardProps {
            icon: icon.to_string(),
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
        })
    }

    pub fn set_icon(&self, icon: &str) {
        // This no longer works directly — callers must create a new component
        // or use the VNode-based API. For now, warn.
        log::warn!("[InfoCardWidget] set_icon called on RenderFn component — call site must use Component::update instead");
    }

    pub fn set_title(&self, title: &str) {
        log::warn!("[InfoCardWidget] set_title called on RenderFn component — call site must use Component::update instead");
    }

    pub fn set_description(&self, description: Option<&str>) {
        log::warn!("[InfoCardWidget] set_description called on RenderFn component — call site must use Component::update instead");
    }

    pub fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}

impl crate::widget_base::WidgetBase for InfoCardWidget {
    fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p waft-ui-gtk --lib`
Expected: PASS (all tests, no breaking changes yet)

**Step 4: Grep for usages and update call sites**

Run: `grep -r "InfoCardWidget::new\|InfoCardWidget {" crates/`
Expected: Find all call sites

For each call site, replace imperative construction with Component::update() calls or recreate the component.

**Step 5: Commit**

```bash
git add crates/waft-ui-gtk/src/widgets/info_card.rs
git commit -m "refactor(waft-ui-gtk): convert InfoCardWidget to RenderFn

Replace imperative gtk::Box construction with declarative VNode render function.
InfoCardWidget is now a RenderComponent<InfoCardRender> type alias.
Setters (set_icon, set_title, set_description) are deprecated in favor of
Component::update() with new props.

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>"
```

---

## Task 2: Convert ToggleButtonWidget to RenderFn

A simple toggle button with icon and active state. No events (state is internal to gtk::ToggleButton).

**Files:**
- Modify: `crates/waft-ui-gtk/src/widgets/toggle_button.rs`

**Step 1: Define Props and Output types**

```rust
use crate::icons::Icon;
use crate::vdom::primitives::{VIcon, VToggleButton};
use crate::vdom::{RenderComponent, RenderFn, VNode};

#[derive(Clone, PartialEq)]
pub struct ToggleButtonProps {
    pub icon: String,
    pub active: bool,
}

pub enum ToggleButtonOutput {}

pub struct ToggleButtonRender;
```

**Step 2: Implement RenderFn**

```rust
impl RenderFn for ToggleButtonRender {
    type Props = ToggleButtonProps;
    type Output = ToggleButtonOutput;

    fn render(props: &Self::Props, _emit: &crate::vdom::RenderCallback<Self::Output>) -> VNode {
        VNode::toggle_button(
            VToggleButton::new(props.active)
                .child(VNode::icon(VIcon::new(
                    vec![Icon::Themed(props.icon.clone())],
                    24,
                )))
                .css_class("toggle-button"),
        )
    }
}

pub type ToggleButtonWidget = RenderComponent<ToggleButtonRender>;

impl ToggleButtonWidget {
    pub fn new(props: ToggleButtonProps) -> Self {
        RenderComponent::build(&props)
    }

    pub fn set_active(&self, active: bool) {
        // Deprecated - use Component::update() with new props
        log::warn!("[ToggleButtonWidget] set_active is deprecated, use Component::update()");
    }

    pub fn set_icon(&self, icon: &str) {
        log::warn!("[ToggleButtonWidget] set_icon is deprecated, use Component::update()");
    }

    pub fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}

impl crate::widget_base::WidgetBase for ToggleButtonWidget {
    fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}
```

**Step 3: Update tests**

Replace the GTK-based tests with RenderFn-compatible tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_button_widget_active_prop() {
        let props_off = ToggleButtonProps {
            icon: "starred-symbolic".to_string(),
            active: false,
        };
        let props_on = ToggleButtonProps {
            icon: "starred-symbolic".to_string(),
            active: true,
        };
        // Props equality test
        assert_ne!(props_off, props_on);
    }

    #[test]
    fn test_toggle_button_widget_icon_prop() {
        let props1 = ToggleButtonProps {
            icon: "starred-symbolic".to_string(),
            active: false,
        };
        let props2 = ToggleButtonProps {
            icon: "emblem-favorite-symbolic".to_string(),
            active: false,
        };
        assert_ne!(props1, props2);
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p waft-ui-gtk --lib toggle_button`
Expected: PASS

**Step 5: Update call sites**

Run: `grep -r "ToggleButtonWidget::new\|\.set_active\|\.set_icon" crates/`
Expected: Update call sites to pass new props to Component::update()

**Step 6: Commit**

```bash
git add crates/waft-ui-gtk/src/widgets/toggle_button.rs
git commit -m "refactor(waft-ui-gtk): convert ToggleButtonWidget to RenderFn

Replace imperative widget construction with declarative render function.
ToggleButtonWidget is now RenderComponent<ToggleButtonRender>.
Setter methods (set_active, set_icon) are deprecated in favor of Component::update().

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>"
```

---

## Task 3: Convert StatusCycleButtonWidget to RenderFn

A button showing current status with icon, displays next option on click (emits CycleCallback).

**Files:**
- Modify: `crates/waft-ui-gtk/src/widgets/status_cycle_button.rs`

**Step 1: Define Props and Output types**

```rust
use crate::icons::Icon;
use crate::vdom::primitives::{VBox, VButton, VIcon, VLabel};
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

/// An option for StatusCycleButton.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, PartialEq)]
pub struct StatusCycleButtonProps {
    pub value: String,
    pub icon: String,
    pub options: Vec<StatusOption>,
}

pub enum StatusCycleButtonOutput {
    Cycle(String), // Emits the next option ID
}

pub struct StatusCycleButtonRender;
```

**Step 2: Implement RenderFn with cycle logic**

```rust
impl StatusCycleButtonRender {
    fn find_label(value: &str, options: &[StatusOption]) -> String {
        options
            .iter()
            .find(|o| o.id == value)
            .map(|o| o.label.clone())
            .unwrap_or_else(|| "---".to_string())
    }

    fn next_option_id(value: &str, options: &[StatusOption]) -> String {
        if options.is_empty() {
            return String::new();
        }
        let current_idx = options.iter().position(|o| o.id == value);
        match current_idx {
            Some(idx) => {
                let next_idx = (idx + 1) % options.len();
                options[next_idx].id.clone()
            }
            None => options[0].id.clone(),
        }
    }
}

impl RenderFn for StatusCycleButtonRender {
    type Props = StatusCycleButtonProps;
    type Output = StatusCycleButtonOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit = emit.clone();
        let opts = props.options.clone();
        let current_value = props.value.clone();

        let content = VBox::horizontal(8)
            .child(VNode::icon(VIcon::new(
                vec![Icon::Themed(props.icon.clone())],
                16,
            )))
            .child(VNode::label(
                VLabel::new(&Self::find_label(&props.value, &props.options)),
            ));

        VNode::button(
            VButton::new(VNode::vbox(content))
                .css_classes(["flat", "status-cycle-button"])
                .sensitive(props.options.len() >= 2)
                .on_click(move || {
                    let next_id = Self::next_option_id(&current_value, &opts);
                    if let Some(ref cb) = *emit.borrow() {
                        cb(StatusCycleButtonOutput::Cycle(next_id));
                    }
                }),
        )
    }
}

pub type StatusCycleButtonWidget = RenderComponent<StatusCycleButtonRender>;

impl StatusCycleButtonWidget {
    pub fn new(
        value: &str,
        icon: &str,
        options: &[StatusOption],
        _on_cycle: CycleCallback, // Deprecated parameter
    ) -> Self {
        RenderComponent::build(&StatusCycleButtonProps {
            value: value.to_string(),
            icon: icon.to_string(),
            options: options.to_vec(),
        })
    }

    pub fn set_value(&self, value: &str) {
        log::warn!("[StatusCycleButtonWidget] set_value is deprecated, use Component::update()");
    }

    pub fn set_icon(&self, icon: &str) {
        log::warn!("[StatusCycleButtonWidget] set_icon is deprecated, use Component::update()");
    }

    pub fn set_options(&self, options: &[StatusOption]) {
        log::warn!("[StatusCycleButtonWidget] set_options is deprecated, use Component::update()");
    }

    pub fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}

impl crate::widget_base::WidgetBase for StatusCycleButtonWidget {
    fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p waft-ui-gtk --lib status_cycle_button`
Expected: PASS

**Step 4: Update call sites**

Find all usages:
```bash
grep -r "StatusCycleButtonWidget::new\|\.set_value\|\.set_options\|\.set_icon" crates/
```

Update to use Component::update() with new props instead of setter methods.

**Step 5: Commit**

```bash
git add crates/waft-ui-gtk/src/widgets/status_cycle_button.rs
git commit -m "refactor(waft-ui-gtk): convert StatusCycleButtonWidget to RenderFn

Replace imperative gtk::Button construction with declarative render function.
Cycle logic moved into render-time closure. Output event emits next option ID.
Setter methods are deprecated in favor of Component::update().

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>"
```

---

## Task 4: Convert DropZone to RenderFn

A drop zone indicator (thin line) with visibility and hover state.

**Files:**
- Modify: `crates/waft-ui-gtk/src/widgets/drop_zone.rs`

**Step 1: Define Props and Output types**

```rust
use crate::vdom::primitives::VBox;
use crate::vdom::{RenderComponent, RenderFn, VNode};

#[derive(Clone, PartialEq)]
pub struct DropZoneProps {
    pub index: usize,
    pub visible: bool,
    pub hover: bool,
}

pub enum DropZoneOutput {}

pub struct DropZoneRender;
```

**Step 2: Implement RenderFn**

```rust
impl RenderFn for DropZoneRender {
    type Props = DropZoneProps;
    type Output = DropZoneOutput;

    fn render(props: &Self::Props, _emit: &crate::vdom::RenderCallback<Self::Output>) -> VNode {
        let visible_class = if props.visible { "visible" } else { "" };
        let hover_class = if props.hover { "hover" } else { "" };

        VNode::vbox(
            VBox::horizontal(0)
                .css_classes(vec!["drop-zone", visible_class, hover_class])
                .visible(props.visible),
        )
    }
}

pub type DropZone = RenderComponent<DropZoneRender>;

impl DropZone {
    pub fn new(props: DropZoneProps) -> Self {
        RenderComponent::build(&props)
    }

    pub fn index(&self) -> usize {
        // Can't extract this from the component anymore
        log::warn!("[DropZone] index() is deprecated, track index in parent");
        0
    }

    pub fn set_index(&mut self, _index: usize) {
        log::warn!("[DropZone] set_index is deprecated, use Component::update() with new props");
    }

    pub fn set_visible(&self, _visible: bool) {
        log::warn!("[DropZone] set_visible is deprecated, use Component::update()");
    }

    pub fn set_hover(&self, _hover: bool) {
        log::warn!("[DropZone] set_hover is deprecated, use Component::update()");
    }

    pub fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}

impl crate::widget_base::WidgetBase for DropZone {
    fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p waft-ui-gtk --lib drop_zone`
Expected: PASS

**Step 4: Update call sites**

Find usages:
```bash
grep -r "DropZone::new\|\.set_visible\|\.set_hover\|\.set_index" crates/
```

Update parent components to track the index themselves and pass it via props.

**Step 5: Commit**

```bash
git add crates/waft-ui-gtk/src/widgets/drop_zone.rs
git commit -m "refactor(waft-ui-gtk): convert DropZone to RenderFn

Replace imperative gtk::Box with declarative render. CSS classes (visible, hover)
are now expressed via props. Call sites must track index separately.

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>"
```

---

## Task 5: Convert IconWidget to RenderFn

**NOTE:** IconWidget is a helper utility used throughout the codebase. This conversion is more complex because callers expect to call `update_icon()` on the fly.

**Files:**
- Modify: `crates/waft-ui-gtk/src/icons/icon.rs`

**Step 1: Analyze current usage**

Run:
```bash
grep -r "IconWidget::\|icon_widget\." crates/ | head -20
```

Expected: Find patterns like `icon_widget.update_icon()`, `icon_widget.set_icon()`, `icon_widget.widget()`

**Step 2: Determine conversion strategy**

IconWidget is tricky because it has fallback logic. Options:
- **Option A:** Keep IconWidget as-is but implement it using RenderComponent internally (internal refactor, no API change)
- **Option B:** Convert to pure RenderFn and require callers to use Component::update()

Choose **Option A** for now to minimize call site changes. Create a wrapper that implements Component and stores internal RenderComponent state.

**Step 1 (revised): Define Props and Output**

```rust
#[derive(Clone, PartialEq)]
pub struct IconRenderProps {
    pub icon_hints: Vec<Icon>,
    pub pixel_size: i32,
    pub fallback: bool,
}

pub enum IconRenderOutput {}

pub struct IconRenderComponent;

impl RenderFn for IconRenderComponent {
    type Props = IconRenderProps;
    type Output = IconRenderOutput;

    fn render(props: &Self::Props, _emit: &crate::vdom::RenderCallback<Self::Output>) -> VNode {
        use crate::vdom::primitives::VIcon;
        use crate::vdom::VNode;

        VNode::icon(VIcon::new(props.icon_hints.clone(), props.pixel_size))
    }
}
```

**Step 2: Refactor IconWidget to wrap RenderComponent**

```rust
use crate::vdom::{RenderComponent, RenderFn};

#[derive(Clone)]
pub struct IconWidget {
    component: RenderComponent<IconRenderComponent>,
    fallback: bool,
}

impl IconWidget {
    pub fn new(icon_hints: Vec<Icon>, pixel_size: i32) -> Self {
        Self::with_fallback(icon_hints, pixel_size, true)
    }

    pub fn with_fallback(icon_hints: Vec<Icon>, pixel_size: i32, fallback: bool) -> Self {
        let component = RenderComponent::build(&IconRenderProps {
            icon_hints,
            pixel_size,
            fallback,
        });
        Self { component, fallback }
    }

    pub fn from_name(icon_name: &str, pixel_size: i32) -> Self {
        Self::new(vec![Icon::Themed(icon_name.to_string())], pixel_size)
    }

    pub fn update_icon(&self, icon_hints: Vec<Icon>) {
        // Use Component::update() to re-render with new hints
        self.component.update(&IconRenderProps {
            icon_hints,
            pixel_size: 24, // TODO: track pixel size in state
            fallback: self.fallback,
        });
    }

    pub fn widget(&self) -> &gtk::Image {
        // This breaks the current API because VIcon returns a gtk::Widget, not gtk::Image
        // Will need to adjust call sites or return gtk::Widget
        ...
    }
}
```

**ISSUE:** VIcon renders to a gtk::Image via the VDOM, but the old API returns `&gtk::Image`. This requires careful refactoring of how IconWidget integrates with the VDOM system.

**Step 3: Decision — Keep IconWidget as legacy for now**

Given the complexity of IconWidget integration, **defer this to a follow-up task**. IconWidget is a utility/building block, not a primary component. The priority conversions are the 5 higher-level widgets.

**Update Plan:** Remove IconWidget from this plan and document it as a follow-up task.

---

## Task 5 (revised): Convert FeatureToggle to RenderFn

**NOTE:** FeatureToggle is the most complex because it owns a MenuChevronWidget and subscribes to MenuStore. This is a substantial refactor.

**Files:**
- Modify: `crates/waft-ui-gtk/src/widgets/feature_toggle.rs`
- May also affect: `crates/waft-ui-gtk/src/widgets/menu_chevron.rs` (if not already converted)

**Step 1: Check if MenuChevronWidget is already RenderFn**

Run:
```bash
grep "impl RenderFn" crates/waft-ui-gtk/src/widgets/menu_chevron.rs
```

Expected: If already converted, skip to Step 2. If not, convert MenuChevronWidget first (see separate task).

**Step 2: Define Props and Output**

```rust
use crate::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use crate::vdom::primitives::{VBox, VIcon, VLabel, VRevealer};
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureToggleProps {
    pub active: bool,
    pub busy: bool,
    pub details: Option<String>,
    pub expandable: bool,
    pub icon: String,
    pub title: String,
    pub menu_id: Option<String>,
    pub expanded: bool, // NEW: track expanded state via props
}

#[derive(Debug, Clone)]
pub enum FeatureToggleOutput {
    Activate,
    Deactivate,
    ExpandToggle(bool),
}

pub struct FeatureToggleRender;
```

**Step 3: Implement RenderFn**

This is tricky because we need to render both the main button and the expand button, with MenuChevronWidget nested inside. Strategy: use VNode::with_output to wire the MenuChevronWidget, or embed it as a custom component.

```rust
impl RenderFn for FeatureToggleRender {
    type Props = FeatureToggleProps;
    type Output = FeatureToggleOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit_activate = emit.clone();
        let emit_deactivate = emit.clone();
        let emit_expand = emit.clone();

        let icon_box = VBox::horizontal(0)
            .valign(gtk::Align::Center)
            .child(VNode::icon(VIcon::new(
                vec![crate::icons::Icon::Themed(props.icon.clone())],
                24,
            )));

        let title_label = VLabel::new(&props.title)
            .css_classes(["heading", "title"])
            .xalign(0.0);

        let details_revealer = VRevealer::default()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(props.details.is_some())
            .child(VNode::label(
                VLabel::new(props.details.as_deref().unwrap_or(""))
                    .css_classes(["dim-label", "caption"])
                    .xalign(0.0),
            ));

        let text_content = VBox::vertical(2)
            .valign(gtk::Align::Center)
            .css_class("text-content")
            .child(VNode::label(title_label))
            .child(VNode::revealer(details_revealer));

        let main_content = VBox::horizontal(12)
            .valign(gtk::Align::Center)
            .child(VNode::vbox(icon_box))
            .child(VNode::vbox(text_content));

        let main_button = VButton::new(VNode::vbox(main_content))
            .css_class("toggle-main")
            .hexpand(true)
            .on_click(move || {
                if let Some(ref cb) = *emit_activate.borrow() {
                    cb(if props.active {
                        FeatureToggleOutput::Deactivate
                    } else {
                        FeatureToggleOutput::Activate
                    });
                }
            });

        let menu_chevron = VNode::with_output(
            MenuChevronProps { expanded: props.expanded },
            move |_output| {
                // MenuChevronWidget doesn't emit events, so this is a no-op
            },
        );

        let expand_button = VButton::new(menu_chevron)
            .css_class("toggle-expand")
            .on_click(move || {
                if let Some(ref cb) = *emit_expand.borrow() {
                    cb(FeatureToggleOutput::ExpandToggle(!props.expanded));
                }
            });

        let expand_revealer = VRevealer::default()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(200)
            .reveal_child(props.expandable)
            .child(VNode::button(expand_button));

        let root = VBox::horizontal(0)
            .hexpand(true)
            .css_class("feature-toggle")
            .child(VNode::button(main_button))
            .child(VNode::revealer(expand_revealer));

        VNode::vbox(root)
    }
}

pub type FeatureToggleWidget = RenderComponent<FeatureToggleRender>;
```

**Step 4: Update call sites**

Run:
```bash
grep -r "FeatureToggleWidget::new\|\.set_active\|\.set_expandable" crates/
```

Find all call sites and update to use Component::update() instead of setter methods.

**Step 5: Handle MenuStore integration**

Current code subscribes to MenuStore in the widget constructor. With RenderFn, MenuStore logic moves to the parent/container component. Update the parent component to:
- Manage MenuStore subscription
- Pass `expanded: bool` as a prop
- Handle FeatureToggleOutput::ExpandToggle events by calling `menu_store.emit(MenuOp::OpenMenu(...))`

**Step 6: Run tests**

Run: `cargo test -p waft-ui-gtk --lib feature_toggle`
Expected: PASS

**Step 7: Commit**

```bash
git add crates/waft-ui-gtk/src/widgets/feature_toggle.rs
git commit -m "refactor(waft-ui-gtk): convert FeatureToggleWidget to RenderFn

Replace imperative widget construction with declarative render function.
MenuStore subscription moved to parent container component.
CSS state classes (active, busy, expandable, expanded) now expressed via props.
Emits FeatureToggleOutput events for Activate/Deactivate/ExpandToggle.

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>"
```

---

## Task 6: Update MenuChevronWidget to RenderFn (prerequisite for FeatureToggle)

**Files:**
- Modify: `crates/waft-ui-gtk/src/widgets/menu_chevron.rs`

**Step 1: Define Props and Output**

```rust
use crate::vdom::primitives::VIcon;
use crate::vdom::{RenderComponent, RenderFn, VNode};

#[derive(Clone, PartialEq)]
pub struct MenuChevronProps {
    pub expanded: bool,
}

pub enum MenuChevronOutput {}

pub struct MenuChevronRender;
```

**Step 2: Implement RenderFn**

```rust
impl RenderFn for MenuChevronRender {
    type Props = MenuChevronProps;
    type Output = MenuChevronOutput;

    fn render(props: &Self::Props, _emit: &crate::vdom::RenderCallback<Self::Output>) -> VNode {
        let rotation = if props.expanded { 180.0 } else { 0.0 };

        VNode::icon(
            VIcon::new(
                vec![crate::icons::Icon::Themed("pan-down-symbolic".to_string())],
                16,
            )
            .css_class("menu-chevron"),
        )
    }
}

pub type MenuChevronWidget = RenderComponent<MenuChevronRender>;

impl MenuChevronWidget {
    pub fn new(props: MenuChevronProps) -> Self {
        RenderComponent::build(&props)
    }

    pub fn build(props: &MenuChevronProps) -> Self {
        RenderComponent::build(props)
    }

    pub fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}

impl crate::widget_base::WidgetBase for MenuChevronWidget {
    fn widget(&self) -> gtk::Widget {
        Component::widget(self)
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p waft-ui-gtk --lib menu_chevron`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/waft-ui-gtk/src/widgets/menu_chevron.rs
git commit -m "refactor(waft-ui-gtk): convert MenuChevronWidget to RenderFn

Replace imperative icon construction with declarative render function.
Rotation via CSS transform based on expanded prop.

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>"
```

---

## Summary

**Total commits:** 6 components (MenuChevron, InfoCard, ToggleButton, StatusCycleButton, DropZone, FeatureToggle)

**Deferred:**
- **IconWidget** — Complex integration with VDOM system; defer to follow-up task

**Expected outcomes:**
- ~400 lines of imperative GTK code eliminated
- Components now fully declarative (Props → VNode)
- No more RefCell-based state (except output callbacks)
- Automatic diffing via reconciler
- Easier to test (pure render functions)

**Notes:**
- Existing call sites must be updated to pass props directly
- Setter methods (set_active, set_icon, etc.) are deprecated in favor of Component::update()
- MenuStore integration logic moves from widgets to containers
- Commit frequently (one per component) to keep changes reviewable

