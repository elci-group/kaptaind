use super::adapter::LanguageAdapter;
use crate::config::loader::PluginsConfig;
use std::path::Path;

pub struct AdapterRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    pub fn register(&mut self, adapter: Box<dyn LanguageAdapter>) {
        self.adapters.push(adapter);
    }

    pub fn resolve(&self, path: &Path) -> Option<&dyn LanguageAdapter> {
        let paths = vec![path.to_path_buf()];
        self.adapters
            .iter()
            .find(|a| !a.detect_files(&paths).is_empty())
            .map(|a| a.as_ref())
    }

    pub fn default_registry() -> Self {
        let mut registry = Self::new();
        super::adapters::register_builtin_adapters(&mut registry);
        registry
    }

    /// Build the default registry and append any plugin adapters from config.
    pub fn default_registry_with_plugins(plugins: &PluginsConfig) -> Self {
        let mut registry = Self::default_registry();
        for adapter_config in &plugins.adapters {
            registry.register(Box::new(super::plugin::PluginAdapter::new(
                adapter_config.clone(),
            )));
        }
        registry
    }
}
