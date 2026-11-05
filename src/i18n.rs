use std::collections::HashMap;

use crate::core::config::models::Language;

const ENGLISH_CATALOG: &str = include_str!("../resources/i18n/en-US.json");
const CHINESE_CATALOG: &str = include_str!("../resources/i18n/zh-CN.json");

#[derive(Debug, Clone)]
pub struct I18n {
    language: Language,
    english: HashMap<String, String>,
    selected: HashMap<String, String>,
}

impl I18n {
    pub fn new(language: Language) -> Self {
        let english = parse_catalog(ENGLISH_CATALOG);
        let selected = match language {
            Language::English => english.clone(),
            Language::SimplifiedChinese => parse_catalog(CHINESE_CATALOG),
        };
        Self {
            language,
            english,
            selected,
        }
    }

    pub const fn language(&self) -> Language {
        self.language
    }

    pub fn text(&self, key: &str) -> String {
        self.selected
            .get(key)
            .or_else(|| self.english.get(key))
            .cloned()
            .unwrap_or_else(|| format!("[{key}]"))
    }

    pub fn text_or_raw(&self, key_or_text: &str) -> String {
        self.selected
            .get(key_or_text)
            .or_else(|| self.english.get(key_or_text))
            .cloned()
            .unwrap_or_else(|| key_or_text.to_owned())
    }

    /// Translation for `key`, or `None` when the catalog has no entry.
    ///
    /// Used where the fallback is domain data (an API-provided category name)
    /// rather than the key itself.
    pub fn try_text(&self, key: &str) -> Option<String> {
        self.selected
            .get(key)
            .or_else(|| self.english.get(key))
            .cloned()
    }

    pub fn format(&self, key: &str, arguments: &[(&str, &str)]) -> String {
        let mut value = self.text(key);
        for (name, replacement) in arguments {
            value = value.replace(&format!("{{{name}}}"), replacement);
        }
        value
    }

    pub fn language_name(&self, language: Language) -> String {
        match language {
            Language::English => self.text("language.english"),
            Language::SimplifiedChinese => self.text("language.simplified_chinese"),
        }
    }
}

fn parse_catalog(source: &str) -> HashMap<String, String> {
    serde_json::from_str(source).expect("valid embedded i18n catalog")
}

#[cfg(test)]
mod tests {
    use super::{CHINESE_CATALOG, ENGLISH_CATALOG, I18n, parse_catalog};
    use crate::core::config::models::Language;

    #[test]
    fn english_is_the_default_and_unknown_codes_fall_back() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(Language::from_code("unknown"), Language::English);
        assert_eq!(Language::from_code("zh-CN"), Language::SimplifiedChinese);
    }

    #[test]
    fn language_serializes_to_stable_codes() {
        assert_eq!(
            serde_json::to_string(&Language::English).unwrap(),
            "\"en-US\""
        );
        assert_eq!(
            serde_json::to_string(&Language::SimplifiedChinese).unwrap(),
            "\"zh-CN\""
        );
        assert_eq!(
            serde_json::from_str::<Language>("\"not-a-language\"").unwrap(),
            Language::English
        );
        assert_eq!(
            serde_json::from_str::<Language>("null").unwrap(),
            Language::English
        );
    }

    #[test]
    fn missing_keys_fall_back_to_english_or_key() {
        let i18n = I18n::new(Language::SimplifiedChinese);
        assert_eq!(i18n.text("language.english"), "英语");
        assert_eq!(
            i18n.text("missing.synthetic.key"),
            "[missing.synthetic.key]"
        );
    }

    #[test]
    fn named_arguments_are_replaced_without_translating_values() {
        let i18n = I18n::new(Language::SimplifiedChinese);
        assert_eq!(
            i18n.format("status.loaded_versions", &[("count", "916")]),
            "已加载 916 个 Minecraft 版本"
        );
        assert_eq!(
            i18n.format("instance.loader", &[("loader", "fabric")]),
            "加载器：fabric"
        );
    }

    #[test]
    fn checked_in_catalogs_have_the_same_keys() {
        let english = parse_catalog(ENGLISH_CATALOG);
        let chinese = parse_catalog(CHINESE_CATALOG);
        let english_keys = english.keys().collect::<std::collections::BTreeSet<_>>();
        let chinese_keys = chinese.keys().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(english_keys, chinese_keys);
    }
}
