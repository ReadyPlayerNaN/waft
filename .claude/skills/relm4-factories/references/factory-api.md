# Relm4 Factory API Reference

## FactoryComponent Trait

```rust
pub trait FactoryComponent: Sized + 'static {
    /// Container widget type (gtk::Box, gtk::ListBox, etc.)
    type ParentWidget: FactoryView;
    /// Messages from background commands
    type CommandOutput: Debug + Send;
    /// Message type this item accepts
    type Input;
    /// Messages sent to parent component
    type Output;
    /// Initialization parameter
    type Init;
    /// Root widget of this item
    type Root: AsRef<Self::ParentWidget::Children> + Debug + Clone;
    /// Widget storage
    type Widgets;
    /// Index type (usually DynamicIndex)
    type Index;

    // Required methods
    fn init_model(init: Self::Init, index: &Self::Index, sender: FactorySender<Self>) -> Self;
    fn init_root(&self) -> Self::Root;
    fn init_widgets(&mut self, root: Self::Root, sender: &FactorySender<Self>) -> Self::Widgets;

    // Provided methods
    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {}
    fn update_cmd(&mut self, message: Self::CommandOutput, sender: FactorySender<Self>) {}
    fn update_view(&self, widgets: &mut Self::Widgets, sender: FactorySender<Self>) {}
    fn shutdown(&mut self, widgets: &mut Self::Widgets, output: Sender<Self::Output>) {}
}
```

## FactoryVecDeque<C>

Container for factory components, similar to `VecDeque`.

### Construction

```rust
let factory: FactoryVecDeque<MyItem> = FactoryVecDeque::builder()
    .launch(gtk::Box::default())  // Parent widget
    .forward(sender.input_sender(), |output| {
        // Transform item outputs to parent inputs
        match output {
            ItemOutput::Selected(idx) => ParentMsg::ItemSelected(idx),
            ItemOutput::Removed(idx) => ParentMsg::RemoveItem(idx),
        }
    });
```

### Read Operations (No Guard Needed)

```rust
// Get length
let count = factory.len();
let is_empty = factory.is_empty();

// Access items
if let Some(item) = factory.get(0) {
    println!("{:?}", item);
}

// Get the container widget
let widget = factory.widget();
```

### Write Operations (Require Guard)

```rust
// Get mutable guard
let mut guard = factory.guard();

// Add items
let index: DynamicIndex = guard.push_back(init_value);
let index: DynamicIndex = guard.push_front(init_value);
guard.insert(position, init_value);

// Remove items
let removed: Option<C> = guard.pop_back();
let removed: Option<C> = guard.pop_front();
let removed: Option<C> = guard.remove(index);

// Clear all
guard.clear();

// Reorder
guard.move_to(current_idx, target_idx);
guard.move_front(current_idx);
guard.move_back(current_idx);
guard.swap(idx_a, idx_b);

// Edit item in place
if let Some(item) = guard.get_mut(index) {
    item.value = new_value;
}

// Widgets automatically update when guard drops
```

## FactoryVecDequeGuard

RAII guard that tracks changes and updates widgets on drop.

```rust
impl<'a, C: FactoryComponent> FactoryVecDequeGuard<'a, C> {
    pub fn push_back(&mut self, init: C::Init) -> DynamicIndex;
    pub fn push_front(&mut self, init: C::Init) -> DynamicIndex;
    pub fn insert(&mut self, index: usize, init: C::Init) -> DynamicIndex;

    pub fn pop_back(&mut self) -> Option<C>;
    pub fn pop_front(&mut self) -> Option<C>;
    pub fn remove(&mut self, index: usize) -> Option<C>;
    pub fn clear(&mut self);

    pub fn move_to(&mut self, current: usize, target: usize);
    pub fn move_front(&mut self, current: usize);
    pub fn move_back(&mut self, current: usize);
    pub fn swap(&mut self, first: usize, second: usize);

    pub fn get(&self, index: usize) -> Option<&C>;
    pub fn get_mut(&mut self, index: usize) -> Option<&mut C>;
    pub fn back(&self) -> Option<&C>;
    pub fn back_mut(&mut self) -> Option<&mut C>;
    pub fn front(&self) -> Option<&C>;
    pub fn front_mut(&mut self) -> Option<&mut C>;

    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;

    // Manual render (usually not needed)
    pub fn drop(self);  // Triggers render
}
```

## DynamicIndex

Stable reference to a factory item that survives reordering.

```rust
impl DynamicIndex {
    /// Get current position as usize
    pub fn current_index(&self) -> usize;
}

// DynamicIndex implements Clone, Debug, PartialEq, Eq, Hash
```

### Why DynamicIndex?

```rust
// Problem with usize:
let idx = 2;  // Points to item C
factory.guard().remove(0);  // Remove item A
// Now idx=2 points to item D, not C!

// Solution with DynamicIndex:
let idx: DynamicIndex = factory.guard().push_back(item_c);
factory.guard().remove(0);  // Remove item A
// idx still refers to item C
let position = idx.current_index();  // Now returns 1
```

## FactorySender<C>

Send messages from factory items.

```rust
impl<C: FactoryComponent> FactorySender<C> {
    /// Send input to self
    pub fn input(&self, message: C::Input);
    pub fn input_sender(&self) -> &Sender<C::Input>;

    /// Send output to parent
    pub fn output(&self, message: C::Output) -> Result<(), SendError>;
    pub fn output_sender(&self) -> &Sender<C::Output>;

    /// Spawn background commands
    pub fn oneshot_command<Fut>(&self, future: Fut);
    pub fn spawn_command<Cmd>(&self, cmd: Cmd);
}
```

## FactoryHashMap<K, C>

HashMap-like container for keyed access.

```rust
let factory: FactoryHashMap<String, MyItem> = FactoryHashMap::builder()
    .launch(gtk::Box::default())
    .forward(sender.input_sender(), transform_fn);

// Insert with key
factory.guard().insert("key1".to_string(), init_value);

// Remove by key
factory.guard().remove(&"key1".to_string());

// Access by key
if let Some(item) = factory.get(&"key1".to_string()) {
    // ...
}
```

## Common Parent Widget Types

| Widget | Use Case |
|--------|----------|
| `gtk::Box` | Linear list (vertical/horizontal) |
| `gtk::ListBox` | Selectable list rows |
| `gtk::FlowBox` | Grid-like flow layout |
| `gtk::Stack` | Stacked pages |

## Async Factories

```rust
#[relm4::factory(async)]
impl AsyncFactoryComponent for AsyncItem {
    type Init = String;
    type Input = ItemMsg;
    type Output = ItemOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    async fn init_model(
        init: Self::Init,
        _index: &DynamicIndex,
        _sender: AsyncFactorySender<Self>,
    ) -> Self {
        let data = fetch_data(&init).await;
        Self { data }
    }

    // ... rest similar to sync version
}
```

Use `AsyncFactoryVecDeque<C>` as the container.
