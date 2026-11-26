use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffLayout {
    #[default]
    Unified,
    Split,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    System,
    #[default]
    Dark,
    Light,
}

impl ThemePreference {
    pub fn is_light(self, system_is_light: bool) -> bool {
        match self {
            Self::System => system_is_light,
            Self::Dark => false,
            Self::Light => true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub diff_layout: DiffLayout,
    pub wrap_lines: bool,
    pub theme: ThemePreference,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"wrapLines":true}"#).unwrap();
        assert_eq!(
            settings,
            Settings {
                diff_layout: DiffLayout::Unified,
                wrap_lines: true,
                theme: ThemePreference::Dark,
            }
        );
    }

    #[test]
    fn system_theme_follows_the_window_appearance() {
        assert!(ThemePreference::System.is_light(true));
        assert!(!ThemePreference::System.is_light(false));
        assert!(ThemePreference::Light.is_light(false));
        assert!(!ThemePreference::Dark.is_light(true));
    }
}
