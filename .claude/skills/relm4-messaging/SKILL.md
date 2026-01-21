---
name: relm4-messaging
description: |
  CRITICAL: Use for Relm4 messaging and communication questions. Triggers on:
  ComponentSender, message forwarding, parent child communication,
  SharedState, Reducer, MessageBroker, background command,
  oneshot_command, spawn_command, relm4 async task,
  Controller emit, output message, input message,
  relm4 消息, 组件通信, 状态共享, 后台任务
---

# Relm4 Messaging Skill

> **Version:** relm4 0.10.1 | **Last Updated:** 2025-01-21
>
> Check for updates: https://crates.io/crates/relm4

You are an expert at the Rust `relm4` crate messaging and state management. Help users by:
- **Writing code**: Generate message passing and async patterns
- **Answering questions**: Explain communication patterns and shared state

## Documentation

Refer to the local files for detailed documentation:
- `./references/communication.md` - Parent-child communication and forwarding
- `./references/shared-state.md` - SharedState and Reducer patterns

## IMPORTANT: Documentation Completeness Check

**Before answering questions, Claude MUST:**

1. Read the relevant reference file(s) listed above
2. If file read fails or file is empty:
   - Inform user: "本地文档不完整，建议运行 `/sync-crate-skills relm4 --force` 更新文档"
   - Still answer based on SKILL.md patterns + built-in knowledge
3. If reference file exists, incorporate its content into the answer

## Key Patterns

### 1. Parent-Child Communication

```rust
// Child component outputs
#[derive(Debug)]
enum ChildOutput {
    ValueChanged(i32),
    RequestClose,
}

// Parent receives and transforms
struct Parent {
    child: Controller<Child>,
}

fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
    let child = Child::builder()
        .launch(initial_value)
        .forward(sender.input_sender(), |output| match output {
            ChildOutput::ValueChanged(v) => ParentMsg::ChildValue(v),
            ChildOutput::RequestClose => ParentMsg::CloseChild,
        });

    let model = Parent { child };
    // ...
}

fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        // Receive forwarded message
        ParentMsg::ChildValue(v) => self.value = v,
        // Send message to child
        ParentMsg::ResetChild => self.child.emit(ChildInput::Reset),
    }
}
```

### 2. Background Commands

```rust
#[derive(Debug)]
enum Msg {
    FetchData,
    DataReceived(String),
}

#[derive(Debug)]
enum CommandOutput {
    FetchComplete(String),
    FetchError(String),
}

fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
    match msg {
        Msg::FetchData => {
            self.loading = true;
            // Spawn async task
            sender.oneshot_command(async {
                match reqwest::get("https://api.example.com/data").await {
                    Ok(resp) => CommandOutput::FetchComplete(resp.text().await.unwrap()),
                    Err(e) => CommandOutput::FetchError(e.to_string()),
                }
            });
        }
        _ => {}
    }
}

fn update_cmd(
    &mut self,
    msg: Self::CommandOutput,
    _sender: ComponentSender<Self>,
    _root: &Self::Root,
) {
    self.loading = false;
    match msg {
        CommandOutput::FetchComplete(data) => self.data = Some(data),
        CommandOutput::FetchError(err) => self.error = Some(err),
    }
}
```

### 3. SharedState for Cross-Component Data

```rust
use relm4::SharedState;

// Define global state
static APP_STATE: SharedState<AppState> = SharedState::new();

#[derive(Default, Clone)]
struct AppState {
    theme: Theme,
    user: Option<User>,
}

// In component init
fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
    // Subscribe to state changes
    APP_STATE.subscribe(sender.input_sender(), |state| {
        Msg::StateChanged(state.clone())
    });

    // Read current state
    let current = APP_STATE.read();

    // ...
}

fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        Msg::SetTheme(theme) => {
            // Write to shared state (notifies all subscribers)
            APP_STATE.write().theme = theme;
        }
        Msg::StateChanged(state) => {
            // Handle state change from other component
            self.theme = state.theme;
        }
    }
}
```

## API Reference Table

| Type | Description | Usage |
|------|-------------|-------|
| `ComponentSender<C>` | Send messages to/from component | `sender.input()`, `sender.output()` |
| `Controller<C>` | Manage child component | `controller.emit()`, `controller.widget()` |
| `SharedState<T>` | Global shared state | `STATE.read()`, `STATE.write()` |
| `Reducer<T>` | State with reducer pattern | Alternative to SharedState |
| `MessageBroker<M>` | Broadcast messages | App-wide events |

## Deprecated Patterns (Don't Use)

| Deprecated | Correct | Notes |
|------------|---------|-------|
| Direct field sharing | SharedState | Type-safe, reactive |
| Manual channel creation | ComponentSender | Built-in messaging |
| Polling for updates | `.subscribe()` | Push-based updates |

## When Writing Code

1. Use `.forward()` to transform child outputs to parent inputs
2. Use `sender.oneshot_command()` for fire-and-forget async tasks
3. Use `sender.command()` with `ShutdownReceiver` for cancellable tasks
4. SharedState notifies subscribers when `.write()` guard drops
5. Clone values when forwarding - messages must be owned
6. Always handle both success and error in CommandOutput

## When Answering Questions

1. Parent → Child: Use `controller.emit(ChildInput::Msg)`
2. Child → Parent: Use `sender.output(ChildOutput::Msg)` + `.forward()`
3. Sibling components: Use SharedState or MessageBroker
4. Async operations: Use `oneshot_command` or `spawn_command`
5. SharedState is static - define with `static STATE: SharedState<T> = SharedState::new()`
