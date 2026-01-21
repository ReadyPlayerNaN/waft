---
name: relm4-components
description: |
  CRITICAL: Use for Relm4 component questions. Triggers on:
  relm4, SimpleComponent, AsyncComponent, Component trait, GTK4 GUI,
  Elm architecture, Model-View-Update, view! macro, #[relm4::component],
  ComponentParts, ComponentSender, init, update, update_view,
  relm4 组件, GTK Rust, 声明式 UI, 组件生命周期
---

# Relm4 Components Skill

> **Version:** relm4 0.10.1 | **Last Updated:** 2025-01-21
>
> Check for updates: https://crates.io/crates/relm4

You are an expert at the Rust `relm4` crate for building GTK4 GUIs. Help users by:
- **Writing code**: Generate Rust code following Elm architecture patterns
- **Answering questions**: Explain components, traits, macros, and lifecycle

## Documentation

Refer to the local files for detailed documentation:
- `./references/traits.md` - Component trait definitions and associated types
- `./references/view-macro.md` - view! macro syntax and features

## IMPORTANT: Documentation Completeness Check

**Before answering questions, Claude MUST:**

1. Read the relevant reference file(s) listed above
2. If file read fails or file is empty:
   - Inform user: "本地文档不完整，建议运行 `/sync-crate-skills relm4 --force` 更新文档"
   - Still answer based on SKILL.md patterns + built-in knowledge
3. If reference file exists, incorporate its content into the answer

## Key Patterns

### 1. Basic SimpleComponent

```rust
use gtk::prelude::*;
use relm4::prelude::*;

struct App {
    counter: u8,
}

#[derive(Debug)]
enum Msg {
    Increment,
    Decrement,
}

#[relm4::component]
impl SimpleComponent for App {
    type Init = u8;
    type Input = Msg;
    type Output = ();

    view! {
        gtk::Window {
            set_title: Some("Counter"),
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                gtk::Button {
                    set_label: "Increment",
                    connect_clicked => Msg::Increment,
                },
                gtk::Label {
                    #[watch]
                    set_label: &format!("Counter: {}", model.counter),
                },
            }
        }
    }

    fn init(counter: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = App { counter };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Msg::Increment => self.counter = self.counter.wrapping_add(1),
            Msg::Decrement => self.counter = self.counter.wrapping_sub(1),
        }
    }
}

fn main() {
    let app = RelmApp::new("relm4.example.counter");
    app.run::<App>(0);
}
```

### 2. Async Component

```rust
#[relm4::component(async)]
impl SimpleAsyncComponent for AsyncApp {
    type Init = ();
    type Input = AsyncMsg;
    type Output = ();

    view! {
        gtk::Window {
            gtk::Label {
                #[watch]
                set_label: model.data.as_deref().unwrap_or("Loading..."),
            }
        }
    }

    async fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let data = fetch_data().await;
        let model = AsyncApp { data: Some(data) };
        let widgets = view_output!();
        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>) {
        match msg {
            AsyncMsg::Refresh => self.data = Some(fetch_data().await),
        }
    }
}
```

### 3. View Macro with #[watch]

```rust
view! {
    gtk::Window {
        set_title: Some("My App"),
        set_default_size: (300, 200),

        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 5,
            set_margin_all: 10,

            gtk::Label {
                // #[watch] updates this whenever model changes
                #[watch]
                set_label: &model.text,
                #[watch]
                set_visible: model.show_label,
            },

            gtk::Button {
                set_label: "Click me",
                // Signal connection with message
                connect_clicked => Msg::ButtonClicked,
            },
        }
    }
}
```

## API Reference Table

| Type | Description | Usage |
|------|-------------|-------|
| `SimpleComponent` | Simplified trait for most components | Most common choice |
| `Component` | Full control trait with CommandOutput | Advanced use cases |
| `AsyncComponent` | Async init and update | Network/IO operations |
| `SimpleAsyncComponent` | Simplified async variant | Async without commands |
| `ComponentSender<C>` | Send messages to/from component | `sender.input()`, `sender.output()` |
| `ComponentParts<C>` | Contains model and widgets | Return from `init()` |
| `Controller<C>` | Manage child component externally | Store in parent model |
| `RelmApp` | Application runner | `RelmApp::new(id).run::<C>(init)` |

## Deprecated Patterns (Don't Use)

| Deprecated | Correct | Notes |
|------------|---------|-------|
| Manual widget creation | `view!` macro | Use declarative UI |
| `set_global_css` on RelmApp | `relm4::set_global_css()` | Use module function |
| Direct field access in view | `#[watch]` attribute | Enables reactive updates |

## When Writing Code

1. Always use `#[relm4::component]` macro for SimpleComponent implementations
2. Use `#[watch]` in view! macro for properties that depend on model state
3. Return `ComponentParts { model, widgets }` from `init()`
4. Use `view_output!()` macro to generate widgets struct
5. Keep `update()` pure - no side effects, only state changes
6. Use `wrapping_add/sub` for numeric counters to avoid overflow panics

## When Answering Questions

1. SimpleComponent is sufficient for 90% of use cases
2. Use AsyncComponent when init or update needs async operations
3. The Elm architecture: Model → Message → Update → View cycle
4. `#[watch]` triggers `update_view()` after every `update()` call
5. RelmApp requires a valid application ID (reverse domain notation)
