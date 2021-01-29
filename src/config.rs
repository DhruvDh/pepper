use std::num::NonZeroU8;

use crate::{
    syntax::SyntaxCollection,
    theme::{pico8_theme, Theme},
};

#[derive(Debug, Clone)]
pub struct ConfigValues {
    pub tab_size: NonZeroU8,
    pub indent_with_tabs: bool,

    pub visual_empty: u8,
    pub visual_space: u8,
    pub visual_tab_first: u8,
    pub visual_tab_repeat: u8,

    pub picker_max_height: NonZeroU8,
}

impl Default for ConfigValues {
    fn default() -> Self {
        Self {
            tab_size: NonZeroU8::new(4).unwrap(),
            indent_with_tabs: true,

            visual_empty: b'~',
            visual_space: b'.',
            visual_tab_first: b'|',
            visual_tab_repeat: b' ',

            picker_max_height: NonZeroU8::new(8).unwrap(),
        }
    }
}

pub struct Config {
    pub values: ConfigValues,
    pub theme: Theme,
    pub syntaxes: SyntaxCollection,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            values: ConfigValues::default(),
            theme: pico8_theme(),
            syntaxes: SyntaxCollection::new(),
        }
    }
}
