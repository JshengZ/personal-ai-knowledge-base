use crate::store::Store;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Local presentation preferences are independent of model/credential settings.
/// Keeping them in metadata makes backup and offline storage migration atomic.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub theme: String,
    pub accent: String,
    pub font_size: u8,
    pub reading_font: String,
    pub density: String,
    pub sidebar_collapsed: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            accent: "graphite".into(),
            font_size: 14,
            reading_font: "sans".into(),
            density: "comfortable".into(),
            sidebar_collapsed: false,
        }
    }
}

pub fn load(store: &Store) -> Result<Appearance> {
    Ok(store.metadata("appearance")?.unwrap_or_default())
}

pub fn update(store: &Store, input: Value) -> Result<Appearance> {
    let mut current = serde_json::to_value(load(store)?)?;
    let Some(changes) = input.as_object() else {
        bail!("外观设置格式无效");
    };
    for (key, value) in changes {
        if current.get(key).is_none() {
            bail!("未知外观选项：{key}");
        }
        current[key] = value.clone();
    }
    let value: Appearance = serde_json::from_value(current)?;
    let custom_color = value.accent.len() == 7
        && value.accent.starts_with('#')
        && value.accent.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit);
    if !["light", "dark", "system"].contains(&value.theme.as_str())
        || ![13, 14, 16, 18].contains(&value.font_size)
        || !["sans", "serif"].contains(&value.reading_font.as_str())
        || !["comfortable", "compact"].contains(&value.density.as_str())
        || (!custom_color
            && !["graphite", "blue", "green", "violet", "rose"].contains(&value.accent.as_str()))
    {
        bail!("请选择有效的主题、颜色、字号和界面密度。");
    }
    store.set_metadata("appearance", &value)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn appearance_is_validated_and_persisted_independently() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("knowledge.sqlite");
        let store = Store::open(&path).unwrap();
        assert_eq!(load(&store).unwrap(), Appearance::default());
        let settings = serde_json::to_value(store.settings().unwrap()).unwrap();
        let saved = update(&store, json!({"theme":"dark","accent":"#47a2ff","font_size":18,"sidebar_collapsed":true})).unwrap();
        for invalid in [json!({"theme":"invalid"}),json!({"font_size":100}),json!({"accent":"red; color: pink"}),json!({"chat":{}})] {
            assert!(update(&store, invalid).is_err());
        }
        assert_eq!(load(&store).unwrap(), saved);
        assert_eq!(serde_json::to_value(store.settings().unwrap()).unwrap(), settings);
        drop(store);
        assert_eq!(load(&Store::open(&path).unwrap()).unwrap(), saved);
    }
}
