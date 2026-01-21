# Relm4 Shared State Reference

## SharedState

Global state accessible from any component with automatic change notifications.

### Definition

```rust
use relm4::SharedState;

// Must be static
static APP_STATE: SharedState<AppState> = SharedState::new();

#[derive(Default, Clone, Debug)]
struct AppState {
    theme: Theme,
    user: Option<User>,
    settings: Settings,
}

#[derive(Default, Clone, Debug)]
enum Theme {
    #[default]
    Light,
    Dark,
    System,
}
```

### Reading State

```rust
fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
    // Read current state
    let state = APP_STATE.read();
    let theme = state.theme.clone();

    let model = MyComponent { theme };
    // ...
}

fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        Msg::CheckUser => {
            let state = APP_STATE.read();
            if let Some(user) = &state.user {
                // Use user data
            }
        }
    }
}
```

### Writing State

```rust
fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        Msg::SetTheme(theme) => {
            // Get write guard
            let mut state = APP_STATE.write();
            state.theme = theme;
            // Subscribers notified when guard drops
        }
        Msg::Login(user) => {
            APP_STATE.write().user = Some(user);
        }
        Msg::Logout => {
            APP_STATE.write().user = None;
        }
        Msg::UpdateSettings(settings) => {
            APP_STATE.write().settings = settings;
        }
    }
}
```

### Subscribing to Changes

```rust
fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
    // Subscribe to all state changes
    APP_STATE.subscribe(sender.input_sender(), |state| {
        Msg::StateChanged(state.clone())
    });

    // Or subscribe to specific field changes
    APP_STATE.subscribe(sender.input_sender(), |state| {
        Msg::ThemeChanged(state.theme.clone())
    });

    // ...
}

fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        Msg::StateChanged(state) => {
            self.theme = state.theme;
            self.logged_in = state.user.is_some();
        }
        Msg::ThemeChanged(theme) => {
            self.apply_theme(theme);
        }
    }
}
```

### SharedState API

```rust
impl<T: Default> SharedState<T> {
    /// Create new SharedState (must be const for static)
    pub const fn new() -> Self;

    /// Get read-only access
    pub fn read(&self) -> SharedStateReadGuard<'_, T>;

    /// Get read-write access (notifies on drop)
    pub fn write(&self) -> SharedStateWriteGuard<'_, T>;

    /// Subscribe to changes
    pub fn subscribe<Msg, F>(&self, sender: &Sender<Msg>, f: F)
    where
        F: Fn(&T) -> Msg + 'static + Send + Sync,
        Msg: Send + 'static;

    /// Get subscriber count
    pub fn subscriber_count(&self) -> usize;
}
```

### Guards

```rust
// Read guard - implements Deref
let guard: SharedStateReadGuard<'_, AppState> = APP_STATE.read();
let theme = &guard.theme;  // Access via Deref

// Write guard - implements Deref + DerefMut
let mut guard: SharedStateWriteGuard<'_, AppState> = APP_STATE.write();
guard.theme = Theme::Dark;  // Modify via DerefMut
// Subscribers notified here when guard drops
```

## Reducer Pattern

Alternative state management using reducer functions.

### Definition

```rust
use relm4::Reducer;

static COUNTER: Reducer<CounterState> = Reducer::new();

#[derive(Default)]
struct CounterState {
    value: i32,
}

// Actions for the reducer
enum CounterAction {
    Increment,
    Decrement,
    Set(i32),
}

impl Reducible for CounterState {
    type Input = CounterAction;

    fn reduce(&mut self, action: Self::Input) {
        match action {
            CounterAction::Increment => self.value += 1,
            CounterAction::Decrement => self.value -= 1,
            CounterAction::Set(v) => self.value = v,
        }
    }
}
```

### Using Reducer

```rust
fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        Msg::Increment => {
            COUNTER.emit(CounterAction::Increment);
        }
        Msg::Reset => {
            COUNTER.emit(CounterAction::Set(0));
        }
    }
}
```

## MessageBroker

Broadcast messages to multiple subscribers.

### Definition

```rust
use relm4::MessageBroker;

// Define at module level
static BROKER: MessageBroker<AppEvent> = MessageBroker::new();

#[derive(Debug, Clone)]
enum AppEvent {
    UserLoggedIn(User),
    UserLoggedOut,
    ThemeChanged(Theme),
    Notification(String),
}
```

### Publishing Events

```rust
fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
    match msg {
        Msg::Login(user) => {
            // Notify all subscribers
            BROKER.send(AppEvent::UserLoggedIn(user));
        }
        Msg::Logout => {
            BROKER.send(AppEvent::UserLoggedOut);
        }
        Msg::ShowNotification(text) => {
            BROKER.send(AppEvent::Notification(text));
        }
    }
}
```

### Subscribing to Events

```rust
fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
    // Subscribe to broker events
    BROKER.subscribe(sender.input_sender(), |event| {
        match event {
            AppEvent::UserLoggedIn(user) => Msg::UpdateUser(Some(user.clone())),
            AppEvent::UserLoggedOut => Msg::UpdateUser(None),
            AppEvent::Notification(text) => Msg::ShowToast(text.clone()),
            _ => Msg::Noop,
        }
    });

    // ...
}
```

### Using with RelmApp

```rust
fn main() {
    let app = RelmApp::new("com.example.app")
        .with_broker(&BROKER);  // Pass broker to app

    app.run::<App>(());
}
```

## Comparison

| Feature | SharedState | Reducer | MessageBroker |
|---------|-------------|---------|---------------|
| State storage | Yes | Yes | No |
| Multiple subscribers | Yes | Yes | Yes |
| Structured mutations | No | Yes (actions) | N/A |
| Event broadcasting | No | No | Yes |
| Best for | Simple shared data | Redux-style state | App-wide events |

## Best Practices

1. **Use SharedState for:**
   - User session/authentication
   - Theme/appearance settings
   - Application configuration

2. **Use Reducer for:**
   - Complex state with many update paths
   - When you want Redux-style predictability
   - State that needs action logging

3. **Use MessageBroker for:**
   - Notifications/toasts
   - Events that don't require state
   - Loose coupling between components

4. **Avoid:**
   - Storing UI-specific state in global state
   - Excessive subscriptions (performance)
   - Circular update loops
