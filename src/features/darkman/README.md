# Darkman Plugin

Provides a toggle button to switch between light and dark mode using [darkman](https://darkman.whynothugo.nl/).

## Plugin ID

```
plugin::darkman
```

## Configuration

```toml
[[plugins]]
id = "plugin::darkman"
```

### Options

This plugin currently has no configuration options.

## Requirements

- [darkman](https://darkman.whynothugo.nl/) must be installed and running as a DBus service

## Features

- Toggle between light and dark mode
- Monitors darkman state changes via DBus
- Shows busy state during mode transitions
