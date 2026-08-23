use std::env;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    English,
    SimplifiedChinese,
}

impl Language {
    pub fn from_locale(locale: Option<&str>) -> Self {
        let Some(locale) = locale else {
            return Self::English;
        };
        let locale = locale.trim();
        if locale
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("zh"))
        {
            Self::SimplifiedChinese
        } else {
            Self::English
        }
    }
}

static LANGUAGE: LazyLock<Language> = LazyLock::new(|| {
    ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
        .as_deref()
        .map_or(Language::English, |locale| {
            Language::from_locale(Some(locale))
        })
});

pub fn language() -> Language {
    *LANGUAGE
}

pub fn tr(english: &'static str, simplified_chinese: &'static str) -> &'static str {
    match language() {
        Language::English => english,
        Language::SimplifiedChinese => simplified_chinese,
    }
}

#[cfg(test)]
mod tests {
    use super::Language;

    #[test]
    fn locale_parsing_is_pure_and_defaults_to_english() {
        for locale in [
            None,
            Some(""),
            Some("C"),
            Some("POSIX"),
            Some("en_US.UTF-8"),
        ] {
            assert_eq!(Language::from_locale(locale), Language::English);
        }
    }

    #[test]
    fn common_chinese_locale_forms_select_simplified_chinese() {
        for locale in ["zh", "zh_CN.UTF-8", "zh-CN", "zh_Hans", "ZH_cn@pinyin"] {
            assert_eq!(
                Language::from_locale(Some(locale)),
                Language::SimplifiedChinese
            );
        }
    }
}
