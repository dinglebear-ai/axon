use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

pub(crate) struct AxonAssets;

const AXON_GLYPH_SVG: &[u8] = include_bytes!("../../../assets/axon-glyph.svg");

impl AssetSource for AxonAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "axon-glyph.svg" => Some(Cow::Borrowed(AXON_GLYPH_SVG)),
            _ => None,
        })
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(match path {
            "" | "." => vec![SharedString::from("axon-glyph.svg")],
            _ => Vec::new(),
        })
    }
}
