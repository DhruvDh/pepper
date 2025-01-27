use std::{collections::VecDeque, fmt, io};

use crate::{
    buffer::{Buffer, BufferHandle},
    buffer_view::BufferViewHandle,
    client::{ClientHandle, ClientManager},
    config::ParseConfigError,
    editor::{Editor, EditorControlFlow},
    editor_utils::MessageKind,
    glob::InvalidGlobError,
    keymap::ParseKeyMapError,
    pattern::PatternError,
    platform::Platform,
};

mod builtin;

pub const HISTORY_CAPACITY: usize = 10;

pub enum CommandError {
    NoSuchCommand,
    TooManyArguments,
    TooFewArguments,
    NoTargetClient,
    NoBufferOpened,
    UnsavedChanges,
    IoError(io::Error),
    ConfigError(ParseConfigError),
    NoSuchColor,
    InvalidColorValue,
    KeyMapError(ParseKeyMapError),
    PatternError(PatternError),
    InvalidGlob,
    RecursiveSyntaxBegin,
    NoCurrentSyntax,
    LspServerNotRunning,
    LspServerNotLogging,
}
impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NoSuchCommand => f.write_str("no such command"),
            Self::TooManyArguments => f.write_str("too many arguments"),
            Self::TooFewArguments => f.write_str("too few arguments"),
            Self::NoTargetClient => f.write_str("no target client"),
            Self::NoBufferOpened => f.write_str("no buffer opened"),
            Self::UnsavedChanges => f.write_str("unsaved changes"),
            Self::IoError(error) => write!(f, "{}", error),
            Self::ConfigError(error) => write!(f, "{}", error),
            Self::NoSuchColor => f.write_str("no such color"),
            Self::InvalidColorValue => f.write_str("invalid color value"),
            Self::KeyMapError(error) => write!(f, "{}", error),
            Self::PatternError(error) => write!(f, "pattern error: {}", error),
            Self::InvalidGlob => write!(f, "{}", InvalidGlobError),
            Self::RecursiveSyntaxBegin => f.write_str("recursive syntax definition"),
            Self::NoCurrentSyntax => {
                f.write_str("no current syntax. did you forget a `syntax-begin`?")
            }
            Self::LspServerNotRunning => f.write_str("no lsp server running"),
            Self::LspServerNotLogging => f.write_str("lsp server is not logging"),
        }
    }
}

type CommandFn = fn(&mut CommandContext) -> Result<EditorControlFlow, CommandError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionSource {
    Commands,
    Buffers,
    Files,
    Custom(&'static [&'static str]),
}

pub struct CommandArgs<'command>(CommandTokenizer<'command>);
impl<'command> CommandArgs<'command> {
    pub fn try_next(&mut self) -> Option<&'command str> {
        self.0.next()
    }

    pub fn next(&mut self) -> Result<&'command str, CommandError> {
        match self.try_next() {
            Some(value) => Ok(value),
            None => Err(CommandError::TooFewArguments),
        }
    }

    pub fn assert_empty(&mut self) -> Result<(), CommandError> {
        match self.try_next() {
            Some(_) => Err(CommandError::TooManyArguments),
            None => Ok(()),
        }
    }
}

pub struct CommandContext<'state, 'command> {
    pub editor: &'state mut Editor,
    pub platform: &'state mut Platform,
    pub clients: &'state mut ClientManager,
    pub client_handle: Option<ClientHandle>,

    pub args: CommandArgs<'command>,
    pub bang: bool,
}
impl<'state, 'command> CommandContext<'state, 'command> {
    pub fn client_handle(&self) -> Result<ClientHandle, CommandError> {
        match self.client_handle {
            Some(handle) => Ok(handle),
            None => Err(CommandError::NoTargetClient),
        }
    }

    pub fn current_buffer_view_handle(&self) -> Result<BufferViewHandle, CommandError> {
        let client_handle = self.client_handle()?;
        match self.clients.get(client_handle).buffer_view_handle() {
            Some(handle) => Ok(handle),
            None => Err(CommandError::NoBufferOpened),
        }
    }

    pub fn current_buffer_handle(&self) -> Result<BufferHandle, CommandError> {
        let buffer_view_handle = self.current_buffer_view_handle()?;
        let buffer_handle = self
            .editor
            .buffer_views
            .get(buffer_view_handle)
            .buffer_handle;
        Ok(buffer_handle)
    }

    pub fn assert_can_discard_all_buffers(&self) -> Result<(), CommandError> {
        if self.bang || !self.editor.buffers.iter().any(Buffer::needs_save) {
            Ok(())
        } else {
            Err(CommandError::UnsavedChanges)
        }
    }

    pub fn assert_can_discard_buffer(&self, handle: BufferHandle) -> Result<(), CommandError> {
        if self.bang || !self.editor.buffers.get(handle).needs_save() {
            Ok(())
        } else {
            Err(CommandError::UnsavedChanges)
        }
    }
}

#[derive(Clone)]
pub struct CommandTokenizer<'a>(pub &'a str);
impl<'a> Iterator for CommandTokenizer<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        fn next_literal_end(s: &str) -> usize {
            match s.find(&[' ', '\t'][..]) {
                Some(i) => i,
                None => s.len(),
            }
        }

        fn parse_balanced_token(s: &str) -> Option<(&str, &str)> {
            let mut chars = s.chars();
            let mut depth = 0;
            loop {
                match chars.next()? {
                    '=' => depth += 1,
                    '[' => break,
                    _ => return None,
                }
            }
            let start = chars.as_str().as_ptr() as usize;
            let mut end = start;
            let mut ending = false;
            let mut matched = 0;
            loop {
                match chars.next()? {
                    ']' => {
                        if ending && matched == depth {
                            break;
                        }

                        ending = true;
                        matched = 0;
                        end = chars.as_str().as_ptr() as usize - 1;
                    }
                    '=' => matched += 1,
                    _ => (),
                }
            }
            let rest = chars.as_str();
            let base = s.as_ptr() as usize;
            let start = start - base;
            let end = end - base;
            let token = &s[start..end];

            Some((token, rest))
        }

        self.0 = self.0.trim_start_matches(&[' ', '\t'][..]);

        match self.0.chars().next()? {
            delim @ ('"' | '\'') => {
                let rest = &self.0[1..];
                match rest.find(delim) {
                    Some(i) => {
                        let token = &rest[..i];
                        self.0 = &rest[i + 1..];
                        Some(token)
                    }
                    None => {
                        let end = next_literal_end(rest);
                        let (token, rest) = self.0.split_at(end + 1);
                        self.0 = rest;
                        Some(token)
                    }
                }
            }
            c => {
                if c == '[' {
                    if let Some((token, rest)) = parse_balanced_token(&self.0[1..]) {
                        self.0 = rest;
                        return Some(token);
                    }
                }

                let end = next_literal_end(self.0);
                let (token, rest) = self.0.split_at(end);
                self.0 = rest;
                Some(token)
            }
        }
    }
}

pub struct BuiltinCommand {
    pub name: &'static str,
    pub completions: &'static [CompletionSource],
    pub func: CommandFn,
}

struct Alias {
    start: u32,
    from_len: u16,
    to_len: u16,
}
impl Alias {
    pub fn from<'a>(&self, texts: &'a str) -> &'a str {
        let end = self.start as usize + self.from_len as usize;
        &texts[self.start as usize..end]
    }

    pub fn to<'a>(&self, texts: &'a str) -> &'a str {
        let start = self.start as usize + self.from_len as usize;
        let end = start + self.to_len as usize;
        &texts[start..end]
    }
}

#[derive(Default)]
pub struct AliasCollection {
    texts: String,
    aliases: Vec<Alias>,
}
impl AliasCollection {
    pub fn add(&mut self, from: &str, to: &str) {
        if from.len() > u16::MAX as _ || to.len() > u16::MAX as _ {
            return;
        }

        for (i, alias) in self.aliases.iter().enumerate() {
            if from == alias.from(&self.texts) {
                let alias_start = alias.start as usize;
                let alias_len = alias.from_len as u32 + alias.to_len as u32;
                self.aliases.remove(i);
                for alias in &mut self.aliases[i..] {
                    alias.start -= alias_len;
                }
                self.texts
                    .drain(alias_start..alias_start + alias_len as usize);
                break;
            }
        }

        let start = self.texts.len() as _;
        self.texts.push_str(from);
        self.texts.push_str(to);

        self.aliases.push(Alias {
            start,
            from_len: from.len() as _,
            to_len: to.len() as _,
        });
    }

    pub fn find(&self, from: &str) -> Option<&str> {
        for alias in &self.aliases {
            if from == alias.from(&self.texts) {
                return Some(alias.to(&self.texts));
            }
        }

        None
    }
}

pub struct CommandManager {
    builtin_commands: &'static [BuiltinCommand],
    history: VecDeque<String>,
    pub aliases: AliasCollection,
}

impl CommandManager {
    pub fn new() -> Self {
        Self {
            builtin_commands: builtin::COMMANDS,
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
            aliases: AliasCollection::default(),
        }
    }

    pub fn find_command(&self, name: &str) -> Option<&BuiltinCommand> {
        self.builtin_commands.iter().find(|c| c.name == name)
    }

    pub fn builtin_commands(&self) -> &[BuiltinCommand] {
        &self.builtin_commands
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn history_entry(&self, index: usize) -> &str {
        match self.history.get(index) {
            Some(e) => &e[..],
            None => "",
        }
    }

    pub fn add_to_history(&mut self, entry: &str) {
        if entry.is_empty() || entry.starts_with(|c: char| c.is_ascii_whitespace()) {
            return;
        }
        if let Some(back) = self.history.back() {
            if back == entry {
                return;
            }
        }

        let mut s = if self.history.len() == self.history.capacity() {
            self.history.pop_front().unwrap()
        } else {
            String::new()
        };

        s.clear();
        s.push_str(entry);
        self.history.push_back(s);
    }

    pub fn eval(
        editor: &mut Editor,
        platform: &mut Platform,
        clients: &mut ClientManager,
        client_handle: Option<ClientHandle>,
        command: &mut String,
    ) -> EditorControlFlow {
        match Self::try_eval(editor, platform, clients, client_handle, command) {
            Ok(flow) => flow,
            Err(error) => {
                editor
                    .status_bar
                    .write(MessageKind::Error)
                    .fmt(format_args!("{}", error));
                EditorControlFlow::Continue
            }
        }
    }

    pub fn try_eval(
        editor: &mut Editor,
        platform: &mut Platform,
        clients: &mut ClientManager,
        client_handle: Option<ClientHandle>,
        command: &mut String,
    ) -> Result<EditorControlFlow, CommandError> {
        if let Some(alias) = CommandTokenizer(command).next() {
            let alias = alias.trim_end_matches('!');
            if let Some(aliased) = editor.commands.aliases.find(alias) {
                let start = alias.as_ptr() as usize - command.as_ptr() as usize;
                let end = start + alias.len();
                command.replace_range(start..end, aliased);
            }
        }

        Self::do_eval(editor, platform, clients, client_handle, command)
    }

    fn do_eval(
        editor: &mut Editor,
        platform: &mut Platform,
        clients: &mut ClientManager,
        client_handle: Option<ClientHandle>,
        command: &str,
    ) -> Result<EditorControlFlow, CommandError> {
        let mut tokenizer = CommandTokenizer(command);
        let command = match tokenizer.next() {
            Some(command) => command,
            None => return Err(CommandError::NoSuchCommand),
        };
        let (command, bang) = match command.strip_suffix('!') {
            Some(command) => (command, true),
            None => (command, false),
        };
        let command_func = match editor.commands.find_command(command) {
            Some(command) => command.func,
            None => return Err(CommandError::NoSuchCommand),
        };

        let mut ctx = CommandContext {
            editor,
            platform,
            clients,
            client_handle,
            args: CommandArgs(tokenizer),
            bang,
        };
        (command_func)(&mut ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_tokens() {
        let mut tokens = CommandTokenizer("cmd arg");
        assert_eq!(Some("cmd"), tokens.next());
        assert_eq!(Some("arg"), tokens.next());
        assert_eq!(None, tokens.next());

        let mut tokens = CommandTokenizer("cmd 'arg0 \"arg1 ");
        assert_eq!(Some("cmd"), tokens.next());
        assert_eq!(Some("'arg0"), tokens.next());
        assert_eq!(Some("\"arg1"), tokens.next());
        assert_eq!(None, tokens.next());

        let mut tokens = CommandTokenizer("cmd arg0'arg1 ");
        assert_eq!(Some("cmd"), tokens.next());
        assert_eq!(Some("arg0'arg1"), tokens.next());
        assert_eq!(None, tokens.next());

        let mut tokens = CommandTokenizer("cmd arg0\"arg1 ");
        assert_eq!(Some("cmd"), tokens.next());
        assert_eq!(Some("arg0\"arg1"), tokens.next());
        assert_eq!(None, tokens.next());

        let mut tokens = CommandTokenizer("cmd 'arg\"0' \"arg'1\"");
        assert_eq!(Some("cmd"), tokens.next());
        assert_eq!(Some("arg\"0"), tokens.next());
        assert_eq!(Some("arg'1"), tokens.next());
        assert_eq!(None, tokens.next());

        let mut tokens = CommandTokenizer("cmd [[arg]]");
        assert_eq!(Some("cmd"), tokens.next());
        assert_eq!(Some("arg"), tokens.next());
        assert_eq!(None, tokens.next());

        let mut tokens = CommandTokenizer("cmd [==[arg]]=]]==]");
        assert_eq!(Some("cmd"), tokens.next());
        assert_eq!(Some("arg]]=]"), tokens.next());
        assert_eq!(None, tokens.next());
    }
}
