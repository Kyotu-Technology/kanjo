//! Alert texts in English and Polish, rendered from the stored parameters at
//! read time, so switching the language re-renders old alerts too.

use kanjo_core::api::AlertKind;
use kanjo_core::time::{DAY_MS, MINUTE_MS};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    En,
    Pl,
}

/// Title and message of an alert. `lang` is `pl` (or `pl-PL`) for Polish; anything else gives English.
pub fn render(kind: AlertKind, params: &Value, lang: &str) -> (String, String) {
    let lang = if lang.trim().to_ascii_lowercase().starts_with("pl") { Lang::Pl } else { Lang::En };
    let p = P(params);
    match kind {
        AlertKind::LimitThreshold => limit_threshold(lang, &p),
        AlertKind::LimitForecast => limit_forecast(lang, &p),
        AlertKind::BudgetWarning => budget_warning(lang, &p),
        AlertKind::BudgetExceeded => budget_exceeded(lang, &p),
        AlertKind::CostSpike => cost_spike(lang, &p),
        AlertKind::RunawaySession => runaway_session(lang, &p),
        AlertKind::AgentLoop => agent_loop(lang, &p),
        AlertKind::CacheCold => cache_cold(lang, &p),
        AlertKind::CacheMissStreak => cache_miss_streak(lang, &p),
        AlertKind::ContextNearLimit => context_near_limit(lang, &p),
    }
}

struct P<'a>(&'a Value);

impl P<'_> {
    fn opt_f(&self, k: &str) -> Option<f64> {
        self.0.get(k).and_then(Value::as_f64)
    }
    fn f(&self, k: &str) -> f64 {
        self.opt_f(k).unwrap_or(0.0)
    }
    fn opt_i(&self, k: &str) -> Option<i64> {
        self.0.get(k).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
    }
    fn i(&self, k: &str) -> i64 {
        self.opt_i(k).unwrap_or(0)
    }
    fn opt_s(&self, k: &str) -> Option<&str> {
        self.0.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
    }
    fn s(&self, k: &str) -> &str {
        self.opt_s(k).unwrap_or("")
    }
    /// Reference moment for "today" / "tomorrow": when the alert was evaluated.
    fn when(&self, lang: Lang, k: &str) -> String {
        let ts = self.i(k);
        when(lang, ts, self.opt_i("evaluated_at").unwrap_or(ts), self.i("tz_offset_min"))
    }
}

fn pct(v: f64) -> String {
    format!("{v:.0}%")
}

fn decimal(lang: Lang, v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    match lang {
        Lang::En => s,
        Lang::Pl => s.replace('.', ","),
    }
}

fn money(lang: Lang, v: f64) -> String {
    match lang {
        Lang::En => format!("${v:.2}"),
        Lang::Pl => format!("{} USD", decimal(lang, v, 2)),
    }
}

fn tokens(lang: Lang, n: i64) -> String {
    let n = n.max(0) as f64;
    match lang {
        Lang::En if n >= 1e6 => format!("{}M", decimal(lang, n / 1e6, 1)),
        Lang::En if n >= 1e3 => format!("{:.0}k", n / 1e3),
        Lang::Pl if n >= 1e6 => format!("{} mln", decimal(lang, n / 1e6, 1)),
        Lang::Pl if n >= 1e3 => format!("{:.0} tys.", n / 1e3),
        _ => format!("{n:.0}"),
    }
}

/// Polish plural form: 1 -> `one`, 2-4 (but not 12-14) -> `few`, otherwise `many`.
fn pl_plural<'a>(n: i64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let n = n.abs();
    if n == 1 {
        one
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        few
    } else {
        many
    }
}

fn en_plural<'a>(n: i64, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}

const EN_DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const PL_DAYS: [&str; 7] = ["w poniedziałek", "we wtorek", "w środę", "w czwartek", "w piątek", "w sobotę", "w niedzielę"];

/// Local day and time of `ts` relative to `reference`: "today at 14:00", "on Thursday at 14:00".
fn when(lang: Lang, ts: i64, reference: i64, tz_offset_min: i64) -> String {
    let off = tz_offset_min * MINUTE_MS;
    let day = (ts + off).div_euclid(DAY_MS);
    let ref_day = (reference + off).div_euclid(DAY_MS);
    let minutes = (ts + off).rem_euclid(DAY_MS) / MINUTE_MS;
    let (h, m) = (minutes / 60, minutes % 60);
    let weekday = (day + 3).rem_euclid(7) as usize;
    match lang {
        Lang::En => {
            let time = format!("{h:02}:{m:02}");
            match day - ref_day {
                0 => format!("today at {time}"),
                1 => format!("tomorrow at {time}"),
                -1 => format!("yesterday at {time}"),
                _ => format!("on {} at {time}", EN_DAYS[weekday]),
            }
        }
        Lang::Pl => {
            let time = format!("{h}:{m:02}");
            match day - ref_day {
                0 => format!("dziś o {time}"),
                1 => format!("jutro o {time}"),
                -1 => format!("wczoraj o {time}"),
                _ => format!("{} o {time}", PL_DAYS[weekday]),
            }
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Window name: English, Polish nominative, Polish genitive.
fn window(w: &str) -> (&'static str, &'static str, &'static str) {
    match w {
        "five_hour" => ("5-hour limit", "limit 5-godzinny", "limitu 5-godzinnego"),
        "seven_day" => ("weekly limit", "limit tygodniowy", "limitu tygodniowego"),
        _ => ("usage limit", "limit wykorzystania", "limitu wykorzystania"),
    }
}

fn limit_threshold(lang: Lang, p: &P) -> (String, String) {
    let (en, _, pl_gen) = window(p.s("window"));
    let th = pct(p.f("threshold"));
    let used = pct(p.f("used_pct"));
    let resets = p.when(lang, "resets_at");
    match lang {
        Lang::En => (format!("{} at {th}", capitalize(en)), format!("You have used {used} of the {en}. It resets {resets}.")),
        Lang::Pl => (format!("Wykorzystano {th} {pl_gen}"), format!("Zużycie {pl_gen} wynosi {used}. Reset nastąpi {resets}.")),
    }
}

fn limit_forecast(lang: Lang, p: &P) -> (String, String) {
    let (en, pl_nom, _) = window(p.s("window"));
    let used = pct(p.f("used_pct"));
    let out = p.when(lang, "exhausts_at");
    let resets = p.when(lang, "resets_at");
    match lang {
        Lang::En => (
            format!("{} will run out before the reset", capitalize(en)),
            format!("At the current pace the {en} runs out {out}, before it resets {resets}. Used so far: {used}."),
        ),
        Lang::Pl => (
            format!("{} skończy się przed resetem", capitalize(pl_nom)),
            format!("Przy obecnym tempie {pl_nom} skończy się {out}, a reset nastąpi dopiero {resets}. Dotąd zużyto {used}."),
        ),
    }
}

/// Period words: English adjective and noun; Polish genitive adjective, accusative adjective, genitive noun.
fn period(p: &str) -> (&'static str, &'static str, &'static str, &'static str, &'static str) {
    match p {
        "day" => ("daily", "day", "dziennego", "dzienny", "dnia"),
        "week" => ("weekly", "week", "tygodniowego", "tygodniowy", "tygodnia"),
        _ => ("monthly", "month", "miesięcznego", "miesięczny", "miesiąca"),
    }
}

fn budget_scope(lang: Lang, p: &P) -> String {
    let label = p.opt_s("scope_label").or(p.opt_s("scope_value"));
    let Some(label) = label else { return String::new() };
    match (lang, p.s("scope")) {
        (Lang::En, "project") => format!(" for project {label}"),
        (Lang::En, "client") => format!(" for client {label}"),
        (Lang::En, "account") => format!(" for account {label}"),
        (Lang::Pl, "project") => format!(" dla projektu {label}"),
        (Lang::Pl, "client") => format!(" dla klienta {label}"),
        (Lang::Pl, "account") => format!(" dla konta {label}"),
        _ => String::new(),
    }
}

fn budget_warning(lang: Lang, p: &P) -> (String, String) {
    let name = p.s("name");
    let (en_adj, en_noun, pl_gen, _, pl_noun) = period(p.s("period"));
    let spent = money(lang, p.f("spent_usd"));
    let amount = money(lang, p.f("amount_usd"));
    let projected = money(lang, p.f("projected_usd"));
    let share = pct(p.f("spent_pct"));
    let scope = budget_scope(lang, p);
    match lang {
        Lang::En => (
            format!("Budget \"{name}\" at {share}"),
            format!("Spent {spent} of the {amount} {en_adj} budget{scope} ({share}). At this pace the {en_noun} would close at {projected}."),
        ),
        Lang::Pl => (
            format!("Budżet „{name}” wykorzystany w {share}"),
            format!(
                "Wydano {spent} z {pl_gen} budżetu {amount}{scope} ({share}). Przy tym tempie wydatki na koniec {pl_noun} wyniosłyby {projected}."
            ),
        ),
    }
}

fn budget_exceeded(lang: Lang, p: &P) -> (String, String) {
    let name = p.s("name");
    let (en_adj, _, _, pl_acc, _) = period(p.s("period"));
    let spent = money(lang, p.f("spent_usd"));
    let amount = money(lang, p.f("amount_usd"));
    let share = pct(p.f("spent_pct"));
    let scope = budget_scope(lang, p);
    match lang {
        Lang::En => (format!("Budget \"{name}\" exceeded"), format!("Spent {spent}, over the {amount} {en_adj} budget{scope} ({share}).")),
        Lang::Pl => (
            format!("Przekroczono budżet „{name}”"),
            format!("Wydatki wyniosły {spent} i przekroczyły {pl_acc} budżet {amount}{scope} ({share})."),
        ),
    }
}

fn cost_spike(lang: Lang, p: &P) -> (String, String) {
    let date = p.s("date");
    let cost = money(lang, p.f("cost_usd"));
    let median = money(lang, p.f("median_usd"));
    let factor = decimal(lang, p.f("factor"), 1);
    let days = p.i("baseline_days");
    match lang {
        Lang::En => (
            "Unusually expensive day".into(),
            format!("Cost on {date} reached {cost}, {factor} times the median day ({median} over the last {days} active days)."),
        ),
        Lang::Pl => (
            "Nietypowo drogi dzień".into(),
            format!(
                "Koszt w dniu {date} sięgnął {cost}, czyli {factor} raza więcej niż mediana ({median} z ostatnich {days} aktywnych dni)."
            ),
        ),
    }
}

/// Short session reference with the project: "1a2b3c4d in acme" / "1a2b3c4d w projekcie acme".
fn session_ref(lang: Lang, p: &P) -> String {
    let id: String = p.s("session_id").chars().take(8).collect();
    match (lang, p.opt_s("project")) {
        (Lang::En, Some(project)) => format!("{id} in {project}"),
        (Lang::Pl, Some(project)) => format!("{id} w projekcie {project}"),
        (_, None) => id,
    }
}

fn runaway_session(lang: Lang, p: &P) -> (String, String) {
    let session = session_ref(lang, p);
    let cost = money(lang, p.f("cost_usd"));
    let median = money(lang, p.f("median_usd"));
    let factor = decimal(lang, p.f("factor"), 1);
    match lang {
        Lang::En => {
            ("Runaway session".into(), format!("Session {session} has reached {cost}, {factor} times a typical session ({median})."))
        }
        Lang::Pl => (
            "Sesja wymyka się spod kontroli".into(),
            format!("Koszt sesji {session} sięgnął {cost}, czyli {factor} raza więcej niż koszt typowej sesji ({median})."),
        ),
    }
}

fn agent_loop(lang: Lang, p: &P) -> (String, String) {
    let session = session_ref(lang, p);
    let n = p.i("requests");
    let minutes = p.i("window_minutes");
    let lines = p.i("lines_changed");
    match lang {
        Lang::En => {
            let changed = if lines == 0 {
                "without changing any code".to_string()
            } else {
                format!("but changed only {lines} {} of code", en_plural(lines, "line", "lines"))
            };
            ("Possible agent loop".into(), format!("Session {session} made {n} requests in {minutes} minutes {changed}."))
        }
        Lang::Pl => {
            let changed = if lines == 0 {
                ", nie zmieniając ani jednej linii kodu".to_string()
            } else {
                format!(", a zmieniła tylko {lines} {} kodu", pl_plural(lines, "linię", "linie", "linii"))
            };
            (
                "Możliwa pętla agenta".into(),
                format!(
                    "Sesja {session} wysłała {n} {} w ciągu {minutes} {}{changed}.",
                    pl_plural(n, "zapytanie", "zapytania", "zapytań"),
                    if minutes == 1 { "minuty" } else { "minut" }
                ),
            )
        }
    }
}

fn cache_cold(lang: Lang, p: &P) -> (String, String) {
    let session = session_ref(lang, p);
    let at = p.when(lang, "expired_at");
    let n = tokens(lang, p.i("recache_tokens"));
    let usd = p.opt_f("recache_usd").map(|v| money(lang, v));
    match lang {
        Lang::En => {
            let cost = usd.map(|u| format!(" (about {u})")).unwrap_or_default();
            (
                "Prompt cache expired".into(),
                format!("The prompt cache of session {session} expired {at}. The next request will write about {n} tokens to the cache again{cost}."),
            )
        }
        Lang::Pl => {
            let cost = usd.map(|u| format!(" (ok. {u})")).unwrap_or_default();
            (
                "Cache promptu wygasł".into(),
                format!("Cache promptu sesji {session} wygasł {at}. Następne zapytanie zapisze w nim ponownie około {n} tokenów{cost}."),
            )
        }
    }
}

/// Human name of a cache miss cause reported by Claude Code; unknown causes are shown as they come.
fn cause_name(lang: Lang, cause: &str) -> String {
    let known = match cause {
        "tools_changed" => Some(("tool set changed", "zmiana zestawu narzędzi")),
        "system_changed" | "system_prompt_changed" => Some(("system prompt changed", "zmiana promptu systemowego")),
        "model_changed" => Some(("model switched", "zmiana modelu")),
        "ttl_expired" | "expired" | "cache_expired" => Some(("cache expired", "wygaśnięcie cache")),
        "claude_md_changed" => Some(("CLAUDE.md changed", "zmiana pliku CLAUDE.md")),
        _ => None,
    };
    match (known, lang) {
        (Some((en, _)), Lang::En) => en.into(),
        (Some((_, pl)), Lang::Pl) => pl.into(),
        (None, _) => cause.replace('_', " "),
    }
}

fn cache_miss_streak(lang: Lang, p: &P) -> (String, String) {
    let session = session_ref(lang, p);
    let n = p.i("misses");
    let minutes = p.i("window_minutes");
    let cause = p.opt_s("top_cause").map(|c| cause_name(lang, c));
    match lang {
        Lang::En => {
            let cause = cause.map(|c| format!(" Most common cause: {c}.")).unwrap_or_default();
            (
                "Repeated cache misses".into(),
                format!("Session {session} had to rebuild the prompt cache {n} times in {minutes} minutes.{cause}"),
            )
        }
        Lang::Pl => {
            let cause = cause.map(|c| format!(" Najczęstsza przyczyna: {c}.")).unwrap_or_default();
            (
                "Powtarzające się chybienia cache".into(),
                format!(
                    "Sesja {session} musiała {n} {} w ciągu {minutes} {} odbudować cache promptu.{cause}",
                    if n == 1 { "raz" } else { "razy" },
                    if minutes == 1 { "minuty" } else { "minut" }
                ),
            )
        }
    }
}

fn context_near_limit(lang: Lang, p: &P) -> (String, String) {
    let session = session_ref(lang, p);
    let used = pct(p.f("ctx_used_pct"));
    match lang {
        Lang::En => (
            "Context window almost full".into(),
            format!("Session {session} uses {used} of its context window. Consider /compact or a fresh session."),
        ),
        Lang::Pl => (
            "Okno kontekstu prawie pełne".into(),
            format!("Sesja {session} wykorzystuje {used} okna kontekstu. Warto użyć /compact albo zacząć nową sesję."),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn polish_plurals() {
        let f = |n| pl_plural(n, "zapytanie", "zapytania", "zapytań");
        assert_eq!([f(1), f(3), f(5), f(12), f(22), f(25)], ["zapytanie", "zapytania", "zapytań", "zapytań", "zapytania", "zapytań"]);
    }

    #[test]
    fn day_names_are_local() {
        // 2026-11-26T10:00Z is a Thursday; 13:00Z is 14:00 in Warsaw winter time.
        let now = 1_795_687_200_000;
        let thu_13z = now + 3 * 3_600_000;
        assert_eq!(when(Lang::En, thu_13z, now, 60), "today at 14:00");
        assert_eq!(when(Lang::Pl, thu_13z + DAY_MS, now, 60), "jutro o 14:00");
        assert_eq!(when(Lang::En, thu_13z + 2 * DAY_MS, now, 60), "on Saturday at 14:00");
        assert_eq!(when(Lang::Pl, thu_13z + 5 * DAY_MS - 5 * 3_600_000, now, 60), "we wtorek o 9:00");
    }

    #[test]
    fn forecast_text_names_day_and_time() {
        let now = 1_795_687_200_000;
        let params = json!({
            "window": "seven_day", "used_pct": 62.0, "exhausts_at": now + 3 * 3_600_000 + 7 * DAY_MS / 7,
            "resets_at": now + 2 * DAY_MS, "evaluated_at": now, "tz_offset_min": 60
        });
        let (title, message) = render(AlertKind::LimitForecast, &params, "en");
        assert_eq!(title, "Weekly limit will run out before the reset");
        assert_eq!(
            message,
            "At the current pace the weekly limit runs out tomorrow at 14:00, before it resets on Saturday at 11:00. Used so far: 62%."
        );
        let (title, message) = render(AlertKind::LimitForecast, &params, "pl");
        assert_eq!(title, "Limit tygodniowy skończy się przed resetem");
        assert_eq!(
            message,
            "Przy obecnym tempie limit tygodniowy skończy się jutro o 14:00, a reset nastąpi dopiero w sobotę o 11:00. Dotąd zużyto 62%."
        );
    }
}
