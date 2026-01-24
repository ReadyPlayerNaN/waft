# Clock Plugin

Displays the current date and time in the overlay header.

## Plugin ID

```
plugin::clock
```

## Configuration

```toml
[[plugins]]
id = "plugin::clock"
on_click = "gnome-calendar"  # optional
```

### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `on_click` | string | `""` | Shell command to run when the clock is clicked. When empty, the clock is not clickable or focusable. |

## Features

- Displays current time and date
- Updates every second
- Placed in the header slot of the overlay
