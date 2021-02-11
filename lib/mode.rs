use crate::{
    client::{ClientManager, ClientHandle},
    editor::{Editor, KeysIterator},
    register::RegisterKey,
};

mod command;
mod insert;
mod normal;
pub mod picker;
pub mod read_line;

pub enum ModeOperation {
    Pending,
    Quit,
    QuitAll,
    ExecuteMacro(RegisterKey),
}

pub trait ModeState {
    fn on_enter(_editor: &mut Editor, _clients: &mut ClientManager, _target: ClientHandle) {}
    fn on_exit(_editor: &mut Editor, _clients: &mut ClientManager, _target: ClientHandle) {}
    fn on_client_keys(
        editor: &mut Editor,
        clients: &mut ClientManager,
        target: ClientHandle,
        keys: &mut KeysIterator,
    ) -> Option<ModeOperation>;
    fn on_editor_events(_editor: &mut Editor, _clients: &mut ClientManager, _target: ClientHandle) {
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModeKind {
    Normal,
    Insert,
    Command,
    ReadLine,
    Picker,
}

impl Default for ModeKind {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Default)]
pub struct Mode {
    kind: ModeKind,
    scratch_buf: String,

    pub normal_state: normal::State,
    pub insert_state: insert::State,
    pub command_state: command::State,
    pub read_line_state: read_line::State,
    pub picker_state: picker::State,
}

impl Mode {
    pub fn kind(&self) -> ModeKind {
        self.kind
    }

    pub fn change_to(
        editor: &mut Editor,
        clients: &mut ClientManager,
        target: ClientHandle,
        next: ModeKind,
    ) {
        match editor.mode.kind {
            ModeKind::Normal => normal::State::on_exit(editor, clients, target),
            ModeKind::Insert => insert::State::on_exit(editor, clients, target),
            ModeKind::Command => command::State::on_exit(editor, clients, target),
            ModeKind::ReadLine => read_line::State::on_exit(editor, clients, target),
            ModeKind::Picker => picker::State::on_exit(editor, clients, target),
        }

        editor.mode.kind = next;

        match editor.mode.kind {
            ModeKind::Normal => normal::State::on_enter(editor, clients, target),
            ModeKind::Insert => insert::State::on_enter(editor, clients, target),
            ModeKind::Command => command::State::on_enter(editor, clients, target),
            ModeKind::ReadLine => read_line::State::on_enter(editor, clients, target),
            ModeKind::Picker => picker::State::on_enter(editor, clients, target),
        }
    }

    pub fn on_client_keys(
        editor: &mut Editor,
        clients: &mut ClientManager,
        target: ClientHandle,
        keys: &mut KeysIterator,
    ) -> Option<ModeOperation> {
        match editor.mode.kind {
            ModeKind::Normal => normal::State::on_client_keys(editor, clients, target, keys),
            ModeKind::Insert => insert::State::on_client_keys(editor, clients, target, keys),
            ModeKind::Command => command::State::on_client_keys(editor, clients, target, keys),
            ModeKind::ReadLine => read_line::State::on_client_keys(editor, clients, target, keys),
            ModeKind::Picker => picker::State::on_client_keys(editor, clients, target, keys),
        }
    }

    // TODO: do we really need modes to handle editor events?
    pub fn on_editor_events(
        editor: &mut Editor,
        clients: &mut ClientManager,
        target: ClientHandle,
    ) {
        match editor.mode.kind {
            ModeKind::Normal => normal::State::on_editor_events(editor, clients, target),
            ModeKind::Insert => insert::State::on_editor_events(editor, clients, target),
            ModeKind::Command => command::State::on_editor_events(editor, clients, target),
            ModeKind::ReadLine => read_line::State::on_editor_events(editor, clients, target),
            ModeKind::Picker => picker::State::on_editor_events(editor, clients, target),
        }
    }
}