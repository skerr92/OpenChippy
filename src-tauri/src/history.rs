use crate::model::Project;

#[derive(Default)]
pub struct ProjectHistory {
    current: Project,
    undo: Vec<Project>,
    redo: Vec<Project>,
}

impl ProjectHistory {
    pub fn current(&self) -> Project {
        self.current.clone()
    }

    pub fn reset(&mut self, project: Project) {
        self.current = project;
        self.undo.clear();
        self.redo.clear();
    }

    pub fn update(&mut self, action: impl FnOnce(&mut Project)) {
        self.undo.push(self.current.clone());
        action(&mut self.current);
        self.redo.clear();
    }

    pub fn try_update<E>(
        &mut self,
        action: impl FnOnce(&mut Project) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut next = self.current.clone();
        action(&mut next)?;
        self.undo.push(std::mem::replace(&mut self.current, next));
        self.redo.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> Option<Project> {
        let previous = self.undo.pop()?;
        self.redo
            .push(std::mem::replace(&mut self.current, previous));
        Some(self.current())
    }

    pub fn redo(&mut self) -> Option<Project> {
        let next = self.redo.pop()?;
        self.undo.push(std::mem::replace(&mut self.current, next));
        Some(self.current())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectHistory;

    #[test]
    fn update_can_be_undone_and_redone() {
        let mut history = ProjectHistory::default();
        history.update(|project| project.add_placeholder());
        assert_eq!(history.current().components.len(), 1);
        assert_eq!(history.undo().unwrap().components.len(), 0);
        assert_eq!(history.redo().unwrap().components.len(), 1);
    }

    #[test]
    fn a_new_change_clears_redo_history() {
        let mut history = ProjectHistory::default();
        history.update(|project| project.add_placeholder());
        history.undo();
        history.update(|project| project.add_placeholder());
        assert!(history.redo().is_none());
    }

    #[test]
    fn reset_starts_a_new_history() {
        let mut history = ProjectHistory::default();
        history.update(|project| project.add_placeholder());
        history.reset(Default::default());
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
