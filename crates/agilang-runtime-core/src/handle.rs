use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::{AgilangError, AgilangValue, RuntimeResult};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HandleId(pub u64);

impl HandleId {
    pub const INVALID: Self = Self(0);
    fn from_parts(index: u32, generation: u32) -> Self {
        Self(((generation as u64) << 32) | index as u64)
    }
    fn index(self) -> usize {
        (self.0 as u32) as usize
    }
    fn generation(self) -> u32 {
        (self.0 >> 32) as u32
    }
}

#[derive(Debug)]
pub enum RuntimeObject {
    Value(AgilangValue),
}

#[derive(Debug)]
struct Slot {
    generation: u32,
    object: Option<Arc<RuntimeObject>>,
}

#[derive(Debug, Default)]
struct RegistryState {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: usize,
}

#[derive(Debug)]
pub struct HandleRegistry {
    state: RwLock<RegistryState>,
}

impl Default for HandleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HandleRegistry {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(RegistryState {
                slots: vec![Slot {
                    generation: 1,
                    object: None,
                }],
                free: Vec::new(),
                live: 0,
            }),
        }
    }

    pub fn insert(&self, object: RuntimeObject) -> HandleId {
        let mut state = self.state.write();
        let index = state.free.pop().unwrap_or_else(|| {
            let index = state.slots.len() as u32;
            state.slots.push(Slot {
                generation: 1,
                object: None,
            });
            index
        });
        let generation = {
            let slot = &mut state.slots[index as usize];
            slot.object = Some(Arc::new(object));
            slot.generation
        };
        state.live += 1;
        HandleId::from_parts(index, generation)
    }

    pub fn insert_value(&self, value: AgilangValue) -> HandleId {
        self.insert(RuntimeObject::Value(value))
    }

    pub fn get(&self, id: HandleId) -> RuntimeResult<Arc<RuntimeObject>> {
        if id == HandleId::INVALID {
            return Err(AgilangError::invalid_handle(id.0));
        }
        let state = self.state.read();
        let slot = state
            .slots
            .get(id.index())
            .ok_or_else(|| AgilangError::invalid_handle(id.0))?;
        if slot.generation != id.generation() {
            return Err(AgilangError::invalid_handle(id.0));
        }
        slot.object
            .clone()
            .ok_or_else(|| AgilangError::invalid_handle(id.0))
    }

    pub fn get_value(&self, id: HandleId) -> RuntimeResult<AgilangValue> {
        match self.get(id)?.as_ref() {
            RuntimeObject::Value(v) => Ok(v.clone()),
        }
    }

    pub fn clone_handle(&self, id: HandleId) -> RuntimeResult<HandleId> {
        Ok(self.insert_value(self.get_value(id)?))
    }

    pub fn release(&self, id: HandleId) -> RuntimeResult<()> {
        if id == HandleId::INVALID {
            return Err(AgilangError::invalid_handle(id.0));
        }
        let mut state = self.state.write();
        {
            let slot = state
                .slots
                .get_mut(id.index())
                .ok_or_else(|| AgilangError::invalid_handle(id.0))?;
            if slot.generation != id.generation() || slot.object.is_none() {
                return Err(AgilangError::invalid_handle(id.0));
            }
            slot.object = None;
            slot.generation = slot.generation.wrapping_add(1).max(1);
        }
        state.free.push(id.index() as u32);
        state.live -= 1;
        Ok(())
    }

    pub fn contains(&self, id: HandleId) -> bool {
        self.get(id).is_ok()
    }
    pub fn len(&self) -> usize {
        self.state.read().live
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn clear(&self) {
        let mut state = self.state.write();
        for index in 1..state.slots.len() {
            let slot = &mut state.slots[index];
            if slot.object.take().is_some() {
                slot.generation = slot.generation.wrapping_add(1).max(1);
            }
        }
        state.free = (1..state.slots.len() as u32).collect();
        state.live = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_handles_are_rejected() {
        let registry = HandleRegistry::new();
        let first = registry.insert_value(AgilangValue::from("AGILANG"));
        registry.release(first).unwrap();
        let second = registry.insert_value(AgilangValue::from("SIBAQ"));
        assert_ne!(first, second);
        assert!(registry.get(first).is_err());
        assert_eq!(
            registry.get_value(second).unwrap().as_str().unwrap(),
            "SIBAQ"
        );
    }
}
