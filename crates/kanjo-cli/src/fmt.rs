//! Number, money, time and table formatting for human output.

use crate::i18n::{pl_plural, Lang};
use kanjo_core::time::{DAY_MS, HOUR_MS, MINUTE_MS};
use kanjo_core::Ctx;

fn group(digits: &str, sep: char) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

pub fn int(lang: Lang, n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let digits = n.unsigned_abs().to_string();
    format!("{sign}{}", group(&digits, if lang == Lang::Pl { '\u{a0}' } else { ',' }))
}

pub fn money(lang: Lang, usd: f64) -> String {
    let cents = (usd.abs() * 100.0).round() as i64;
    let sign = if usd < 0.0 && cents > 0 { "-" } else { "" };
    let whole = int(lang, cents / 100);
    let dec = if lang == Lang::Pl { ',' } else { '.' };
    format!("{sign}${whole}{dec}{:02}", cents % 100)
}

pub fn pct(lang: Lang, p: f64) -> String {
    let s = format!("{p:.0}%");
    if lang == Lang::Pl {
        s.replace('.', ",")
    } else {
        s
    }
}

pub fn decimal(lang: Lang, v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    if lang == Lang::Pl {
        s.replace('.', ",")
    } else {
        s
    }
}

pub fn date(ctx: &Ctx, ts: i64) -> String {
    ctx.date_label(ts)
}

pub fn clock(ctx: &Ctx, ts: i64) -> String {
    let local = ts + ctx.tz_offset_min as i64 * MINUTE_MS;
    let mins = local.rem_euclid(DAY_MS) / MINUTE_MS;
    format!("{:02}:{:02}", mins / 60, mins % 60)
}

pub fn date_time(ctx: &Ctx, ts: i64) -> String {
    format!("{} {}", date(ctx, ts), clock(ctx, ts))
}

/// A moment relative to now: the clock time today, otherwise the date and time.
pub fn when(ctx: &Ctx, ts: i64) -> String {
    if ctx.day_start(ts) == ctx.day_start(ctx.now) {
        clock(ctx, ts)
    } else {
        date_time(ctx, ts)
    }
}

/// "2 d 4 h", "1 h 20 min", "5 min"; Polish uses the matching noun forms for days.
pub fn duration(lang: Lang, ms: i64) -> String {
    let ms = ms.max(0);
    let days = ms / DAY_MS;
    let hours = (ms % DAY_MS) / HOUR_MS;
    let mins = (ms % HOUR_MS) / MINUTE_MS;
    let day_word = match lang {
        Lang::En => {
            if days == 1 {
                "day"
            } else {
                "days"
            }
        }
        Lang::Pl => pl_plural(days, "dzień", "dni", "dni"),
    };
    if days > 0 {
        if hours > 0 {
            format!("{days} {day_word} {hours} h")
        } else {
            format!("{days} {day_word}")
        }
    } else if hours > 0 {
        format!("{hours} h {mins} min")
    } else {
        format!("{} min", mins.max(if ms > 0 { 1 } else { 0 }))
    }
}

pub fn short_model(model: &str) -> &str {
    model.strip_prefix("claude-").unwrap_or(model)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

enum Row {
    Cells(Vec<String>),
    Rule,
}

/// A plain-text table with aligned columns. Widths are counted in characters,
/// which is right for Latin scripts with diacritics.
pub struct Table {
    headers: Vec<String>,
    align: Vec<Align>,
    rows: Vec<Row>,
}

impl Table {
    pub fn new(headers: &[(&str, Align)]) -> Table {
        Table {
            headers: headers.iter().map(|(h, _)| h.to_string()).collect(),
            align: headers.iter().map(|(_, a)| *a).collect(),
            rows: Vec::new(),
        }
    }

    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(Row::Cells(cells));
    }

    pub fn rule(&mut self) {
        self.rows.push(Row::Rule);
    }

    pub fn is_empty(&self) -> bool {
        !self.rows.iter().any(|r| matches!(r, Row::Cells(_)))
    }

    pub fn render(&self, bold_header: bool) -> String {
        let n = self.headers.len();
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.chars().count()).collect();
        for row in &self.rows {
            if let Row::Cells(cells) = row {
                for (i, c) in cells.iter().enumerate().take(n) {
                    widths[i] = widths[i].max(c.chars().count());
                }
            }
        }
        let line = |cells: &[String]| {
            let mut out = String::new();
            for (i, w) in widths.iter().enumerate() {
                let cell = cells.get(i).map(String::as_str).unwrap_or("");
                let pad = w.saturating_sub(cell.chars().count());
                if i > 0 {
                    out.push_str("  ");
                }
                if self.align[i] == Align::Right {
                    out.push_str(&" ".repeat(pad));
                    out.push_str(cell);
                } else {
                    out.push_str(cell);
                    if i + 1 < n {
                        out.push_str(&" ".repeat(pad));
                    }
                }
            }
            out.trim_end().to_string()
        };
        let total: usize = widths.iter().sum::<usize>() + 2 * n.saturating_sub(1);
        let mut out = String::new();
        let header = line(&self.headers);
        if bold_header {
            out.push_str(&format!("\x1b[1m{header}\x1b[0m\n"));
        } else {
            out.push_str(&header);
            out.push('\n');
        }
        out.push_str(&"-".repeat(total));
        out.push('\n');
        for row in &self.rows {
            match row {
                Row::Cells(cells) => out.push_str(&line(cells)),
                Row::Rule => out.push_str(&"-".repeat(total)),
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_follow_the_language() {
        assert_eq!(int(Lang::En, 1_234_567), "1,234,567");
        assert_eq!(int(Lang::Pl, 1_234_567), "1\u{a0}234\u{a0}567");
        assert_eq!(money(Lang::En, 1234.5), "$1,234.50");
        assert_eq!(money(Lang::Pl, 0.004), "$0,00");
        assert_eq!(duration(Lang::Pl, 2 * DAY_MS + 3 * HOUR_MS), "2 dni 3 h");
        assert_eq!(duration(Lang::Pl, DAY_MS), "1 dzień");
        assert_eq!(duration(Lang::En, 80 * MINUTE_MS), "1 h 20 min");
    }

    #[test]
    fn table_aligns_columns() {
        let mut t = Table::new(&[("Name", Align::Left), ("Cost", Align::Right)]);
        t.row(vec!["żółw".into(), "$1.00".into()]);
        t.row(vec!["a".into(), "$10.00".into()]);
        let text = t.render(false);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[2], "żółw   $1.00");
        assert_eq!(lines[3], "a     $10.00");
    }
}
