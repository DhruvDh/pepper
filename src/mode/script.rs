use crate::{
    editor::{EditorLoop, KeysIterator},
    mode::{poll_input, FromMode, InputPollResult, ModeContext, ModeOperation},
    script::ScriptContext,
};

pub fn on_enter(ctx: &mut ModeContext) {
    ctx.input.clear();
}

pub fn on_event(
    mut ctx: &mut ModeContext,
    keys: &mut KeysIterator,
    from_mode: &FromMode,
) -> ModeOperation {
    match poll_input(&mut ctx, keys) {
        InputPollResult::Pending => ModeOperation::None,
        InputPollResult::Canceled => ModeOperation::EnterMode(from_mode.as_mode()),
        InputPollResult::Submited => {
            let mut editor_loop = EditorLoop::Continue;
            let context = ScriptContext {
                editor_loop: &mut editor_loop,
                target_client: ctx.target_client,

                config: ctx.config,
                keymaps: ctx.keymaps,
                buffers: ctx.buffers,
                buffer_views: ctx.buffer_views,

                clients: ctx.clients,

                status_message_kind: ctx.status_message_kind,
                status_message: ctx.status_message,
            };

            let result = ctx.scripts.eval(context, &ctx.input[..]);
            ctx.input.clear();
            match result {
                Ok(()) => ModeOperation::EnterMode(from_mode.as_mode()),
                Err(e) => match editor_loop {
                    EditorLoop::Quit => ModeOperation::Quit,
                    EditorLoop::QuitAll => ModeOperation::QuitAll,
                    EditorLoop::Continue => {
                        use std::error::Error;
                        let mut message = e.to_string();
                        let mut error = e.source();
                        while let Some(e) = error {
                            message.push('\n');
                            let s = e.to_string();
                            message.push_str(&s);
                            error = e.source();
                        }
                        ModeOperation::EnterMode(from_mode.as_mode())
                    }
                },
            }
        }
    }
}