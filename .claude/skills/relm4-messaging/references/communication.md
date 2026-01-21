# Relm4 Component Communication Reference

## Message Flow Overview

```
┌─────────────────────────────────────────────────────┐
│                    Parent Component                  │
│  ┌─────────────────────────────────────────────┐   │
│  │ Controller<Child>                            │   │
│  │  .emit(ChildInput)  ←── Send to child       │   │
│  │  .forward()         ←── Receive from child  │   │
│  └─────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
                          ↑↓
┌─────────────────────────────────────────────────────┐
│                    Child Component                   │
│  sender.input()    ←── Send to self                 │
│  sender.output()   ←── Send to parent               │
└─────────────────────────────────────────────────────┘
```

## ComponentSender API

```rust
impl<C: Component> ComponentSender<C> {
    // === Input Messages (to self) ===

    /// Send input message to this component
    pub fn input(&self, message: C::Input);

    /// Get sender for forwarding
    pub fn input_sender(&self) -> &Sender<C::Input>;

    // === Output Messages (to parent) ===

    /// Send output to parent component
    /// Returns Err if parent dropped the receiver
    pub fn output(&self, message: C::Output) -> Result<(), SendError>;

    /// Get sender for external use
    pub fn output_sender(&self) -> &Sender<C::Output>;

    // === Background Commands ===

    /// Spawn async task, result sent to update_cmd
    pub fn oneshot_command<Fut>(&self, future: Fut)
    where
        Fut: Future<Output = C::CommandOutput> + Send + 'static;

    /// Spawn with shutdown handling
    pub fn command<Cmd, Fut>(&self, cmd: Cmd)
    where
        Cmd: FnOnce(Sender<C::CommandOutput>, ShutdownReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send;

    /// Spawn blocking task on thread pool
    pub fn spawn_command<Cmd>(&self, cmd: Cmd)
    where
        Cmd: FnOnce() -> C::CommandOutput + Send + 'static;

    /// Spawn blocking with auto-shutdown
    pub fn spawn_oneshot_command<Cmd>(&self, cmd: Cmd)
    where
        Cmd: FnOnce() -> C::CommandOutput + Send + 'static;
}
```

## Parent-Child Communication

### Setting Up Child Component

```rust
struct Parent {
    child: Controller<Child>,
    child_value: i32,
}

#[derive(Debug)]
enum ParentMsg {
    // Messages forwarded from child
    ChildValueChanged(i32),
    ChildRequestedClose,
    // Messages to send to child
    ResetChild,
    UpdateChildConfig(Config),
}

fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
    // Launch child with initial value
    let child = Child::builder()
        .launch(42)  // Init value
        .forward(sender.input_sender(), |output| {
            // Transform ChildOutput → ParentMsg
            match output {
                ChildOutput::ValueChanged(v) => ParentMsg::ChildValueChanged(v),
                ChildOutput::RequestClose => ParentMsg::ChildRequestedClose,
            }
        });

    let model = Parent { child, child_value: 42 };

    // Access child's widget for layout
    let child_widget = model.child.widget();

    // ...
}
```

### Sending Messages to Child

```rust
fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        ParentMsg::ResetChild => {
            // Send input message to child
            self.child.emit(ChildInput::Reset);
        }
        ParentMsg::UpdateChildConfig(config) => {
            self.child.emit(ChildInput::SetConfig(config));
        }
        // Handle forwarded messages
        ParentMsg::ChildValueChanged(v) => {
            self.child_value = v;
        }
        _ => {}
    }
}
```

### Child Component Sending Output

```rust
#[derive(Debug)]
enum ChildInput {
    Increment,
    Reset,
    SetConfig(Config),
}

#[derive(Debug)]
enum ChildOutput {
    ValueChanged(i32),
    RequestClose,
}

fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
    match msg {
        ChildInput::Increment => {
            self.value += 1;
            // Notify parent of change
            let _ = sender.output(ChildOutput::ValueChanged(self.value));
        }
        ChildInput::Reset => {
            self.value = 0;
            let _ = sender.output(ChildOutput::ValueChanged(0));
        }
        _ => {}
    }
}
```

## Background Commands

### Simple Async Task (oneshot_command)

```rust
#[derive(Debug)]
enum CommandOutput {
    DataLoaded(Vec<Item>),
    LoadError(String),
}

fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
    match msg {
        Msg::LoadData => {
            self.loading = true;

            sender.oneshot_command(async {
                match fetch_items().await {
                    Ok(items) => CommandOutput::DataLoaded(items),
                    Err(e) => CommandOutput::LoadError(e.to_string()),
                }
            });
        }
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
        CommandOutput::DataLoaded(items) => {
            self.items = items;
            self.error = None;
        }
        CommandOutput::LoadError(err) => {
            self.error = Some(err);
        }
    }
}
```

### Cancellable Task (command with ShutdownReceiver)

```rust
fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
    match msg {
        Msg::StartPolling => {
            sender.command(|out, shutdown| async move {
                loop {
                    tokio::select! {
                        _ = shutdown.recv() => break,
                        result = fetch_update() => {
                            if out.send(CommandOutput::Update(result)).is_err() {
                                break;
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            });
        }
    }
}
```

### Blocking Task (spawn_command)

```rust
fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
    match msg {
        Msg::ProcessFile(path) => {
            self.processing = true;

            // Runs on thread pool, doesn't block UI
            sender.spawn_oneshot_command(move || {
                let content = std::fs::read_to_string(&path).ok();
                let processed = content.map(|c| expensive_processing(&c));
                CommandOutput::FileProcessed(processed)
            });
        }
    }
}
```

## Controller API

```rust
impl<C: Component> Controller<C> {
    /// Get input sender for the child
    pub fn sender(&self) -> &Sender<C::Input>;

    /// Send input message to child
    pub fn emit(&self, event: C::Input);

    /// Access child's root widget
    pub fn widget(&self) -> &C::Root;

    /// Access child's model (use sparingly)
    pub fn model(&self) -> Ref<'_, C>;

    /// Access child's widgets
    pub fn widgets(&self) -> Ref<'_, C::Widgets>;

    /// Watch for state changes
    pub fn state(&self) -> &StateWatcher<C>;

    /// Keep component running after Controller drops
    pub fn detach_runtime(&mut self);
}
```

## Multiple Children

```rust
struct App {
    sidebar: Controller<Sidebar>,
    content: Controller<Content>,
    dialog: Option<Controller<Dialog>>,
}

fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
    let sidebar = Sidebar::builder()
        .launch(())
        .forward(sender.input_sender(), |o| match o {
            SidebarOutput::ItemSelected(id) => AppMsg::SelectItem(id),
        });

    let content = Content::builder()
        .launch(())
        .forward(sender.input_sender(), |o| match o {
            ContentOutput::Modified => AppMsg::ContentModified,
        });

    let model = App { sidebar, content, dialog: None };
    // ...
}

fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
    match msg {
        AppMsg::SelectItem(id) => {
            // Forward selection to content
            self.content.emit(ContentInput::LoadItem(id));
        }
        AppMsg::ShowDialog => {
            // Create dialog on demand
            let dialog = Dialog::builder()
                .launch(())
                .forward(sender.input_sender(), |o| match o {
                    DialogOutput::Confirmed => AppMsg::DialogConfirmed,
                    DialogOutput::Cancelled => AppMsg::DialogCancelled,
                });
            self.dialog = Some(dialog);
        }
        AppMsg::DialogConfirmed | AppMsg::DialogCancelled => {
            // Close dialog
            self.dialog = None;
        }
    }
}
```
