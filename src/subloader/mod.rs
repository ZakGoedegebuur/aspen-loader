use std::sync::mpsc;

use crate::Loadable;

pub trait LoadExp {
    fn load(&mut self);
}

pub struct SubLoader<T: Loadable> {
    item: T,
    sender: mpsc::Sender<<T as Loadable>::Output>
}

impl<T: Loadable> SubLoader<T> {
    pub fn new(item: T) -> (Box<Self>, mpsc::Receiver<<T as Loadable>::Output>) {
        let (sender, reciever) = mpsc::channel::<<T as Loadable>::Output>();

        (Box::new(Self { item, sender }), reciever)
    }
}

impl<T: Loadable> LoadExp for SubLoader<T> {
    fn load(&mut self) {
        let out = self.item.load();
        _ = self.sender.send(out);
    }
}