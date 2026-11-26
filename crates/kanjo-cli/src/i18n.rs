use kanjo_core::settings::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Pl,
}

impl Lang {
    pub fn from_code(code: &str) -> Option<Lang> {
        let code = code.trim().to_ascii_lowercase();
        if code.starts_with("pl") {
            Some(Lang::Pl)
        } else if code.starts_with("en") {
            Some(Lang::En)
        } else {
            None
        }
    }

    pub fn from_settings(language: Language) -> Option<Lang> {
        match language {
            Language::En => Some(Lang::En),
            Language::Pl => Some(Lang::Pl),
            Language::Auto => None,
        }
    }

    /// The first non-empty of LC_ALL, LC_MESSAGES and LANG decides; Polish locales give Polish, anything else English.
    pub fn from_env() -> Lang {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                if !v.is_empty() {
                    return if v.to_ascii_lowercase().starts_with("pl") { Lang::Pl } else { Lang::En };
                }
            }
        }
        Lang::En
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Pl => "pl",
        }
    }

    pub fn t(self, en: &'static str, pl: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Pl => pl,
        }
    }
}

/// `tr!(lang, "English {x}", "Polski {x}")` formats the text in the given language.
macro_rules! tr {
    ($lang:expr, $en:literal, $pl:literal $(,)?) => {
        match $lang {
            $crate::i18n::Lang::En => format!($en),
            $crate::i18n::Lang::Pl => format!($pl),
        }
    };
    ($lang:expr, $en:literal, $pl:literal, $($arg:tt)+) => {
        match $lang {
            $crate::i18n::Lang::En => format!($en, $($arg)+),
            $crate::i18n::Lang::Pl => format!($pl, $($arg)+),
        }
    };
}

/// Polish noun form for a count: 1 -> one, 2-4 (but not 12-14) -> few, otherwise many.
pub fn pl_plural(n: i64, one: &'static str, few: &'static str, many: &'static str) -> &'static str {
    let n = n.abs();
    if n == 1 {
        one
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        few
    } else {
        many
    }
}
