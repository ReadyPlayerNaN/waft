# Agenda tomorrow toggle request

## Requested change

Add another toggle button to the overview agenda header.

### Placement

- Place it to the right of the existing **Hide past events** button.

### Icon

- Use `media-seek-forward-symbolic`.

### Behavior

- The new toggle should control visibility of **tomorrow events**.
- When the toggle is **on**, tomorrow events are shown.
- When the toggle is **off**, tomorrow events are hidden.

### Default state change

- Tomorrow events are currently visible by default.
- Change the default so that tomorrow events are **hidden by default**.

## Notes

This is a behavior/UI change in the overview agenda section.

## Constraints

- Do not implement.
- It should behave like the existing past-events toggle.
- It should stay entirely inside the overview UI.
- No API, daemon, plugin, threading, or persistence changes.
