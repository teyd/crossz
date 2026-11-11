use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

// gpui-kit embeds only its components' icons; these are the extra ones the UI uses.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Crosshair, Gamepad2, Keyboard, Trash, Pencil, X, FolderOpen, Plus, Settings
    ]
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match ExtraIcons.load(path)? {
            Some(bytes) => Ok(Some(bytes)),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
