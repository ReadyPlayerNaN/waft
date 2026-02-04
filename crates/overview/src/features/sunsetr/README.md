# Sunsetr Plugin

Provides a toggle button to control night light (color temperature adjustment) using [sunsetr](https://github.com/just-paja/sunsetr).

## Plugin ID

```
plugin::sunsetr
```

## Configuration

```toml
[[plugins]]
id = "plugin::sunsetr"
```

### Options

This plugin currently has no configuration options.

## Requirements

- [sunsetr](https://github.com/just-paja/sunsetr) CLI tool must be installed and available in PATH

## Features

- Toggle night light on/off
- Displays next transition time (e.g., "Until: 6:30 AM")
- Monitors sunsetr state via IPC
- Shows busy state during transitions
