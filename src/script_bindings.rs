use std::{
    fmt,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
};

use crate::{
    buffer::TextRef,
    config::ParseConfigError,
    editor::{EditorLoop, StatusMessageKind},
    keymap::ParseKeyMapError,
    mode::Mode,
    pattern::Pattern,
    script::{ScriptContext, ScriptEngine, ScriptError, ScriptResult, ScriptStr},
    theme::ParseThemeError,
};

pub struct QuitError;
impl fmt::Display for QuitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("could not quit now")
    }
}

pub fn bind_all<'a>(scripts: &'a mut ScriptEngine) -> ScriptResult<()> {
    macro_rules! register_all {
        ($($func:ident,)*) => {
            $(scripts.register_ctx_function(stringify!($func), bindings::$func)?;)*
        }
    }

    register_all! {
        client_index,
        quit, quit_all, open, close, close_all, save, save_all,
        selection, replace, print, pipe, spawn,
        config, syntax_extension, syntax_rule, theme,
        mapn, maps, mapi,
    };

    Ok(())
}

mod bindings {
    use super::*;

    pub fn client_index(ctx: &mut ScriptContext, _: ()) -> ScriptResult<usize> {
        Ok(ctx.target_client.into_index())
    }

    pub fn quit(ctx: &mut ScriptContext, _: ()) -> ScriptResult<()> {
        *ctx.editor_loop = EditorLoop::Quit;
        Err(ScriptError::from(QuitError))
    }

    pub fn quit_all(ctx: &mut ScriptContext, _: ()) -> ScriptResult<()> {
        *ctx.editor_loop = EditorLoop::QuitAll;
        Err(ScriptError::from(QuitError))
    }

    pub fn open(ctx: &mut ScriptContext, path: ScriptStr) -> ScriptResult<()> {
        let path = Path::new(path.to_str()?);
        let buffer_view_handle = ctx
            .buffer_views
            .new_buffer_from_file(ctx.buffers, &ctx.config.syntaxes, ctx.target_client, path)
            .map_err(ScriptError::from)?;
        ctx.set_current_buffer_view_handle(Some(buffer_view_handle));
        Ok(())
    }

    pub fn close(ctx: &mut ScriptContext, _: ()) -> ScriptResult<()> {
        if let Some(handle) = ctx
            .current_buffer_view_handle()
            .and_then(|h| ctx.buffer_views.get(h))
            .map(|v| v.buffer_handle)
        {
            ctx.buffer_views
                .remove_where(ctx.buffers, |view| view.buffer_handle == handle);
        }

        ctx.set_current_buffer_view_handle(None);
        Ok(())
    }

    pub fn close_all(ctx: &mut ScriptContext, _: ()) -> ScriptResult<()> {
        ctx.buffer_views.remove_where(ctx.buffers, |_| true);
        for c in ctx.clients.client_refs() {
            c.client.current_buffer_view_handle = None;
        }
        Ok(())
    }

    pub fn save(ctx: &mut ScriptContext, path: Option<ScriptStr>) -> ScriptResult<()> {
        let buffer_handle = match ctx
            .current_buffer_view_handle()
            .and_then(|h| ctx.buffer_views.get(h))
            .map(|v| v.buffer_handle)
        {
            Some(handle) => handle,
            None => return Err(ScriptError::from("no buffer opened")),
        };

        let buffer = match ctx.buffers.get_mut(buffer_handle) {
            Some(buffer) => buffer,
            None => return Err(ScriptError::from("no buffer opened")),
        };

        match path {
            Some(path) => {
                let path = Path::new(path.to_str()?);
                buffer.set_path(&ctx.config.syntaxes, path);
                buffer.save_to_file().map_err(ScriptError::from)?;
                Ok(())
            }
            None => buffer.save_to_file().map_err(ScriptError::from),
        }
    }

    pub fn save_all(ctx: &mut ScriptContext, _: ()) -> ScriptResult<()> {
        for buffer in ctx.buffers.iter() {
            buffer.save_to_file().map_err(ScriptError::from)?;
        }
        Ok(())
    }

    pub fn selection(ctx: &mut ScriptContext, _: ()) -> ScriptResult<String> {
        let mut selection = String::new();
        ctx.current_buffer_view_handle()
            .and_then(|h| ctx.buffer_views.get(h))
            .map(|v| v.get_selection_text(ctx.buffers, &mut selection));
        Ok(selection)
    }

    pub fn replace(ctx: &mut ScriptContext, text: ScriptStr) -> ScriptResult<()> {
        if let Some(handle) = ctx.current_buffer_view_handle() {
            let text = TextRef::Str(text.to_str()?);
            ctx.buffer_views
                .delete_in_selection(ctx.buffers, &ctx.config.syntaxes, handle);
            ctx.buffer_views
                .insert_text(ctx.buffers, &ctx.config.syntaxes, handle, text);
        }
        Ok(())
    }

    pub fn print(ctx: &mut ScriptContext, message: ScriptStr) -> ScriptResult<()> {
        let message = message.to_str()?;
        *ctx.status_message_kind = StatusMessageKind::Info;
        ctx.status_message.clear();
        ctx.status_message.push_str(message);
        Ok(())
    }

    pub fn pipe(
        _ctx: &mut ScriptContext,
        (name, args, input): (ScriptStr, Vec<ScriptStr>, Option<ScriptStr>),
    ) -> ScriptResult<String> {
        let child = run_process(name, args, input, Stdio::piped())?;
        let child_output = child.wait_with_output().map_err(ScriptError::from)?;
        if child_output.status.success() {
            let child_output = String::from_utf8_lossy(&child_output.stdout[..]);
            Ok(child_output.into_owned())
        } else {
            let child_output = String::from_utf8_lossy(&child_output.stdout[..]);
            Err(ScriptError::from(child_output.into_owned()))
        }
    }

    pub fn spawn(
        _ctx: &mut ScriptContext,
        (name, args, input): (ScriptStr, Vec<ScriptStr>, Option<ScriptStr>),
    ) -> ScriptResult<()> {
        run_process(name, args, input, Stdio::null())?;
        Ok(())
    }

    fn run_process(
        name: ScriptStr,
        args: Vec<ScriptStr>,
        input: Option<ScriptStr>,
        output: Stdio,
    ) -> ScriptResult<Child> {
        let mut command = Command::new(name.to_str()?);
        command.stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        command.stdout(output);
        command.stderr(Stdio::piped());
        for arg in args {
            command.arg(arg.to_str()?);
        }

        let mut child = command.spawn().map_err(ScriptError::from)?;
        if let Some(stdin) = child.stdin.as_mut() {
            let bytes = match input.as_ref() {
                Some(input) => input.as_bytes(),
                None => &[],
            };
            let _ = stdin.write_all(bytes);
        }
        child.stdin = None;
        Ok(child)
    }

    pub fn config(
        ctx: &mut ScriptContext,
        (name, value): (ScriptStr, ScriptStr),
    ) -> ScriptResult<()> {
        let name = name.to_str()?;
        let value = value.to_str()?;

        if let Err(e) = ctx.config.values.parse_and_set(name, value) {
            let message = match e {
                ParseConfigError::ConfigNotFound => helper::parsing_error(e, name, 0),
                ParseConfigError::ParseError(e) => helper::parsing_error(e, value, 0),
            };
            return Err(ScriptError::from(message));
        }
        Ok(())
    }

    pub fn syntax_extension(
        ctx: &mut ScriptContext,
        (main_extension, other_extension): (ScriptStr, ScriptStr),
    ) -> ScriptResult<()> {
        let main_extension = main_extension.to_str()?;
        let other_extension = other_extension.to_str()?;
        ctx.config
            .syntaxes
            .get_by_extension(main_extension)
            .add_extension(other_extension.into());
        Ok(())
    }

    pub fn syntax_rule(
        ctx: &mut ScriptContext,
        (main_extension, token_kind, pattern): (ScriptStr, ScriptStr, ScriptStr),
    ) -> ScriptResult<()> {
        let main_extension = main_extension.to_str()?;
        let token_kind = token_kind.to_str()?;
        let pattern = pattern.to_str()?;

        let token_kind = token_kind.parse().map_err(ScriptError::from)?;
        let pattern = Pattern::new(pattern).map_err(|e| {
            let message = helper::parsing_error(e, pattern, 0);
            ScriptError::from(message)
        })?;

        ctx.config
            .syntaxes
            .get_by_extension(main_extension)
            .add_rule(token_kind, pattern);
        Ok(())
    }

    pub fn theme(
        ctx: &mut ScriptContext,
        (name, color): (ScriptStr, ScriptStr),
    ) -> ScriptResult<()> {
        let name = name.to_str()?;
        let color = color.to_str()?;

        if let Err(e) = ctx.config.theme.parse_and_set(name, color) {
            let context = format!("{} {}", name, color);
            let error_index = match e {
                ParseThemeError::ColorNotFound => 0,
                _ => context.len(),
            };
            let message = helper::parsing_error(e, &context[..], error_index);
            return Err(ScriptError::from(message));
        }

        Ok(())
    }

    pub fn mapn(ctx: &mut ScriptContext, (from, to): (ScriptStr, ScriptStr)) -> ScriptResult<()> {
        map_mode(ctx, Mode::Normal, from, to)
    }

    pub fn maps(ctx: &mut ScriptContext, (from, to): (ScriptStr, ScriptStr)) -> ScriptResult<()> {
        map_mode(ctx, Mode::Select, from, to)
    }

    pub fn mapi(ctx: &mut ScriptContext, (from, to): (ScriptStr, ScriptStr)) -> ScriptResult<()> {
        map_mode(ctx, Mode::Insert, from, to)
    }

    fn map_mode(
        ctx: &mut ScriptContext,
        mode: Mode,
        from: ScriptStr,
        to: ScriptStr,
    ) -> ScriptResult<()> {
        let from = from.to_str()?;
        let to = to.to_str()?;

        match ctx.keymaps.parse_map(mode.discriminant(), from, to) {
            Ok(()) => Ok(()),
            Err(ParseKeyMapError::From(e)) => {
                let message = helper::parsing_error(e.error, from, e.index);
                Err(ScriptError::from(message))
            }
            Err(ParseKeyMapError::To(e)) => {
                let message = helper::parsing_error(e.error, to, e.index);
                Err(ScriptError::from(message))
            }
        }
    }
}

mod helper {
    use super::*;

    pub fn parsing_error<T>(message: T, text: &str, error_index: usize) -> String
    where
        T: fmt::Display,
    {
        let (before, after) = text.split_at(error_index);
        match (before.len(), after.len()) {
            (0, 0) => format!("{} at ''", message),
            (_, 0) => format!("{} at '{}' <- here", message, before),
            (0, _) => format!("{} at here -> '{}'", message, after),
            (_, _) => format!("{} at '{}' <- here '{}'", message, before, after),
        }
    }
}