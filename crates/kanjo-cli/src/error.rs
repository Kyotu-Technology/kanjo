use crate::i18n::Lang;
use kanjo_core::Error;

/// A failure the user sees: what went wrong and what to do next. Never a stack trace.
pub enum Failure {
    Msg {
        message: String,
        hint: Option<String>,
    },
    Core(Error),
    /// The message was already printed; only the exit code is left.
    Silent(u8),
}

pub type CliResult<T> = std::result::Result<T, Failure>;

impl From<Error> for Failure {
    fn from(e: Error) -> Failure {
        Failure::Core(e)
    }
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Failure {
        Failure::Core(Error::Io(e))
    }
}

pub fn fail(message: String, hint: String) -> Failure {
    Failure::Msg { message, hint: Some(hint) }
}

impl Failure {
    pub fn exit_code(&self) -> u8 {
        match self {
            Failure::Silent(code) => *code,
            _ => 1,
        }
    }

    /// The text printed to stderr, in the user's language.
    pub fn render(&self, lang: Lang) -> Option<String> {
        let (message, hint) = match self {
            Failure::Silent(_) => return None,
            Failure::Msg { message, hint } => (message.clone(), hint.clone()),
            Failure::Core(e) => explain(e, lang),
        };
        let mut out = format!("{} {message}", lang.t("Error:", "Błąd:"));
        if let Some(hint) = hint {
            out.push_str(&format!("\n{} {hint}", lang.t("What to do:", "Co zrobić:")));
        }
        Some(out)
    }
}

fn explain(e: &Error, lang: Lang) -> (String, Option<String>) {
    match e {
        Error::Db(err) => (
            tr!(lang, "the database could not be used ({err}).", "nie udało się skorzystać z bazy danych ({err})."),
            Some(tr!(
                lang,
                "run \"kanjo doctor\" to see what is wrong. If the database is damaged, restore a backup with \"kanjo restore FILE\".",
                "uruchom \"kanjo doctor\", żeby zobaczyć, co jest nie tak. Jeśli baza jest uszkodzona, przywróć kopię poleceniem \"kanjo restore PLIK\"."
            )),
        ),
        Error::Io(err) => (
            tr!(lang, "a file could not be read or written ({err}).", "nie udało się odczytać ani zapisać pliku ({err})."),
            Some(tr!(
                lang,
                "check that the path exists and that you are allowed to use it.",
                "sprawdź, czy ścieżka istnieje i czy masz do niej uprawnienia."
            )),
        ),
        Error::Json(err) => (
            tr!(lang, "the data is not valid JSON ({err}).", "dane nie są poprawnym plikiem JSON ({err})."),
            Some(tr!(lang, "check the file you passed and try again.", "sprawdź podany plik i spróbuj ponownie.")),
        ),
        Error::Config(msg) => (
            tr!(lang, "configuration problem: {msg}.", "problem z konfiguracją: {msg}."),
            Some(tr!(lang, "run \"kanjo doctor\" for a full check.", "uruchom \"kanjo doctor\", żeby sprawdzić całą instalację.")),
        ),
        Error::Invalid(msg) => (
            tr!(lang, "{msg}.", "nieprawidłowe dane: {msg}."),
            Some(tr!(
                lang,
                "check the values you passed; \"kanjo help <command>\" lists what each command accepts.",
                "sprawdź podane wartości; \"kanjo help <polecenie>\" pokazuje, co przyjmuje każde polecenie."
            )),
        ),
        Error::NotFound(msg) => (
            tr!(lang, "not found: {msg}.", "nie znaleziono: {msg}."),
            Some(tr!(lang, "check the name or id and try again.", "sprawdź nazwę lub identyfikator i spróbuj ponownie.")),
        ),
        Error::Other(msg) => (
            tr!(lang, "{msg}.", "{msg}."),
            Some(tr!(lang, "run \"kanjo doctor\" for a full check.", "uruchom \"kanjo doctor\", żeby sprawdzić całą instalację.")),
        ),
    }
}
