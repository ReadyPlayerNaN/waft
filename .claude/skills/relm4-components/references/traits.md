# Relm4 Component Traits Reference

## SimpleComponent

Elm-inspired variant that separates view updates from input updates. **Recommended for most use cases.**

```rust
pub trait SimpleComponent: Sized + 'static {
    /// Message type the component accepts
    type Input: Debug;
    /// Message type the component emits to parent
    type Output: Debug;
    /// Parameter for initialization
    type Init;
    /// The top-level widget (usually gtk::Window)
    type Root: Debug + Clone;
    /// Storage for widgets created in view!
    type Widgets;

    // Required methods
    fn init_root() -> Self::Root;
    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self>;

    // Provided methods (override as needed)
    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {}
    fn update_view(&self, widgets: &mut Self::Widgets, sender: ComponentSender<Self>) {}
    fn shutdown(&mut self, widgets: &mut Self::Widgets, output: Sender<Self::Output>) {}
}
```

## Component

Full control trait with CommandOutput support for background task results.

```rust
pub trait Component: Sized + 'static {
    /// Messages from background commands
    type CommandOutput: Debug + Send + 'static;
    type Input: Debug + 'static;
    type Output: Debug + 'static;
    type Init;
    type Root: Debug + Clone;
    type Widgets: 'static;

    // Required
    fn init_root() -> Self::Root;
    fn init(init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self>;

    // Provided
    fn builder() -> ComponentBuilder<Self>;
    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root);
    fn update_cmd(&mut self, message: Self::CommandOutput, sender: ComponentSender<Self>, root: &Self::Root);
    fn update_view(&self, widgets: &mut Self::Widgets, sender: ComponentSender<Self>);
    fn shutdown(&mut self, widgets: &mut Self::Widgets, output: Sender<Self::Output>);
}
```

## AsyncComponent

For components requiring async initialization or message handling.

```rust
pub trait AsyncComponent: Sized + 'static {
    type CommandOutput: Debug + Send + 'static;
    type Input: Debug + 'static;
    type Output: Debug + 'static;
    type Init;
    type Root: Debug + Clone;
    type Widgets;

    // Required - note async fn
    fn init_root() -> Self::Root;
    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self>;

    // Provided - async update
    async fn update(&mut self, message: Self::Input, sender: AsyncComponentSender<Self>);
    fn update_view(&self, widgets: &mut Self::Widgets, sender: AsyncComponentSender<Self>);
    fn init_loading_widgets(root: Self::Root) -> Option<LoadingWidgets>;
}
```

## SimpleAsyncComponent

Simplified async variant without CommandOutput.

Use `#[relm4::component(async)]` macro:

```rust
#[relm4::component(async)]
impl SimpleAsyncComponent for MyApp {
    type Init = ();
    type Input = Msg;
    type Output = ();
    // ... async fn init, async fn update
}
```

## Key Structs

### ComponentSender<C>

```rust
impl<C: Component> ComponentSender<C> {
    // Send input message to self
    pub fn input(&self, message: C::Input);
    pub fn input_sender(&self) -> &Sender<C::Input>;

    // Send output message to parent
    pub fn output(&self, message: C::Output) -> Result<(), SendError>;
    pub fn output_sender(&self) -> &Sender<C::Output>;

    // Spawn background commands
    pub fn oneshot_command<Fut>(&self, future: Fut);
    pub fn spawn_command<Cmd>(&self, cmd: Cmd);
    pub fn spawn_oneshot_command<Cmd>(&self, cmd: Cmd);
}
```

### Controller<C>

Manage a child component from parent:

```rust
impl<C: Component> Controller<C> {
    pub fn sender(&self) -> &Sender<C::Input>;  // Send messages
    pub fn widget(&self) -> &C::Root;           // Access root widget
    pub fn emit(&self, event: C::Input);        // Send input message
    pub fn model(&self) -> Ref<'_, C>;          // Access model
    pub fn detach_runtime(&mut self);           // Keep running after drop
}
```

### RelmApp

```rust
impl<M> RelmApp<M> {
    pub fn new(app_id: &str) -> Self;
    pub fn from_app(app: impl IsA<Application>) -> Self;

    // Builder methods
    pub fn with_broker(self, broker: &'static MessageBroker<M>) -> Self;
    pub fn with_args(self, args: Vec<String>) -> Self;
    pub fn visible_on_activate(self, visible: bool) -> Self;

    // Run the application
    pub fn run<C>(self, payload: C::Init);       // Sync component
    pub fn run_async<C>(self, payload: C::Init); // Async component
}
```

## Choosing the Right Trait

| Use Case | Trait |
|----------|-------|
| Simple UI, no async | `SimpleComponent` |
| Need CommandOutput | `Component` |
| Async init or update | `AsyncComponent` or `SimpleAsyncComponent` |
| Child component in parent | Store as `Controller<ChildModel>` |
