# OpenSpec -- Historical Archive

This directory contains archived specifications from Waft's development history. It serves as a record of **design decisions and implementation rationale**, not as current documentation.

## WARNING: Outdated Implementation Details

Most specifications in this archive pre-date the **2026-02-12 entity-based architecture rewrite**. Implementation details in these specs describe the old widget-based architecture (direct D-Bus subscriptions, `waft-ipc` widget protocol, relm4 components) and **do not reflect how the codebase works today**.

For current architecture documentation, see `CLAUDE.md` in the project root.

## Archive Structure

Completed changes live in `changes/archive/` using date-prefixed directories:

```
changes/archive/
    YYYY-MM-DD-change-name/
        proposal.md     # Why this change was needed
        design.md       # Technical decisions and approach
        tasks.md        # Work breakdown
        specs/          # Detailed capability specifications
```

Active (in-progress) changes live directly in `changes/`.

Shared capability specifications live in `specs/`.

## When to Reference

Look here when you need to understand **why** a decision was made:

- Why a particular D-Bus interface was chosen over alternatives
- Why a plugin was structured a certain way
- What trade-offs were considered for a feature
- What the original scope and non-goals were for a change

Do **not** look here for how things currently work -- the codebase and `CLAUDE.md` are the source of truth for that.

## What's Outdated

All specs dated before 2026-02-12 describe the old architecture:

- **Widget protocol** (`waft-ipc`) -- replaced by entity-based protocol (`waft-protocol`)
- **Direct D-Bus subscriptions in UI** -- replaced by plugin daemons providing entities to central daemon
- **relm4 component patterns** -- replaced by `Plugin` trait + `PluginRuntime` + `EntityNotifier`
- **Store-based state management** -- replaced by entity routing through central daemon
