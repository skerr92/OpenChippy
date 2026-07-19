use crate::model::Project;

/// Stable internal boundary for future importers, exporters, and analysis tools.
/// Dynamic plugin loading is deliberately deferred until its trust model is defined.
pub trait OpenChippyPlugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn on_project_loaded(&self, _project: &Project) {}
}

#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn OpenChippyPlugin>>,
}

impl PluginRegistry {
    pub fn register(&mut self, plugin: impl OpenChippyPlugin + 'static) {
        self.plugins.push(Box::new(plugin));
    }

    pub fn ids(&self) -> Vec<&'static str> {
        self.plugins.iter().map(|plugin| plugin.id()).collect()
    }
}
