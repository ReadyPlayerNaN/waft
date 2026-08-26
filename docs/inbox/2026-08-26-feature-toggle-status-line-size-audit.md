# Feature toggle status-line size audit

## Summary

Audit result: I do **not** see a CSS rule or component branch that explicitly makes feature toggles with a second line shorter than toggles without one.

What I **can** confirm is:

1. both variants use the same outer button structure
2. both variants use the same `min-height: 48px`
3. the two-line variant packs more content into that same height
4. the details/status line is not styled by the intended CSS selector because the class names do not match

So the most grounded diagnosis is that the two-line toggles are visually more cramped inside the same height, rather than having a separate smaller outer height rule.

## Files inspected

- `crates/waft-ui-gtk/src/widgets/feature_toggle.rs`
- `crates/waft-ui-gtk/src/vdom/reconciler.rs`
- `crates/waft-ui-gtk/src/vdom/container.rs`
- `crates/overview/src/ui/main_window.rs`
- `crates/waft-ui-gtk/src/widgets/feature_grid.rs`

## Confirmed findings

### 1. Same outer widget path for both variants

In `crates/waft-ui-gtk/src/widgets/feature_toggle.rs`, both one-line and two-line toggles are rendered through the same `FeatureToggleWidget` / `FeatureToggleRender` path.

The main clickable surface is always a `VCustomButton`, which becomes a `gtk::Button` via the VDOM reconciler:

- `feature_toggle.rs` creates `VCustomButton::new(...).css_class("toggle-main")`
- `reconciler.rs` builds that as a `gtk::Button`
- `container.rs` places exactly one child into the button with `set_child(Some(widget))`

There is no alternate rendering path for the two-line version.

### 2. Same outer CSS height contract

In `crates/overview/src/ui/main_window.rs`, the main feature toggle button always gets:

```css
.feature-toggle .toggle-main {
    min-height: 48px;
    padding: 2px 20px 2px 12px;
}
```

So both variants share:

- same `min-height`
- same top/bottom padding
- same outer button class

I did not find a CSS rule that says “if details are present, make the button shorter”.

### 3. Two-line toggles pack more content into the same height

In `feature_toggle.rs`, the text column is built as:

- title label
- details revealer
- vertical spacing `2`

Code shape:

```rust
let mut text_box = VBox::vertical(2).valign(gtk::Align::Center);
text_box = text_box.child(VNode::label(title));
text_box = text_box.child(details_revealer);
```

This means the two-line toggle must fit:

- title
- second line
- inter-line spacing

inside the same minimum-height pill that the one-line toggle uses.

This strongly suggests the visible effect is:

- one-line toggles have more empty vertical breathing room
- two-line toggles look denser / more compressed

That is the most grounded code-based explanation found in this audit.

### 4. The intended details CSS rule is dead

In `main_window.rs`, CSS defines:

```css
.feature-toggle .toggle-main .details {
  font-size: 14px;
  margin: 0;
  padding: 0;
}
```

But in `feature_toggle.rs`, the details label is created with classes:

```rust
.css_class("dim-label")
.css_class("caption")
```

There is **no `details` class** applied.

So the intended details-specific CSS rule does not actually match anything.

This is a real bug / mismatch, even if it may not fully explain the visual size complaint on its own.

## Findings that were checked and rejected

### 1. The details `Revealer` is probably not the main culprit

The details line is wrapped in a `gtk::Revealer`.

However:

- when details are absent, it is created with `reveal = false`
- when details are present, it is created with `reveal = true`

A collapsed revealer should not be a convincing explanation for:

- “toggles with second line are smaller than toggles without one”

If the revealer were the main factor, the expectation would tend toward the opposite direction.

### 2. No explicit CSS rule for smaller two-line toggles was found

I did not find:

- a selector targeting toggles-with-details specifically
- a selector reducing height for the detailed variant
- a component branch changing outer sizing based on whether `details` is `Some(_)`

## Current best diagnosis

The best grounded diagnosis from code inspection is:

1. both toggle variants share the same outer minimum height
2. the detailed variant contains more vertical content inside that same pill
3. the status/details text is not receiving the intended `.details` styling because the class is never applied

Therefore the visual mismatch is most likely caused by:

- the same fixed/minimum height being reused for both one-line and two-line content
- very small vertical padding (`2px` top and bottom)
- missing details-specific CSS class hookup

## Conclusion

I cannot truthfully confirm that CSS contains a direct rule that makes the two-line toggles smaller.

I **can** confirm:

- the details CSS selector is broken / unused
- the two-line toggles are forced to fit more content into the same minimum-height button

So the likely problem is **cramped layout inside a shared height**, not a dedicated smaller-height rule for status-line toggles.
