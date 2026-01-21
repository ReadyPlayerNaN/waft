use relm4::prelude::*;

/// Trait for senders that can send output messages.
///
/// This trait abstracts over `ComponentSender<T>` and `FactorySender<T>`
/// to work with both sender types.
///
/// # Examples
///
/// Using with `ComponentSender`:
/// ```rust,ignore
/// use relm4::ComponentSender;
/// use crate::ui::events::send_or_log;
///
/// fn handle_simple_component(sender: &ComponentSender<MyComponent>, output: MyOutput) {
///     send_or_log(sender, output);
/// }
/// ```
///
/// Using with `FactorySender`:
/// ```rust,ignore
/// use relm4::FactorySender;
/// use crate::ui::events::send_or_log;
///
/// fn handle_factory_component(sender: &FactorySender<MyFactory>, output: MyOutput) {
///     send_or_log(sender, output);
/// }
/// ```
pub trait SendableOutput {
    type Output: std::fmt::Debug;

    fn send_output(&self, output: Self::Output) -> Result<(), Self::Output>;
}

impl<T> SendableOutput for ComponentSender<T>
where
    T: SimpleComponent,
{
    type Output = T::Output;

    fn send_output(&self, output: Self::Output) -> Result<(), Self::Output> {
        self.output(output)
    }
}

impl<T> SendableOutput for FactorySender<T>
where
    T: FactoryComponent,
{
    type Output = T::Output;

    fn send_output(&self, output: Self::Output) -> Result<(), Self::Output> {
        self.output(output)
    }
}

/// Sends output through a sender or logs an error if sending fails.
///
/// This function works with both `ComponentSender<T>` and `FactorySender<T>`,
/// providing a unified interface for error handling when sending output messages.
///
/// # Arguments
///
/// * `sender` - Either a `ComponentSender<T>` or `FactorySender<T>`
/// * `output` - The output message to send
///
/// # Examples
///
/// With SimpleComponent:
/// ```rust,ignore
/// sender.output(MyOutput::SomeValue).unwrap(); // Old way
/// send_or_log(&sender, MyOutput::SomeValue);   // New way
/// ```
///
/// With FactoryComponent:
/// ```rust,ignore
/// sender.output(MyFactoryOutput::Action).unwrap(); // Old way
/// send_or_log(&sender, MyFactoryOutput::Action);   // New way
/// ```
pub fn send_or_log<S>(sender: &S, output: S::Output)
where
    S: SendableOutput,
{
    if let Err(e) = sender.send_output(output) {
        log::error!("Failed to send output: {:?}", e);
    }
}
