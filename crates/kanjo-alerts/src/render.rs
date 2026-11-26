//! Alert texts in English and Polish, rendered from the stored parameters at
//! read time, so switching the language re-renders old alerts too.

use kanjo_core::api::AlertKind;
use kanjo_core::settings::CurrencySettings;
use kanjo_core::time::{utc_datetime, DAY_MS, MINUTE_MS};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    En,
    Pl,
}

pub(crate) struct Style<'a> {
    pub lang: &'a str,
    pub currency: &'a CurrencySettings,
    pub now: Option<i64>,
}

/// Title and message of an alert in USD, worded as on the day it was created.
/// `lang` is `pl` (or `pl-PL`) for Polish; anything else gives English.
pub fn render(kind: AlertKind, params: &Value, lang: &str) -> (String, String) {
    render_styled(kind, params, &Style { lang, currency: &CurrencySettings::default(), now: None })
}

pub(crate) fn render_styled(kind: AlertKind, params: &Value, style: &Style) -> (String, String) {
    let lang = if style.lang.trim().to_ascii_lowercase().starts_with("pl") { Lang::Pl } else { Lang::En };
    let p = P { v: params, currency: style.currency, now: style.now };
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

struct P<'a> {
    v: &'a Value,
    currency: &'a CurrencySettings,
    now: Option<i64>,
}

impl P<'_> {
    fn opt_f(&self, k: &str) -> Option<f64> {
        self.v.get(k).and_then(Value::as_f64)
    }
    fn f(&self, k: &str) -> f64 {
        self.opt_f(k).unwrap_or(0.0)
    }
    fn opt_i(&self, k: &str) -> Option<i64> {
        self.v.get(k).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
    }
    fn i(&self, k: &str) -> i64 {
        self.opt_i(k).unwrap_or(0)
    }
    fn opt_s(&self, k: &str) -> Option<&str> {
        self.v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
    }
    fn s(&self, k: &str) -> &str {
        self.opt_s(k).unwrap_or("")
    }
    /// The moment `k` relative to when the alert was evaluated, or as a date once that day is over.
    fn when(&self, lang: Lang, k: &str) -> String {
        let ts = self.i(k);
        let created = self.opt_i("evaluated_at").or(self.now).unwrap_or(ts);
        when(lang, ts, created, self.now.unwrap_or(created), self.i("tz_offset_min"))
    }

    fn date(&self, lang: Lang, k: &str) -> String {
        let raw = self.s(k);
        let parts: Vec<i64> = raw.split('-').filter_map(|x| x.parse().ok()).collect();
        let [y, m, d] = parts[..] else { return raw.to_string() };
        if !(1..=12).contains(&m) {
            return raw.to_string();
        }
        let this_year = self.now.map(|n| utc_datetime(n + self.i("tz_offset_min") * MINUTE_MS).year() as i64).unwrap_or(y);
        let date = date_words(lang, y, m, d, this_year);
        match lang {
            Lang::En => format!("on {date}"),
            Lang::Pl => format!("w dniu {date}"),
        }
    }

    fn money(&self, lang: Lang, usd: f64) -> String {
        let rate = self.currency.usd_rate;
        let value = usd * if rate.is_finite() && rate > 0.0 { rate } else { 1.0 };
        let code = self.currency.code.trim().to_ascii_uppercase();
        let symbol = currency_symbol(if code.is_empty() { "USD" } else { &code });
        let places = if value.abs() >= 1000.0 { 0 } else { 2 };
        let digits = format!("{:.places$}", value.abs());
        let (int, frac) = digits.split_once('.').unwrap_or((&digits, ""));
        let (group, point, min_grouped) = match lang {
            Lang::En => (",", ".", 4),
            Lang::Pl => ("\u{a0}", ",", 5),
        };
        let mut number = String::new();
        for (i, c) in int.chars().enumerate() {
            if i > 0 && int.len() >= min_grouped && (int.len() - i) % 3 == 0 {
                number.push_str(group);
            }
            number.push(c);
        }
        if !frac.is_empty() {
            number.push_str(point);
            number.push_str(frac);
        }
        let sign = if value < 0.0 && digits.chars().any(|c| c.is_ascii_digit() && c != '0') { "-" } else { "" };
        match lang {
            Lang::En if symbol.ends_with(char::is_alphabetic) => format!("{sign}{symbol}\u{a0}{number}"),
            Lang::En => format!("{sign}{symbol}{number}"),
            Lang::Pl => format!("{sign}{number}\u{a0}{symbol}"),
        }
    }
}

fn currency_symbol(code: &str) -> &str {
    match code {
        "USD" | "AUD" | "CAD" | "NZD" | "MXN" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" | "CNY" => "¥",
        "PLN" => "zł",
        "CZK" => "Kč",
        "SEK" | "NOK" | "DKK" => "kr",
        "HUF" => "Ft",
        "INR" => "₹",
        "UAH" => "₴",
        "KRW" => "₩",
        "BRL" => "R$",
        other => other,
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
const EN_MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const PL_MONTHS: [&str; 12] =
    ["stycznia", "lutego", "marca", "kwietnia", "maja", "czerwca", "lipca", "sierpnia", "września", "października", "listopada", "grudnia"];

fn date_words(lang: Lang, y: i64, m: i64, d: i64, this_year: i64) -> String {
    let month = match lang {
        Lang::En => EN_MONTHS[(m - 1) as usize],
        Lang::Pl => PL_MONTHS[(m - 1) as usize],
    };
    if y == this_year {
        format!("{d} {month}")
    } else {
        format!("{d} {month} {y}")
    }
}

/// Local day and time of `ts` relative to `created`, the day the alert was evaluated:
/// "today at 14:00", "on Thursday at 14:00", "on 3 December at 09:00". When `read`
/// falls on a later local day than `created`, the moment is always written as a date.
fn when(lang: Lang, ts: i64, created: i64, read: i64, tz_offset_min: i64) -> String {
    let off = tz_offset_min * MINUTE_MS;
    let local_day = |t: i64| (t + off).div_euclid(DAY_MS);
    let (day, ref_day) = (local_day(ts), local_day(created));
    let delta = if local_day(read) == ref_day { Some(day - ref_day) } else { None };
    let minutes = (ts + off).rem_euclid(DAY_MS) / MINUTE_MS;
    let time = format!("{:02}:{:02}", minutes / 60, minutes % 60);
    let weekday = (day + 3).rem_euclid(7) as usize;
    let civil = utc_datetime(ts + off);
    let date = date_words(lang, civil.year() as i64, civil.month() as i64, civil.day() as i64, utc_datetime(read + off).year() as i64);
    match lang {
        Lang::En => match delta {
            Some(0) => format!("today at {time}"),
            Some(1) => format!("tomorrow at {time}"),
            Some(-1) => format!("yesterday at {time}"),
            Some(2..=6) => format!("on {} at {time}", EN_DAYS[weekday]),
            _ => format!("on {date} at {time}"),
        },
        Lang::Pl => match delta {
            Some(0) => format!("dziś o {time}"),
            Some(1) => format!("jutro o {time}"),
            Some(-1) => format!("wczoraj o {time}"),
            Some(2..=6) => format!("{} o {time}", PL_DAYS[weekday]),
            _ => format!("{date} o {time}"),
        },
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
    let spent = p.money(lang, p.f("spent_usd"));
    let amount = p.money(lang, p.f("amount_usd"));
    let projected = p.money(lang, p.f("projected_usd"));
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
    let spent = p.money(lang, p.f("spent_usd"));
    let amount = p.money(lang, p.f("amount_usd"));
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
    let date = p.date(lang, "date");
    let cost = p.money(lang, p.f("cost_usd"));
    let median = p.money(lang, p.f("median_usd"));
    let factor = decimal(lang, p.f("factor"), 1);
    let days = p.i("baseline_days");
    match lang {
        Lang::En => (
            "Unusually expensive day".into(),
            format!("Cost {date} reached {cost}, {factor} times the median day ({median} over the last {days} active days)."),
        ),
        Lang::Pl => (
            "Nietypowo drogi dzień".into(),
            format!("Koszt {date} sięgnął {cost}, czyli {factor} raza więcej niż mediana ({median} z ostatnich {days} aktywnych dni)."),
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
    let cost = p.money(lang, p.f("cost_usd"));
    let median = p.money(lang, p.f("median_usd"));
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
    let usd = p.opt_f("recache_usd").map(|v| p.money(lang, v));
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
        assert_eq!(when(Lang::En, thu_13z, now, now, 60), "today at 14:00");
        assert_eq!(when(Lang::Pl, thu_13z + DAY_MS, now, now, 60), "jutro o 14:00");
        assert_eq!(when(Lang::En, thu_13z + 2 * DAY_MS, now, now, 60), "on Saturday at 14:00");
        assert_eq!(when(Lang::Pl, thu_13z + 5 * DAY_MS - 5 * 3_600_000, now, now, 60), "we wtorek o 09:00");
        assert_eq!(when(Lang::En, thu_13z + 7 * DAY_MS, now, now, 60), "on 3 December at 14:00");
        assert_eq!(when(Lang::Pl, thu_13z - 3 * DAY_MS, now, now, 60), "23 listopada o 14:00");
    }

    #[test]
    fn old_alerts_name_dates_instead_of_today() {
        let now = 1_795_687_200_000;
        let thu_13z = now + 3 * 3_600_000;
        let next_day = now + DAY_MS;
        assert_eq!(when(Lang::En, thu_13z, now, now + 12 * 3_600_000, 60), "today at 14:00");
        assert_eq!(when(Lang::En, thu_13z, now, next_day, 60), "on 26 November at 14:00");
        assert_eq!(when(Lang::Pl, thu_13z + DAY_MS, now, next_day, 60), "27 listopada o 14:00");
        assert_eq!(when(Lang::En, thu_13z, now, now + 60 * DAY_MS, 60), "on 26 November 2026 at 14:00");
    }

    #[test]
    fn money_follows_the_display_currency() {
        let usd = CurrencySettings::default();
        let pln = CurrencySettings { code: "PLN".into(), usd_rate: 4.0 };
        let chf = CurrencySettings { code: "chf".into(), usd_rate: 1.0 };
        let v = json!({});
        let p = |c| P { v: &v, currency: c, now: None };
        assert_eq!(p(&usd).money(Lang::En, 28.07), "$28.07");
        assert_eq!(p(&usd).money(Lang::En, 1400.0), "$1,400");
        assert_eq!(p(&usd).money(Lang::Pl, 28.07), "28,07\u{a0}$");
        assert_eq!(p(&pln).money(Lang::Pl, 101.25), "405,00\u{a0}zł");
        assert_eq!(p(&pln).money(Lang::Pl, 350.0), "1400\u{a0}zł");
        assert_eq!(p(&pln).money(Lang::Pl, 3086.4), "12\u{a0}346\u{a0}zł");
        assert_eq!(p(&pln).money(Lang::En, 1.375), "zł\u{a0}5.50");
        assert_eq!(p(&chf).money(Lang::En, 1234567.0), "CHF\u{a0}1,234,567");
        let budget = json!({"name": "Team", "period": "month", "spent_usd": 101.25, "amount_usd": 100.0, "spent_pct": 101.25});
        let style = Style { lang: "pl", currency: &pln, now: None };
        let (_, message) = render_styled(AlertKind::BudgetExceeded, &budget, &style);
        assert_eq!(message, "Wydatki wyniosły 405,00\u{a0}zł i przekroczyły miesięczny budżet 400,00\u{a0}zł (101%).");
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
