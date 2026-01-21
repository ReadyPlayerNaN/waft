---
name: relm4-factories
description: |
  CRITICAL: Use for Relm4 factory and collection questions. Triggers on:
  FactoryVecDeque, FactoryComponent, DynamicIndex, factory guard,
  relm4 list, relm4 collection, #[relm4::factory], FactoryHashMap,
  dynamic widget list, widget from collection, multiple widgets,
  relm4 工厂, 动态列表, 集合组件, 工厂模式
---

# Relm4 Factories Skill

> **Version:** relm4 0.10.1 | **Last Updated:** 2025-01-21
>
> Check for updates: https://crates.io/crates/relm4

You are an expert at the Rust `relm4` crate factories for managing dynamic widget collections. Help users by:
- **Writing code**: Generate FactoryComponent implementations
- **Answering questions**: Explain factory patterns, guards, and DynamicIndex

## Documentation

Refer to the local files for detailed documentation:
- `./references/factory-api.md` - FactoryVecDeque and FactoryComponent API

## IMPORTANT: Documentation Completeness Check

**Before answering questions, Claude MUST:**

1. Read the relevant reference file(s) listed above
2. If file read fails or file is empty:
   - Inform user: "本地文档不完整，建议运行 `/sync-crate-skills relm4 --force` 更新文档"
   - Still answer based on SKILL.md patterns + built-in knowledge
3. If reference file exists, incorporate its content into the answer

## Key Patterns

### 1. Basic FactoryComponent

```rust
struct Counter {
    value: u8,
}

#[derive(Debug)]
enum CounterMsg {
    Increment,
    Decrement,
}

#[derive(Debug)]
enum CounterOutput {
    Remove(DynamicIndex),
}

#[relm4::factory]
impl FactoryComponent for Counter {
    type Init = u8;
    type Input = CounterMsg;
    type Output = CounterOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 5,

            gtk::Label {
                #[watch]
                set_label: &self.value.to_string(),
            },

            gtk::Button {
                set_label: "+",
                connect_clicked => CounterMsg::Increment,
            },

            gtk::Button {
                set_label: "-",
                connect_clicked => CounterMsg::Decrement,
            },

            gtk::Button {
                set_label: "Remove",
                connect_clicked[sender, index] => move |_| {
                    sender.output(CounterOutput::Remove(index.clone())).unwrap();
                },
            },
        }
    }

    fn init_model(value: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { value }
    }

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        match msg {
            CounterMsg::Increment => self.value = self.value.wrapping_add(1),
            CounterMsg::Decrement => self.value = self.value.wrapping_sub(1),
        }
    }
}
```

### 2. Parent Component with Factory

```rust
struct App {
    counters: FactoryVecDeque<Counter>,
}

#[derive(Debug)]
enum AppMsg {
    AddCounter,
    RemoveCounter(DynamicIndex),
}

#[relm4::component]
impl SimpleComponent for App {
    type Init = ();
    type Input = AppMsg;
    type Output = ();

    view! {
        gtk::Window {
            set_title: Some("Factory Example"),

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                gtk::Button {
                    set_label: "Add Counter",
                    connect_clicked => AppMsg::AddCounter,
                },

                #[local_ref]
                counter_box -> gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 5,
                },
            }
        }
    }

    fn init(_: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let counters = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                CounterOutput::Remove(index) => AppMsg::RemoveCounter(index),
            });

        let model = App { counters };
        let counter_box = model.counters.widget();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            AppMsg::AddCounter => {
                self.counters.guard().push_back(0);
            }
            AppMsg::RemoveCounter(index) => {
                self.counters.guard().remove(index.current_index());
            }
        }
    }
}
```

### 3. Guard Pattern for Mutations

```rust
fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        // Each guard() call creates RAII guard
        // Widgets update when guard drops
        Msg::Add(value) => {
            self.items.guard().push_back(value);
        }
        Msg::Remove(idx) => {
            self.items.guard().remove(idx);
        }
        Msg::Clear => {
            self.items.guard().clear();
        }
        // Batch operations in single guard
        Msg::AddMany(values) => {
            let mut guard = self.items.guard();
            for v in values {
                guard.push_back(v);
            }
            // All items rendered when guard drops here
        }
        Msg::MoveToFront(idx) => {
            self.items.guard().move_front(idx);
        }
    }
}
```

## API Reference Table

| Type | Description | Usage |
|------|-------------|-------|
| `FactoryComponent` | Trait for factory-managed components | Implement for item type |
| `FactoryVecDeque<C>` | VecDeque-like container | Main factory container |
| `FactoryHashMap<K, C>` | HashMap-like container | Key-based access |
| `DynamicIndex` | Stable reference to item | Survives reordering |
| `FactorySender<C>` | Message sender for factory items | `sender.output()` |
| `FactoryVecDequeGuard` | RAII guard for mutations | `factory.guard()` |

## Deprecated Patterns (Don't Use)

| Deprecated | Correct | Notes |
|------------|---------|-------|
| Direct mutation without guard | `factory.guard().push_back()` | Guard ensures UI sync |
| Using `usize` for removal | `DynamicIndex` | Index may change |
| Multiple separate guard calls | Single guard for batch ops | Better performance |

## When Writing Code

1. Always use `guard()` for any mutation to FactoryVecDeque
2. Use `DynamicIndex` for stable references - it auto-updates on reorder
3. Clone `index` when capturing in closures: `[sender, index]`
4. Forward factory outputs to parent using `.forward()`
5. Use `#[local_ref]` in view! to reference factory widget
6. Call `index.current_index()` to get current usize position

## When Answering Questions

1. Factory = efficient widget generation from collections
2. Guard pattern ensures UI updates happen atomically on drop
3. DynamicIndex vs usize: DynamicIndex survives insert/remove operations
4. FactoryComponent is similar to SimpleComponent but for items in a list
5. Parent holds `FactoryVecDeque<ItemType>`, items implement `FactoryComponent`
