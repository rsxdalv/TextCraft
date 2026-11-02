//! When each command is enabled.
//!
//! Enablement is state, not a property of the command alone, so the registry
//! declares a rule per command and the shell supplies the state it knows. This
//! keeps the "why is this greyed out?" logic testable without a window.

/// The condition under which a command may be invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Availability {
    /// Always enabled.
    Always,
    /// Never enabled; used for placeholder entries that only hold a place.
    Never,
    /// Enabled when a document exists.
    Edited,
    /// Enabled only when the current document has unsaved changes.
    Dirty,
    /// Enabled only when there is a selection.
    Selection,
    /// Enabled only when the document is a Markdown document.
    Markdown,
    /// Enabled only when the AI service is reachable and configured.
    Service,
    /// Enabled only when more than one tab is open.
    MultipleTabs,
    /// Enabled only when the active document has a path on disk.
    HasPath,
    /// Enabled only when there are recent files in history.
    HasRecent,
    /// Enabled only when there is at least one closed tab to restore.
    HasClosedTabs,
    /// Enabled when there are multiple tabs or any tab has unsaved changes.
    SaveAll,
    /// Enabled only when the active document has an edit to undo.
    CanUndo,
    /// Enabled only when the active document has an edit to redo.
    CanRedo,
}

/// The shell state that availability rules are evaluated against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvailabilityContext {
    /// Whether at least one document is open.
    pub document_present: bool,
    /// Whether the current document has unsaved changes.
    pub dirty: bool,
    /// Whether the current selection is non-empty.
    pub has_selection: bool,
    /// Whether the current document is a Markdown document.
    pub markdown_enabled: bool,
    /// Whether the AI service is configured and reachable.
    pub service_available: bool,
    /// How many tabs are open on the current window.
    pub tab_count: usize,
    /// Whether the status bar is currently shown (the `View ▸ Status bar` check).
    pub status_bar_visible: bool,
    /// Whether the workspace sidebar is currently shown (`View ▸ Workspace sidebar`).
    pub sidebar_visible: bool,
    /// Whether word wrap is currently on (the `View ▸ Word wrap` check).
    pub word_wrap: bool,
    /// Whether a Markdown document is shown formatted rather than as source.
    pub formatted_view: bool,
    /// Whether the active document is backed by a file on disk.
    pub has_path: bool,
    /// Whether any tab in the window has unsaved changes.
    pub any_dirty: bool,
    /// Whether there are recent files in the session.
    pub has_recent: bool,
    /// Whether there are closed tabs in the window undo stack.
    pub has_closed_tabs: bool,
    /// Whether the active document has an edit to undo.
    pub can_undo: bool,
    /// Whether the active document has an edit to redo.
    pub can_redo: bool,
}

impl AvailabilityContext {
    /// The state at first launch: one empty, clean, unselected plain document.
    pub const fn initial() -> Self {
        Self {
            document_present: true,
            dirty: false,
            has_selection: false,
            markdown_enabled: false,
            service_available: false,
            tab_count: 1,
            status_bar_visible: true,
            sidebar_visible: true,
            word_wrap: true,
            formatted_view: false,
            has_path: false,
            any_dirty: false,
            has_recent: false,
            has_closed_tabs: false,
            can_undo: false,
            can_redo: false,
        }
    }
}

impl Default for AvailabilityContext {
    fn default() -> Self {
        Self::initial()
    }
}

impl Availability {
    /// Whether a command with this rule may be invoked in `context`.
    pub const fn allows(self, context: AvailabilityContext) -> bool {
        match self {
            Availability::Always => true,
            Availability::Never => false,
            Availability::Edited => context.document_present,
            Availability::Dirty => context.document_present && context.dirty,
            Availability::Selection => context.document_present && context.has_selection,
            Availability::Markdown => context.document_present && context.markdown_enabled,
            Availability::Service => context.service_available,
            Availability::MultipleTabs => context.tab_count > 1,
            Availability::HasPath => context.document_present && context.has_path,
            Availability::HasRecent => context.has_recent,
            Availability::HasClosedTabs => context.has_closed_tabs,
            Availability::SaveAll => context.tab_count > 1 || context.any_dirty,
            Availability::CanUndo => context.document_present && context.can_undo,
            Availability::CanRedo => context.document_present && context.can_redo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_context_disables_document_dependent_commands() {
        let ctx = AvailabilityContext::initial();
        assert!(Availability::Always.allows(ctx));
        assert!(!Availability::Never.allows(ctx));
        assert!(Availability::Edited.allows(ctx));
        assert!(!Availability::Dirty.allows(ctx));
        assert!(!Availability::Selection.allows(ctx));
        assert!(!Availability::Markdown.allows(ctx));
        assert!(!Availability::Service.allows(ctx));
        assert!(!Availability::MultipleTabs.allows(ctx));
        assert!(!Availability::HasPath.allows(ctx));
        assert!(!Availability::HasRecent.allows(ctx));
        assert!(!Availability::HasClosedTabs.allows(ctx));
        assert!(!Availability::SaveAll.allows(ctx));
    }

    #[test]
    fn tab_and_selection_rules_track_state() {
        let ctx = AvailabilityContext {
            has_selection: true,
            tab_count: 3,
            has_path: true,
            any_dirty: true,
            has_recent: true,
            has_closed_tabs: true,
            ..AvailabilityContext::initial()
        };
        assert!(Availability::Selection.allows(ctx));
        assert!(Availability::MultipleTabs.allows(ctx));
        assert!(Availability::HasPath.allows(ctx));
        assert!(Availability::SaveAll.allows(ctx));
        assert!(Availability::HasRecent.allows(ctx));
        assert!(Availability::HasClosedTabs.allows(ctx));
    }

    #[test]
    fn save_all_allows_single_dirty_tab() {
        let ctx = AvailabilityContext {
            tab_count: 1,
            dirty: true,
            any_dirty: true,
            ..AvailabilityContext::initial()
        };
        assert!(Availability::SaveAll.allows(ctx));
    }

    #[test]
    fn rules_require_a_document() {
        let ctx = AvailabilityContext {
            document_present: false,
            has_selection: true,
            dirty: true,
            has_path: true,
            ..AvailabilityContext::initial()
        };
        assert!(!Availability::Edited.allows(ctx));
        assert!(!Availability::Selection.allows(ctx));
        assert!(!Availability::Dirty.allows(ctx));
        assert!(!Availability::HasPath.allows(ctx));
        assert!(Availability::Always.allows(ctx));
    }
}
