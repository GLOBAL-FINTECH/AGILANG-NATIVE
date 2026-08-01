use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Default)]
pub struct Container {
    bindings: HashMap<TypeId, Box<dyn Any>>,
}

impl Container {
    pub fn new() -> Self {
        Container {
            bindings: HashMap::new(),
        }
    }

    pub fn singleton<T: 'static>(&mut self, value: T) {
        self.bindings.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn resolve<T: 'static>(&self) -> Option<&T> {
        self.bindings
            .get(&TypeId::of::<T>())
            .and_then(|any| any.downcast_ref::<T>())
    }
}
