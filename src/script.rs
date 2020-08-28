#![macro_use]

use std::{fs::File, io::Read, path::Path, sync::Arc};

use mlua::prelude::{
    FromLuaMulti, Lua, LuaError, LuaLightUserData, LuaResult, LuaString, ToLuaMulti,
};

use crate::{
    buffer::BufferCollection,
    buffer_view::{BufferViewCollection, BufferViewHandle},
    config::Config,
    connection::TargetClient,
    editor::ClientTargetMap,
    editor_operation::EditorOperationSerializer,
    keymap::KeyMapCollection,
};

pub type ScriptResult<T> = LuaResult<T>;
pub type ScriptStr<'lua> = LuaString<'lua>;

macro_rules! impl_script_data {
    ($t:ty) => {
        impl mlua::prelude::LuaUserData for $t {}
    };
}

pub struct ScriptContext<'a> {
    pub target_client: TargetClient,
    pub client_target_map: &'a mut ClientTargetMap,
    pub operations: &'a mut EditorOperationSerializer,

    pub config: &'a Config,
    pub keymaps: &'a mut KeyMapCollection,
    pub buffers: &'a mut BufferCollection,
    pub buffer_views: &'a mut BufferViewCollection,
    pub current_buffer_view_handle: &'a mut Option<BufferViewHandle>,
}

pub struct ScriptEngine {
    lua: Lua,
}

impl ScriptEngine {
    pub fn new() -> Self {
        Self::try_new().unwrap()
    }

    pub fn try_new() -> ScriptResult<Self> {
        let libs = mlua::StdLib::TABLE
            | mlua::StdLib::STRING
            | mlua::StdLib::UTF8
            | mlua::StdLib::MATH
            | mlua::StdLib::PACKAGE;
        let lua = Lua::new_with(libs)?;
        Ok(Self { lua })
    }

    pub fn register_ctx_function<'lua, A, R, F>(
        &'lua mut self,
        name: &str,
        func: F,
    ) -> ScriptResult<()>
    where
        A: FromLuaMulti<'lua>,
        R: ToLuaMulti<'lua>,
        F: 'static + Fn(&mut ScriptContext, A) -> ScriptResult<R>
    {
        let func = self.lua.create_function(move |lua, args| {
            let ctx: LuaLightUserData = lua.named_registry_value("ctx")?;
            let ctx = unsafe { &mut *(ctx.0 as *mut _) };
            func(ctx, args)
        })?;
        self.lua.globals().set(name, func)?;
        Ok(())
    }

    pub fn eval(&mut self, mut ctx: ScriptContext, source: &str) -> ScriptResult<()> {
        self.update_ctx(&mut ctx)?;
        self.lua.load(source).exec()?;
        Ok(())
    }

    pub fn load_entry_file(&mut self, mut ctx: ScriptContext, path: &Path) -> ScriptResult<()> {
        let mut file = File::open(path).map_err(|e| LuaError::ExternalError(Arc::new(e)))?;
        let metadata = file
            .metadata()
            .map_err(|e| LuaError::ExternalError(Arc::new(e)))?;
        let mut source = String::with_capacity(metadata.len() as _);
        file.read_to_string(&mut source)
            .map_err(|e| LuaError::ExternalError(Arc::new(e)))?;

        self.update_ctx(&mut ctx)?;

        let chunk = self.lua.load(&source);
        let chunk = if let Some(name) = path.to_str() {
            chunk.set_name(name)?
        } else {
            chunk
        };

        chunk.exec()?;
        Ok(())
    }

    fn update_ctx(&mut self, ctx: &mut ScriptContext) -> ScriptResult<()> {
        self.lua
            .set_named_registry_value("ctx", LuaLightUserData(ctx as *mut ScriptContext as _))
    }
}
