use flume::{Receiver, Sender, unbounded};
use log::error;
use relm4::SimpleComponent;
use relm4::component::{Connector, Controller};

pub struct Channel<T> {
    pub sender: Sender<T>,
    pub receiver: Receiver<T>,
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        Channel {
            sender: sender,
            receiver: receiver,
        }
    }
}

pub fn connect_component<Component>(
    component: Connector<Component>,
    channel: &Channel<Component::Output>,
) -> Controller<Component>
where
    Component: SimpleComponent,
{
    let sender = channel.sender.clone();
    component.connect_receiver(move |_sender, output: Component::Output| {
        match sender.send(output) {
            Ok(_) => (),
            Err(err) => {
                error!("Failed to forward message: {:?}", err);
            }
        }
    })
}
