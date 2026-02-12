# Plugin Development Guide

**Note:** The legacy dynamic plugin system (`.so` files via `waft-plugin-api`) has been removed.
All plugins now use the **daemon architecture** with `waft-plugin-sdk`.

For developing new plugins, see the daemon plugin SDK in `crates/plugin-sdk/`
and the existing plugin implementations in `plugins/`.

For the daemon migration guide, see [plugin-daemon-migration-guide.md](plugin-daemon-migration-guide.md).
