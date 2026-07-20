use agilang_framework_container::Container;
use std::path::PathBuf;

pub trait ServiceProvider {
    fn register(&self, container: &mut Container);
    fn boot(&self);
}

pub struct Application {
    pub container: Container,
    pub providers: Vec<Box<dyn ServiceProvider>>,
    pub base_path: PathBuf,
}

impl Application {
    pub fn create(base_path: PathBuf) -> Self {
        Application {
            container: Container::new(),
            providers: vec![],
            base_path,
        }
    }

    pub fn register(&mut self, provider: Box<dyn ServiceProvider>) {
        provider.register(&mut self.container);
        self.providers.push(provider);
    }

    pub fn boot(&self) {
        for p in &self.providers {
            p.boot();
        }
    }
}
