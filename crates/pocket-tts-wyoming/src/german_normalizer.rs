//! Conservative German speech normalization. No tokenizer/model or runtime dependencies.
//! Technical tokens are kept intact; ambiguous decimal points require a known unit.

const SMALL: [&str; 20] = [
    "null",
    "eins",
    "zwei",
    "drei",
    "vier",
    "fünf",
    "sechs",
    "sieben",
    "acht",
    "neun",
    "zehn",
    "elf",
    "zwölf",
    "dreizehn",
    "vierzehn",
    "fünfzehn",
    "sechzehn",
    "siebzehn",
    "achtzehn",
    "neunzehn",
];
const TENS: [&str; 10] = [
    "", "", "zwanzig", "dreißig", "vierzig", "fünfzig", "sechzig", "siebzig", "achtzig", "neunzig",
];

fn under_thousand(n: u64) -> String {
    if n < 20 {
        return SMALL[n as usize].into();
    }
    if n < 100 {
        let unit = n % 10;
        return if unit == 0 {
            TENS[(n / 10) as usize].into()
        } else {
            format!(
                "{}und{}",
                if unit == 1 {
                    "ein"
                } else {
                    SMALL[unit as usize]
                },
                TENS[(n / 10) as usize]
            )
        };
    }
    format!(
        "{}hundert{}",
        if n / 100 == 1 {
            "ein"
        } else {
            SMALL[(n / 100) as usize]
        },
        if n % 100 == 0 {
            String::new()
        } else {
            under_thousand(n % 100)
        }
    )
}
/// Full u64 range. German long scale (Milliarde=10^9, Billion=10^12).
pub fn cardinal(n: u64) -> String {
    if n < 1000 {
        return under_thousand(n);
    }
    if n < 1_000_000 {
        return format!(
            "{}tausend{}",
            if n / 1000 == 1 {
                "ein".into()
            } else {
                under_thousand(n / 1000)
            },
            if n % 1000 == 0 {
                String::new()
            } else {
                under_thousand(n % 1000)
            }
        );
    }
    for (scale, singular, plural) in [
        (1_000_000_000_000_000_000, "Trillion", "Trillionen"),
        (1_000_000_000_000_000, "Billiarde", "Billiarden"),
        (1_000_000_000_000, "Billion", "Billionen"),
        (1_000_000_000, "Milliarde", "Milliarden"),
        (1_000_000, "Million", "Millionen"),
    ] {
        if n >= scale {
            let count = n / scale;
            let head = if count == 1 {
                format!("eine {singular}")
            } else {
                format!("{} {plural}", cardinal(count))
            };
            return if n % scale == 0 {
                head
            } else {
                format!("{head} {}", cardinal(n % scale))
            };
        }
    }
    unreachable!()
}

// Longest spellings first; boundaries prevent units matching parts of identifiers.
fn unit_at(s: &str) -> Option<(usize, &'static str, &'static str)> {
    for (raw, singular, plural) in [
        ("Grad Celsius", "Grad Celsius", "Grad Celsius"),
        ("°C", "Grad Celsius", "Grad Celsius"),
        ("Kilowattstunden", "Kilowattstunde", "Kilowattstunden"),
        ("Kilowattstunde", "Kilowattstunde", "Kilowattstunden"),
        ("Wattstunden", "Wattstunde", "Wattstunden"),
        ("Wattstunde", "Wattstunde", "Wattstunden"),
        ("Kilowatt", "Kilowatt", "Kilowatt"),
        ("Watt", "Watt", "Watt"),
        ("Meter", "Meter", "Meter"),
        ("Liter", "Liter", "Liter"),
        ("Prozent", "Prozent", "Prozent"),
        ("Grad", "Grad", "Grad"),
        ("km/h", "Kilometer pro Stunde", "Kilometer pro Stunde"),
        ("m/s", "Meter pro Sekunde", "Meter pro Sekunde"),
        ("kWh", "Kilowattstunde", "Kilowattstunden"),
        ("Wh", "Wattstunde", "Wattstunden"),
        ("kW", "Kilowatt", "Kilowatt"),
        ("mA", "Milliampere", "Milliampere"),
        ("kHz", "Kilohertz", "Kilohertz"),
        ("Hz", "Hertz", "Hertz"),
        ("hPa", "Hektopascal", "Hektopascal"),
        ("Pa", "Pascal", "Pascal"),
        ("cm", "Zentimeter", "Zentimeter"),
        ("mm", "Millimeter", "Millimeter"),
        ("km", "Kilometer", "Kilometer"),
        ("kg", "Kilogramm", "Kilogramm"),
        ("ml", "Milliliter", "Milliliter"),
        ("W", "Watt", "Watt"),
        ("V", "Volt", "Volt"),
        ("A", "Ampere", "Ampere"),
        ("m", "Meter", "Meter"),
        ("g", "Gramm", "Gramm"),
        ("l", "Liter", "Liter"),
        ("%", "Prozent", "Prozent"),
    ] {
        if s.starts_with(raw)
            && s[raw.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
        {
            return Some((raw.len(), singular, plural));
        }
    }
    None
}
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
fn space_len(s: &str) -> usize {
    s.len() - s.trim_start_matches(char::is_whitespace).len()
}
fn preceding_word(s: &str) -> String {
    s.trim_end_matches(|c: char| !word_char(c))
        .split(|c: char| !word_char(c))
        .next_back()
        .unwrap_or("")
        .to_lowercase()
}
fn technical_context(s: &str) -> bool {
    matches!(
        preceding_word(s).as_str(),
        "port"
            | "ports"
            | "tcp"
            | "udp"
            | "version"
            | "versionen"
            | "telefon"
            | "telefonnummer"
            | "rufnummer"
            | "tel"
            | "mobil"
            | "pin"
            | "plz"
            | "postleitzahl"
            | "id"
            | "kennung"
            | "artikelnummer"
            | "verhältnis"
            | "maßstab"
            | "kapitel"
            | "ergebnis"
            | "hostname"
            | "host"
            | "ip"
            | "adresse"
    )
}
/// Parse a complete German grouped token, never a numeric prefix.
/// IPv4-shaped values take precedence over grouping, including 1.234.234.234.
fn grouped_number(s: &str) -> Option<(u64, Option<&str>)> {
    if !s.contains('.') {
        return None;
    }
    let s = s.strip_prefix('-').unwrap_or(s);
    let (integer, fraction) = match s.split_once(',') {
        Some((integer, f)) if !f.is_empty() && f.bytes().all(|c| c.is_ascii_digit()) => {
            (integer, Some(f))
        }
        Some(_) => return None,
        None => (s, None),
    };
    let groups: Vec<&str> = integer.split('.').collect();
    if groups.len() < 2
        || !(1..=3).contains(&groups[0].len())
        || groups[0].starts_with('0')
        || !groups[0].bytes().all(|c| c.is_ascii_digit())
        || !groups[1..]
            .iter()
            .all(|g| g.len() == 3 && g.bytes().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    if groups.len() == 4 && groups.iter().all(|g| g.parse::<u8>().is_ok()) {
        return None;
    }
    let mut value = 0u64;
    for g in groups {
        value = value
            .checked_mul(1000)?
            .checked_add(g.parse::<u64>().ok()?)?;
    }
    Some((value, fraction))
}
fn numeric_span_end(s: &str, begin: usize) -> usize {
    let mut end = begin;
    while s.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    while matches!(s.as_bytes().get(end), Some(b'.' | b','))
        && s.as_bytes().get(end + 1).is_some_and(u8::is_ascii_digit)
    {
        end += 1;
        while s.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    end
}

const MONTHS: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];
#[derive(Debug, Clone, Copy)]
struct Date {
    len: usize,
    day: u8,
    month: u8,
    year: Option<u16>,
}
impl Date {
    fn valid(self) -> bool {
        if self.day == 0 || !(1..=12).contains(&self.month) || self.year == Some(0) {
            return false;
        }
        // Without a year, February 29 is a possible calendar date.
        let leap = self
            .year
            .is_none_or(|y| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0));
        let max = match self.month {
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        self.day <= max
    }
}
// Strict complete-token shape: D[D].M[M].YYYY, D[D].M[M].YY,
// D[D].M[M]. or YYYY-MM-DD. Return invalid candidates too so their
// numeric fragments remain untouched instead of being partially normalized.
fn date_at(s: &str) -> Option<Date> {
    let digits = |p: usize| {
        s.as_bytes()[p..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let first = digits(0);
    let (end, day, month, year) = if first == 4 && s.as_bytes().get(4) == Some(&b'-') {
        if s.len() < 10 || digits(5) != 2 || s.as_bytes().get(7) != Some(&b'-') || digits(8) != 2 {
            return None;
        }
        (
            10,
            s[8..10].parse().ok()?,
            s[5..7].parse().ok()?,
            Some(s[..4].parse().ok()?),
        )
    } else {
        if !(1..=2).contains(&first) || s.as_bytes().get(first) != Some(&b'.') {
            return None;
        }
        let m = first + 1;
        let m_len = digits(m);
        if !(1..=2).contains(&m_len) || s.as_bytes().get(m + m_len) != Some(&b'.') {
            return None;
        }
        let y = m + m_len + 1;
        let y_len = digits(y);
        let year = match y_len {
            0 => None,
            2 => Some(2000 + s[y..y + 2].parse::<u16>().ok()?),
            4 => Some(s[y..y + 4].parse().ok()?),
            _ => return None,
        };
        (
            y + y_len,
            s[..first].parse().ok()?,
            s[m..m + m_len].parse().ok()?,
            year,
        )
    };
    let after = &s[end..];
    if after
        .chars()
        .next()
        .is_some_and(|c| word_char(c) || matches!(c, ':' | '/' | '\\' | '-'))
        || matches!(after.as_bytes().first(), Some(b'.' | b','))
            && after.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
    {
        return None;
    }
    Some(Date {
        len: end,
        day,
        month,
        year,
    })
}
fn ordinal_day(day: u8, before: &str) -> String {
    let stem = match day {
        1 => "erst".into(),
        2 => "zweit".into(),
        3 => "dritt".into(),
        7 => "siebt".into(),
        8 => "acht".into(),
        n if n < 20 => format!("{}t", cardinal(n as u64)),
        n => format!("{}st", cardinal(n as u64)),
    };
    let ending = match preceding_word(before).as_str() {
        "am" | "vom" | "zum" => "en",
        "der" => "e",
        _ => "er",
    };
    format!("{stem}{ending}")
}

// Parse before cardinal numbers, but only outside protected technical spans.
// Invalid calendar candidates are returned too and preserved as a whole.
fn named_date_at(s: &str) -> Option<Date> {
    named_date_at_mode(s, false)
}
fn named_date_at_mode(s: &str, extended: bool) -> Option<Date> {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if !(1..=2).contains(&digits) {
        return None;
    }
    let point = usize::from(s.as_bytes().get(digits) == Some(&b'.'));
    let rest = &s[digits + point..];
    let trimmed = rest.trim_start_matches(char::is_whitespace);
    if trimmed.len() == rest.len() {
        return None;
    }
    let start = s.len() - trimmed.len();
    let name_len = trimmed
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or(trimmed.len());
    let name = &trimmed[..name_len];
    let aliases = [
        "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
    ];
    let month = MONTHS
        .iter()
        .position(|m| m.to_lowercase() == name.to_lowercase())
        .or_else(|| {
            if extended && trimmed[name_len..].starts_with('.') {
                aliases.iter().position(|a| *a == name)
            } else {
                None
            }
        })?;
    let mut end = start + name_len;
    if extended
        && !MONTHS
            .iter()
            .any(|m| m.to_lowercase() == name.to_lowercase())
        && s[end..].starts_with('.')
    {
        end += 1;
    }
    let tail = &s[end..];
    let year_tail = tail.trim_start_matches(char::is_whitespace);
    let year_digits = year_tail.bytes().take_while(u8::is_ascii_digit).count();
    let year = if tail.len() != year_tail.len() && year_digits > 0 {
        if year_digits != 4 {
            return None;
        }
        end = s.len() - year_tail.len() + year_digits;
        Some(year_tail[..year_digits].parse().ok()?)
    } else {
        None
    };
    if s[end..]
        .chars()
        .next()
        .is_some_and(|c| word_char(c) || matches!(c, '/' | '\\' | ':' | '-'))
    {
        return None;
    }
    Some(Date {
        len: end,
        day: s[..digits].parse().ok()?,
        month: month as u8 + 1,
        year,
    })
}

/// Protected byte ranges. Phone-like multi-token spans are protected before numeric parsing.
fn protected(text: &str) -> Vec<(usize, usize)> {
    protected_mode(text, false)
}
fn protected_mode(text: &str, extended: bool) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    for token in text.split_inclusive(char::is_whitespace) {
        let raw = token.trim();
        let start = offset + token.len() - token.trim_start().len();
        let clean = raw.trim_matches(|c: char| {
            matches!(c, '"' | '\'' | '(' | ')' | ',' | ';' | '!' | '?' | '.')
        });
        let digits = clean.chars().filter(|c| c.is_ascii_digit()).count();
        let suffix_unit = clean
            .find(|c: char| c.is_alphabetic() || c == '°' || c == '%')
            .is_some_and(|p| {
                clean[..p]
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '-'))
                    && unit_at(&clean[p..]).is_some_and(|(n, _, _)| n == clean.len() - p)
            });
        let is_time = clean.split_once(':').is_some_and(|(h, m)| {
            h.len() <= 2
                && m.len() == 2
                && h.bytes().all(|b| b.is_ascii_digit())
                && m.bytes().all(|b| b.is_ascii_digit())
        });
        let alpha_digits =
            digits > 0 && clean.chars().any(|c| c.is_alphabetic() || c == '_') && !suffix_unit;
        let path = clean.contains('/') && !suffix_unit || clean.contains('\\');
        let dots = clean.matches('.').count();
        let number_body = if suffix_unit {
            &clean[..clean
                .find(|c: char| c.is_alphabetic() || c == '°' || c == '%')
                .unwrap()]
        } else {
            clean
        };
        let grouped = grouped_number(if extended {
            number_body.trim_matches(['€', '$', '£', '¥'])
        } else {
            number_body
        })
        .is_some();
        let date_token = raw.trim_start_matches(['\"', '\'', '(']);
        let date = date_at(date_token).is_some_and(|d| {
            date_token[d.len..]
                .chars()
                .all(|c| matches!(c, '.' | ',' | ';' | '!' | '?' | ')' | '\"' | '\''))
        });
        let phone = clean.starts_with('+')
            || clean.starts_with("00") && digits >= 7
            || clean.trim_start_matches('-').contains('-')
            || digits >= 7 && clean.contains('-');
        if clean.contains("://")
            || alpha_digits
            || path
            || dots >= 2 && !grouped && !date
            || dots > 0 && number_body.contains(',') && !grouped
            || clean.contains(':') && !is_time
            || phone && !date
            || technical_context(&text[..start])
        {
            ranges.push((start, start + raw.len()));
        }
        offset += token.len();
    }
    // A phone cue protects a complete numeric/separator run, including spaced country/area codes.
    for (i, c) in text.char_indices() {
        if (c.is_ascii_digit() || c == '+' || c == '(')
            && text[..i]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_ascii_digit())
        {
            let before = &text[..i];
            let cue = technical_context(before)
                && matches!(
                    preceding_word(before).as_str(),
                    "telefon" | "telefonnummer" | "rufnummer" | "tel" | "mobil"
                );
            let signed_phone = c == '+' || c == '(';
            if cue || signed_phone {
                let n = text[i..]
                    .chars()
                    .take_while(|c| {
                        c.is_ascii_digit()
                            || c.is_whitespace()
                            || matches!(c, '+' | '-' | '(' | ')' | '/')
                    })
                    .map(char::len_utf8)
                    .sum::<usize>();
                if cue || text[i..i + n].bytes().filter(u8::is_ascii_digit).count() >= 7 {
                    ranges.push((i, i + n));
                }
            }
        }
    }
    // Spaced domestic phone sequences (leading zero, >=7 digits, >=2 groups).
    let mut run_start = None;
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), '\0')))
    {
        if c.is_ascii_digit() || c.is_whitespace() || matches!(c, '(' | ')' | '+' | '-') {
            if run_start.is_none() && c.is_ascii_digit() {
                run_start = Some(i);
            }
        } else if let Some(start) = run_start.take() {
            let s = &text[start..i];
            if s.starts_with('0')
                && s.split_whitespace()
                    .next()
                    .is_some_and(|first| (2..=6).contains(&first.len()))
                && s.bytes().filter(u8::is_ascii_digit).count() >= 7
                && s.split_whitespace().count() >= 2
                && !s.contains('\n')
            {
                ranges.push((start, i));
            }
        }
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut().filter(|last| start <= last.1) {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    merged
}

/// Committed spans refer exclusively to UTF-8 byte ranges in the original text.
/// Priority: protected ranges > date > time > quantity/percent > formatted number > cardinal.
/// The production candidate additionally recognizes currency and abbreviation spans.
#[derive(Debug)]
struct Span<'a> {
    range: std::ops::Range<usize>,
    kind: SpanKind<'a>,
}
#[derive(Debug)]
enum SpanKind<'a> {
    Protected,
    Date(Date),
    Time { hour: u64, minute: u64 },
    Quantity(Number<'a>),
    Percent(Number<'a>),
    GroupedNumber(Number<'a>),
    DecimalNumber(Number<'a>),
    Cardinal(Number<'a>),
    Extension(String),
}
#[derive(Debug)]
struct Number<'a> {
    value: u64,
    negative: bool,
    fraction: Option<&'a str>,
    unit: Option<(usize, &'static str, &'static str)>,
}
impl Span<'_> {
    fn render(&self, original: &str, output: &mut String) {
        match &self.kind {
            SpanKind::Protected => output.push_str(&original[self.range.clone()]),
            SpanKind::Extension(value) => output.push_str(value),
            SpanKind::Date(date) => {
                output.push_str(&ordinal_day(date.day, &original[..self.range.start]));
                output.push(' ');
                output.push_str(MONTHS[date.month as usize - 1]);
                if let Some(year) = date.year {
                    output.push(' ');
                    output.push_str(&cardinal(year as u64));
                }
            }
            SpanKind::Time { hour, minute } => {
                output.push_str(&format!(
                    "{} Uhr",
                    if *hour == 1 {
                        "ein".into()
                    } else {
                        cardinal(*hour)
                    }
                ));
                if *minute != 0 {
                    output.push(' ');
                    output.push_str(&cardinal(*minute));
                }
            }
            SpanKind::Quantity(n)
            | SpanKind::Percent(n)
            | SpanKind::GroupedNumber(n)
            | SpanKind::DecimalNumber(n)
            | SpanKind::Cardinal(n) => n.render(output),
        }
    }
}
impl Number<'_> {
    fn render(&self, output: &mut String) {
        if self.negative {
            output.push_str("minus ");
        }
        let one_unit = self.value == 1 && self.fraction.is_none() && self.unit.is_some();
        if one_unit {
            output.push_str(
                if self.unit.is_some_and(|(_, singular, _)| {
                    matches!(singular, "Wattstunde" | "Kilowattstunde")
                }) {
                    "eine"
                } else {
                    "ein"
                },
            );
        } else {
            output.push_str(&cardinal(self.value));
        }
        if let Some(f) = self.fraction {
            output.push_str(" Komma");
            for d in f.bytes() {
                output.push(' ');
                output.push_str(SMALL[(d - b'0') as usize]);
            }
        }
        if let Some((_, singular, plural)) = self.unit {
            output.push(' ');
            output.push_str(if self.value == 1 && self.fraction.is_none() {
                singular
            } else {
                plural
            });
        }
    }
}
fn date_span(text: &str, start: usize, limit: usize, extended: bool) -> Option<Span<'_>> {
    let date = if extended {
        named_date_at_mode(&text[start..limit], true)
    } else {
        named_date_at(&text[start..limit])
    }
    .or_else(|| date_at(&text[start..limit]))?;
    // A protected year-like token must not turn an incomplete candidate into
    // a yearless date (e.g. February 29 before a protected "2025:" token).
    if date.year.is_none()
        && text[start + date.len..limit]
            .chars()
            .all(char::is_whitespace)
        && text[limit..].starts_with(|c: char| c.is_ascii_digit())
    {
        return Some(Span {
            range: start..limit,
            kind: SpanKind::Protected,
        });
    }
    Some(Span {
        range: start..start + date.len,
        kind: if date.valid() {
            SpanKind::Date(date)
        } else {
            SpanKind::Protected
        },
    })
}
fn time_span(text: &str, start: usize, limit: usize) -> Option<Span<'_>> {
    let s = &text[start..limit];
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if s.as_bytes().get(digits) != Some(&b':') {
        return None;
    }
    let mins = digits + 1;
    let end = mins
        + s.as_bytes()[mins..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
    let hour = s[..digits].parse::<u8>().ok();
    let minute = s[mins..end].parse::<u8>().ok();
    if digits <= 2
        && end - mins == 2
        && hour.is_some_and(|h| h <= 23)
        && minute.is_some_and(|m| m <= 59)
        && s[end..]
            .chars()
            .next()
            .is_none_or(|c| !word_char(c) && c != ':')
    {
        let ws = space_len(&s[end..]);
        let mut consumed = end;
        if ws > 0
            && s[end + ws..].starts_with("Uhr")
            && s[end + ws + 3..]
                .chars()
                .next()
                .is_none_or(|c| !word_char(c))
        {
            consumed += ws + 3;
        }
        return Some(Span {
            range: start..start + consumed,
            kind: SpanKind::Time {
                hour: hour.unwrap() as u64,
                minute: minute.unwrap() as u64,
            },
        });
    }
    let consumed = s.find(char::is_whitespace).unwrap_or(s.len());
    Some(Span {
        range: start..start + consumed,
        kind: SpanKind::Protected,
    })
}
fn number_span(text: &str, start: usize, limit: usize) -> Span<'_> {
    let s = &text[start..limit];
    let negative = s.starts_with('-');
    let begin = usize::from(negative);
    let integer_end = begin
        + s.as_bytes()[begin..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
    let mut end = integer_end;
    let group_end = numeric_span_end(s, begin);
    let grouped = grouped_number(&s[begin..group_end]);
    let mut fraction = None;
    let separator = s.as_bytes().get(end).copied();
    if matches!(separator, Some(b',' | b'.'))
        && s.as_bytes().get(end + 1).is_some_and(u8::is_ascii_digit)
    {
        end += 1;
        let f = end;
        while s.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        fraction = Some(&s[f..end]);
    }
    if let Some((_, f)) = grouped {
        end = group_end;
        fraction = f;
    }
    let ws = space_len(&s[end..]);
    let unit = unit_at(&s[end + ws..]);
    let after = s[end..].chars().next();
    let leading_zero = integer_end - begin > 1 && s.as_bytes()[begin] == b'0';
    let unsupported = after.is_some_and(|c| {
        word_char(c) && unit.is_none() || matches!(c, ':' | '/' | '\\') && unit.is_none()
    }) || grouped.is_none()
        && separator == Some(b'.')
        && fraction.is_some_and(|f| unit.is_none() || f.len() >= 3)
        || leading_zero
        || text.trim() == &text[start..start + end]
            && integer_end - begin == 5
            && s[begin..integer_end]
                .parse::<u64>()
                .is_ok_and(|n| n <= 65535);
    let value = grouped
        .map(|(n, _)| Ok(n))
        .unwrap_or_else(|| s[begin..integer_end].parse::<u64>());
    if unsupported || value.is_err() {
        return Span {
            range: start..start + end,
            kind: SpanKind::Protected,
        };
    }
    let number = Number {
        value: value.unwrap(),
        negative,
        fraction,
        unit,
    };
    // A quantity/percent consumes the unit together with the complete numeric token.
    // No separate cardinal match can consume a prefix of this committed span.
    let kind = if let Some((length, singular, _)) = unit {
        end += ws + length;
        if singular == "Prozent" {
            SpanKind::Percent(number)
        } else {
            SpanKind::Quantity(number)
        }
    } else if grouped.is_some() {
        SpanKind::GroupedNumber(number)
    } else if fraction.is_some() {
        SpanKind::DecimalNumber(number)
    } else {
        SpanKind::Cardinal(number)
    };
    Span {
        range: start..start + end,
        kind,
    }
}
/// Explicit first-match priority. All recognizers see the original text only.
fn span_at_mode(text: &str, start: usize, limit: usize, extended: bool) -> Option<Span<'_>> {
    let c = text[start..limit].chars().next()?;
    let boundary = text[..start]
        .chars()
        .next_back()
        .is_none_or(|c| !word_char(c) && !matches!(c, '.' | ':' | '/' | '\\' | '+' | '-'));
    if !boundary {
        return None;
    }
    if c.is_ascii_digit() {
        if let Some(span) = date_span(text, start, limit, extended) {
            return Some(span);
        }
        if let Some(span) = time_span(text, start, limit) {
            return Some(span);
        }
    }
    if extended {
        if let Some((len, value)) =
            crate::normalizer_extensions::abbreviation_at(text, start, limit)
                .or_else(|| crate::normalizer_extensions::currency_at(text, start, limit))
        {
            return Some(Span {
                range: start..start + len,
                kind: SpanKind::Extension(value),
            });
        }
    }
    if !c.is_ascii_digit()
        && !(c == '-' && text[start + 1..limit].starts_with(|c: char| c.is_ascii_digit()))
    {
        return None;
    }
    Some(number_span(text, start, limit))
}

#[cfg(test)]
fn span_at(text: &str, start: usize, limit: usize) -> Option<Span<'_>> {
    span_at_mode(text, start, limit, false)
}
pub fn normalize(text: &str) -> String {
    normalize_mode(text, false, &[])
}
/// Production candidate: original span rules plus protected currency/abbreviation spans.
pub fn normalize_final(text: &str) -> String {
    normalize_mode(text, true, &[])
}
pub fn normalize_safe(text: &str) -> String {
    normalize_with_rules(text, crate::safe_rules::ACCEPTED)
}
pub fn normalize_with_rules(text: &str, rules: &[crate::safe_rules::Rule]) -> String {
    normalize_mode(text, false, rules)
}
fn normalize_mode(text: &str, extended: bool, rules: &[crate::safe_rules::Rule]) -> String {
    let ranges = if extended {
        protected_mode(text, true)
    } else {
        protected(text)
    };
    let mut output = String::with_capacity(text.len());
    let mut i = 0;
    let mut range_index = 0;
    while i < text.len() {
        while range_index < ranges.len() && ranges[range_index].1 <= i {
            range_index += 1;
        }
        if let Some((_, end)) = ranges
            .get(range_index)
            .filter(|(start, end)| *start <= i && i < *end)
        {
            Span {
                range: i..*end,
                kind: SpanKind::Protected,
            }
            .render(text, &mut output);
            i = *end;
            continue;
        }
        // A recognizer cannot consume even part of the next protected range.
        let limit = ranges
            .get(range_index)
            .map_or(text.len(), |(start, _)| *start);
        let safe_span = if !rules.is_empty()
            && text[..i]
                .chars()
                .next_back()
                .is_none_or(|c| !word_char(c) && !matches!(c, '.' | ':' | '/' | '\\' | '+' | '-'))
        {
            rules.iter().find_map(|&rule| {
                if rule == crate::safe_rules::Rule::Month {
                    return if text[i..limit].starts_with(|c: char| c.is_ascii_digit())
                        && crate::safe_rules::safe_context(&text[..i])
                    {
                        date_span(text, i, limit, true)
                    } else {
                        None
                    };
                }
                crate::safe_rules::at(text, i, limit, rule).map(|(len, value)| Span {
                    range: i..i + len,
                    kind: SpanKind::Extension(value),
                })
            })
        } else {
            None
        };
        // Structured baseline date/time always outranks all additions.
        let base = span_at_mode(text, i, limit, extended);
        let recognized = match base {
            Some(ref span)
                if matches!(
                    span.kind,
                    SpanKind::Date(_) | SpanKind::Time { .. } | SpanKind::Protected
                ) =>
            {
                base
            }
            _ => safe_span.or(base),
        };
        if let Some(span) = recognized {
            debug_assert!(span.range.start == i && span.range.end > i && span.range.end <= limit);
            span.render(text, &mut output);
            i = span.range.end;
        } else {
            let c = text[i..].chars().next().unwrap();
            output.push(c);
            i += c.len_utf8();
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cardinal_reference() {
        for (n, s) in [
            (0, "null"),
            (1, "eins"),
            (2, "zwei"),
            (7, "sieben"),
            (10, "zehn"),
            (11, "elf"),
            (12, "zwölf"),
            (16, "sechzehn"),
            (17, "siebzehn"),
            (20, "zwanzig"),
            (21, "einundzwanzig"),
            (25, "fünfundzwanzig"),
            (42, "zweiundvierzig"),
            (99, "neunundneunzig"),
            (100, "einhundert"),
            (101, "einhunderteins"),
            (125, "einhundertfünfundzwanzig"),
            (999, "neunhundertneunundneunzig"),
            (1000, "eintausend"),
            (2026, "zweitausendsechsundzwanzig"),
            (1_000_000, "eine Million"),
            (2_000_001, "zwei Millionen eins"),
            (1_000_000_000, "eine Milliarde"),
            (
                u64::MAX,
                "achtzehn Trillionen vierhundertsechsundvierzig Billiarden siebenhundertvierundvierzig Billionen dreiundsiebzig Milliarden siebenhundertneun Millionen fünfhunderteinundfünfzigtausendsechshundertfünfzehn",
            ),
        ] {
            assert_eq!(cardinal(n), s, "{n}");
        }
    }
    #[test]
    fn complete_sentences() {
        for (input, expected) in [
            (
                "Im Wohnzimmer sind es 21,5 °C.",
                "Im Wohnzimmer sind es einundzwanzig Komma fünf Grad Celsius.",
            ),
            (
                "Die Luftfeuchtigkeit beträgt 62 %.",
                "Die Luftfeuchtigkeit beträgt zweiundsechzig Prozent.",
            ),
            (
                "Der aktuelle Verbrauch beträgt 425 W.",
                "Der aktuelle Verbrauch beträgt vierhundertfünfundzwanzig Watt.",
            ),
            (
                "Die Solaranlage erzeugt 2,4 kW.",
                "Die Solaranlage erzeugt zwei Komma vier Kilowatt.",
            ),
            (
                "Um 18:30 Uhr wird das Licht eingeschaltet.",
                "Um achtzehn Uhr dreißig wird das Licht eingeschaltet.",
            ),
            (
                "Die Außentemperatur beträgt -3,5 °C und die Luftfeuchtigkeit 87 %.",
                "Die Außentemperatur beträgt minus drei Komma fünf Grad Celsius und die Luftfeuchtigkeit siebenundachtzig Prozent.",
            ),
            (
                "Der Stromverbrauch heute beträgt 12,7 kWh.",
                "Der Stromverbrauch heute beträgt zwölf Komma sieben Kilowattstunden.",
            ),
            (
                "Die Entfernung beträgt 125 m.",
                "Die Entfernung beträgt einhundertfünfundzwanzig Meter.",
            ),
            (
                "Es sind 0, 1, 2 und 42!",
                "Es sind null, eins, zwei und zweiundvierzig!",
            ),
            (
                "Werte: -1, -5 und -12,5.",
                "Werte: minus eins, minus fünf und minus zwölf Komma fünf.",
            ),
            (
                "Werte: 2,5; 19,5; 21,7; 0,5; 3,14.",
                "Werte: zwei Komma fünf; neunzehn Komma fünf; einundzwanzig Komma sieben; null Komma fünf; drei Komma eins vier.",
            ),
            (
                "1 °C, 2°C, 21 Grad, 21 Grad Celsius.",
                "ein Grad Celsius, zwei Grad Celsius, einundzwanzig Grad, einundzwanzig Grad Celsius.",
            ),
            (
                "1 %; 25%; 99,5 %.",
                "ein Prozent; fünfundzwanzig Prozent; neunundneunzig Komma fünf Prozent.",
            ),
            (
                "08:00, 08:30, 14:05, 21:45, 8:00, 8:30.",
                "acht Uhr, acht Uhr dreißig, vierzehn Uhr fünf, einundzwanzig Uhr fünfundvierzig, acht Uhr, acht Uhr dreißig.",
            ),
            (
                "1 Wh und 2 Wh; 1 kWh und 2 kWh.",
                "eine Wattstunde und zwei Wattstunden; eine Kilowattstunde und zwei Kilowattstunden.",
            ),
            ("1 Meter und 2 Meter.", "ein Meter und zwei Meter."),
            (
                "21.5 °C und 2.4kW.",
                "einundzwanzig Komma fünf Grad Celsius und zwei Komma vier Kilowatt.",
            ),
            (
                "Heute wurden 12,7 Kilowattstunden verbraucht.",
                "Heute wurden zwölf Komma sieben Kilowattstunden verbraucht.",
            ),
        ] {
            assert_eq!(normalize(input), expected, "{input}");
            assert_eq!(normalize(expected), expected, "idempotence: {expected}");
        }
    }
    #[test]
    fn unit_spellings() {
        for (raw, spoken) in [
            ("W", "Watt"),
            ("kW", "Kilowatt"),
            ("Wh", "Wattstunden"),
            ("kWh", "Kilowattstunden"),
            ("V", "Volt"),
            ("A", "Ampere"),
            ("mA", "Milliampere"),
            ("Hz", "Hertz"),
            ("kHz", "Kilohertz"),
            ("m", "Meter"),
            ("cm", "Zentimeter"),
            ("mm", "Millimeter"),
            ("km", "Kilometer"),
            ("g", "Gramm"),
            ("kg", "Kilogramm"),
            ("ml", "Milliliter"),
            ("l", "Liter"),
            ("Pa", "Pascal"),
            ("hPa", "Hektopascal"),
            ("km/h", "Kilometer pro Stunde"),
            ("m/s", "Meter pro Sekunde"),
        ] {
            for sep in ["", " ", "\u{a0}"] {
                assert_eq!(
                    normalize(&format!("Es sind 2{sep}{raw}.")),
                    format!("Es sind zwei {spoken}.")
                );
            }
        }
    }
    #[test]
    fn technical_tokens_unchanged() {
        for s in [
            "192.0.2.100",
            "192.0.2.100:10204",
            "Port 10204",
            "10204",
            "TCP 8080",
            "Version 1.2.3",
            "1.2.3",
            "https://example.org:8080/a/42?q=1",
            "/app/models/german_24l",
            "./model-2.safetensors",
            "C:\\model2\\voice1",
            "Q8",
            "Q8_0",
            "german_24l",
            "abc123",
            "abc-123",
            "v1.2",
            "foo@example2.org",
            "+49 30 12345678",
            "Telefon 030 12345678",
            "00493012345678",
            "030-12345678",
            "01234",
            "12:345",
            "24:00",
            "08:30:12",
            "2.5",
            "18446744073709551616",
        ] {
            assert_eq!(normalize(s), s, "{s}");
        }
        assert_eq!(
            normalize("Port 10204, es sind 21 °C."),
            "Port 10204, es sind einundzwanzig Grad Celsius."
        );
    }
}

#[cfg(test)]
mod regressions {
    use super::*;
    macro_rules! cases {
        ($($name:ident: $input:expr => $output:expr;)*) => { $(#[test] fn $name() { assert_eq!(normalize($input),$output); assert_eq!(normalize($output),$output); assert_eq!(super::normalize_final($input),$output); assert_eq!(super::normalize_final($output),$output); })* };
    }
    cases! {
        short_cardinals: "0 1 2 7" => "null eins zwei sieben";
        teen_cardinals: "10 11 12 16 17" => "zehn elf zwölf sechzehn siebzehn";
        tens: "20 21 25 42 99" => "zwanzig einundzwanzig fünfundzwanzig zweiundvierzig neunundneunzig";
        hundreds: "100 101 125 999" => "einhundert einhunderteins einhundertfünfundzwanzig neunhundertneunundneunzig";
        thousands: "1000 und 2026" => "eintausend und zweitausendsechsundzwanzig";
        millions: "Wir zählen 1234567 Menschen." => "Wir zählen eine Million zweihundertvierunddreißigtausendfünfhundertsiebenundsechzig Menschen.";
        decimals: "3,140 und 0,05." => "drei Komma eins vier null und null Komma null fünf.";
        negatives: "-1 und -5 und -12,5." => "minus eins und minus fünf und minus zwölf Komma fünf.";
        literal_minus: "minus 3,5 Grad Celsius" => "minus drei Komma fünf Grad Celsius";
        temperatures: "1°C, 2 °C, 21°C." => "ein Grad Celsius, zwei Grad Celsius, einundzwanzig Grad Celsius.";
        temperature_no_duplicate: "21 Grad Celsius." => "einundzwanzig Grad Celsius.";
        percent_singular: "1%, 25 %, 99,5%." => "ein Prozent, fünfundzwanzig Prozent, neunundneunzig Komma fünf Prozent.";
        clock_zero: "08:00 und 8:00 Uhr." => "acht Uhr und acht Uhr.";
        clock_minutes: "08:30, 14:05, 21:45." => "acht Uhr dreißig, vierzehn Uhr fünf, einundzwanzig Uhr fünfundvierzig.";
        clock_one: "01:00 Uhr" => "ein Uhr";
        energy_singular: "1 Wh und 1 kWh." => "eine Wattstunde und eine Kilowattstunde.";
        energy_plural: "2 Wh und 2 kWh." => "zwei Wattstunden und zwei Kilowattstunden.";
        length_singular: "1 m und 1 Meter." => "ein Meter und ein Meter.";
        volume_singular: "1 l und 1 Liter." => "ein Liter und ein Liter.";
        dot_unit: "2.4 kW und 21.5°C." => "zwei Komma vier Kilowatt und einundzwanzig Komma fünf Grad Celsius.";
        grouped_thousands_unit: "1.234 W und 21.500°C." => "eintausendzweihundertvierunddreißig Watt und einundzwanzigtausendfünfhundert Grad Celsius.";
        ambiguous_dot: "2.4 und 1.234." => "2.4 und eintausendzweihundertvierunddreißig.";
        punctuation: "(42), 7! 2? 3; 4." => "(zweiundvierzig), sieben! zwei? drei; vier.";
        utf8: "Äußere Werte: 21,5\u{a0}°C." => "Äußere Werte: einundzwanzig Komma fünf Grad Celsius.";
        ipv4: "Adresse 192.0.2.100:10204, Temperatur 21 °C." => "Adresse 192.0.2.100:10204, Temperatur einundzwanzig Grad Celsius.";
        port_colon: "Port: 10204, 2 Lampen." => "Port: 10204, zwei Lampen.";
        version: "Version 1.2.3 und Q8_0, german_24l." => "Version 1.2.3 und Q8_0, german_24l.";
        url: "https://example.org:8080/42?value=1 und 2 Lampen." => "https://example.org:8080/42?value=1 und zwei Lampen.";
        paths: "/app/voices/voice2.safetensors und ./foo42.wav" => "/app/voices/voice2.safetensors und ./foo42.wav";
        ids: "ID: 12345, abc123 und Q8." => "ID: 12345, abc123 und Q8.";
        phone: "Telefon: +49 30 12345678 und 2 Lampen." => "Telefon: +49 30 12345678 und zwei Lampen.";
        phone_local: "Telefonnummer: 030 12345678." => "Telefonnummer: 030 12345678.";
        phone_two_groups: "Nummer 030 12345678, es sind 2 Lampen." => "Nummer 030 12345678, es sind zwei Lampen.";
        zero_number_list: "Wert 0 100 200." => "Wert null einhundert zweihundert.";
        phone_groups: "Kontakt: 030 123 4567." => "Kontakt: 030 123 4567.";
        invalid_clock: "24:00 08:60 08:30:12" => "24:00 08:60 08:30:12";
        ratio: "Verhältnis 1:20 und Maßstab 1:50" => "Verhältnis 1:20 und Maßstab 1:50";
        ranges_and_dates: "5-7, 2026-10-07, 01.02.2026" => "5-7, siebter Oktober zweitausendsechsundzwanzig, erster Februar zweitausendsechsundzwanzig";
        no_digits: "Ein warmer Tag ohne Zahlen." => "Ein warmer Tag ohne Zahlen.";
        overflow: "18446744073709551616" => "18446744073709551616";
        empty: "" => "";
        decimal_trailing_zero: "1,00 kWh" => "eins Komma null null Kilowattstunden";
    }
    #[test]
    fn all_cardinals_to_100000_are_idempotent() {
        for n in 0..100000 {
            let text = format!("Wert {n}.");
            let result = normalize(&text);
            assert_eq!(result, format!("Wert {}.", cardinal(n)));
            assert_eq!(normalize(&result), result);
        }
    }
    #[test]
    fn bounded_request_and_unicode_do_not_panic() {
        for s in [
            "1".repeat(16384),
            "21°C, ".repeat(1500),
            "Äß😀\n".repeat(1500),
        ] {
            let normalized = normalize(&s);
            assert_eq!(normalize(&normalized), normalized);
        }
    }
}

#[cfg(test)]
mod thousands_regressions {
    use super::*;
    #[test]
    fn all_requested_grouped_cardinals() {
        for (input, n) in [
            ("999", 999),
            ("1.000", 1000),
            ("1.001", 1001),
            ("1.250", 1250),
            ("1.234", 1234),
            ("12.345", 12345),
            ("9.999", 9999),
            ("10.000", 10000),
            ("12.500", 12500),
            ("99.999", 99999),
            ("100.000", 100000),
            ("999.999", 999999),
            ("1.000.000", 1_000_000),
            ("1.234.567", 1_234_567),
            ("12.345.678", 12_345_678),
            ("123.456", 123456),
        ] {
            assert_eq!(normalize(input), cardinal(n), "{input}");
            assert_eq!(
                normalize(&format!("Wert {input}.")),
                format!("Wert {}.", cardinal(n))
            );
        }
    }
    #[test]
    fn grouped_decimals_are_whole_tokens() {
        for (input, expected) in [
            ("1.234,5", "eintausendzweihundertvierunddreißig Komma fünf"),
            (
                "1.234,56",
                "eintausendzweihundertvierunddreißig Komma fünf sechs",
            ),
            (
                "12.345,67",
                "zwölftausenddreihundertfünfundvierzig Komma sechs sieben",
            ),
            (
                "2.450,5 kWh",
                "zweitausendvierhundertfünfzig Komma fünf Kilowattstunden",
            ),
            (
                "-2.450,50°C",
                "minus zweitausendvierhundertfünfzig Komma fünf null Grad Celsius",
            ),
            ("1.250 W", "eintausendzweihundertfünfzig Watt"),
            ("12.500 W", "zwölftausendfünfhundert Watt"),
            ("0,5", "null Komma fünf"),
            ("10,5", "zehn Komma fünf"),
        ] {
            assert_eq!(normalize(input), expected);
            assert_eq!(normalize(expected), expected);
        }
    }
    #[test]
    fn technical_values_keep_priority() {
        for s in [
            "192.0.2.100",
            "127.0.0.1",
            "1.234.234.234",
            "1.2.3",
            "2.4.1",
            "v1.2.3",
            "Version 1.234.567",
            "Hostname 1.234.567",
            "https://example.org/1.234.567",
            "/app/models/1.234.567",
            "./1.234.567",
            "host1.234.example",
            "german_24l",
            "Q8_0",
            "01.234",
            "1234.567",
            "1.23.456",
            "1.234,5,6",
            "18.446.744.073.709.551.616",
        ] {
            assert_eq!(normalize(s), s, "{s}");
        }
        assert_eq!(
            normalize("Der Server läuft unter 192.0.2.100:10204."),
            "Der Server läuft unter 192.0.2.100:10204."
        );
        assert_eq!(
            normalize("Version 1.2.3 ist installiert."),
            "Version 1.2.3 ist installiert."
        );
        assert_eq!(
            normalize("Der Verbrauch beträgt 1.250 Watt."),
            "Der Verbrauch beträgt eintausendzweihundertfünfzig Watt."
        );
        assert_eq!(
            normalize("Im Jahr 2026 beträgt der Verbrauch 2.450,5 Kilowattstunden."),
            "Im Jahr zweitausendsechsundzwanzig beträgt der Verbrauch zweitausendvierhundertfünfzig Komma fünf Kilowattstunden."
        );
    }
    #[test]
    fn checked_full_u64_grouping() {
        assert_eq!(normalize("18.446.744.073.709.551.615"), cardinal(u64::MAX));
        assert_eq!(grouped_number("1.23"), None);
        assert_eq!(grouped_number("1.234,56"), Some((1234, Some("56"))));
    }
}

#[cfg(test)]
mod date_regressions {
    use super::*;
    macro_rules! cases {
        ($($name:ident: $input:expr => $output:expr;)*) => { $(#[test] fn $name() { assert_eq!(normalize($input),$output); assert_eq!(normalize($output),$output); assert_eq!(super::normalize_final($input),$output); assert_eq!(super::normalize_final($output),$output); })* };
    }
    cases! {
        padded_date: "07.10.2026" => "siebter Oktober zweitausendsechsundzwanzig";
        unpadded_date: "7.10.2026" => "siebter Oktober zweitausendsechsundzwanzig";
        short_year: "07.10.26" => "siebter Oktober zweitausendsechsundzwanzig";
        short_year_zero: "1.1.00" => "erster Januar zweitausend";
        no_year: "07.10. und 7.10." => "siebter Oktober und siebter Oktober";
        iso: "2026-10-07" => "siebter Oktober zweitausendsechsundzwanzig";
        am: "am 07.10.2026" => "am siebten Oktober zweitausendsechsundzwanzig";
        am_uppercase: "Am 03.10.2026 ist Feiertag." => "Am dritten Oktober zweitausendsechsundzwanzig ist Feiertag.";
        am_twenty_one: "Der Termin ist am 21.10.2026." => "Der Termin ist am einundzwanzigsten Oktober zweitausendsechsundzwanzig.";
        vom: "vom 07.10.2026" => "vom siebten Oktober zweitausendsechsundzwanzig";
        zum: "zum 08.10.2026" => "zum achten Oktober zweitausendsechsundzwanzig";
        range: "vom 07.10.2026 bis zum 09.10.2026" => "vom siebten Oktober zweitausendsechsundzwanzig bis zum neunten Oktober zweitausendsechsundzwanzig";
        am_no_year: "am 7.10." => "am siebten Oktober";
        christmas_no_year: "Termin am 24.12." => "Termin am vierundzwanzigsten Dezember";
        der_today: "Heute ist der 07.10.2026." => "Heute ist der siebte Oktober zweitausendsechsundzwanzig.";
        date_and_clock: "Der nächste Termin ist am 12.10.2026 um 14:30 Uhr." => "Der nächste Termin ist am zwölften Oktober zweitausendsechsundzwanzig um vierzehn Uhr dreißig.";
        range_and_cardinal: "Vom 07.10.2026 bis zum 09.10.2026 sind es 3 Tage." => "Vom siebten Oktober zweitausendsechsundzwanzig bis zum neunten Oktober zweitausendsechsundzwanzig sind es drei Tage.";
        date_clock_temperature: "Am 24.12.2026 um 18:30 Uhr sind draußen 2,5 Grad Celsius." => "Am vierundzwanzigsten Dezember zweitausendsechsundzwanzig um achtzehn Uhr dreißig sind draußen zwei Komma fünf Grad Celsius.";
        date_energy: "Der Energieverbrauch am 07.10.2026 beträgt 12,5 kWh." => "Der Energieverbrauch am siebten Oktober zweitausendsechsundzwanzig beträgt zwölf Komma fünf Kilowattstunden.";
        requested_range: "Der Termin läuft vom 07.10.2026 bis zum 09.10.2026." => "Der Termin läuft vom siebten Oktober zweitausendsechsundzwanzig bis zum neunten Oktober zweitausendsechsundzwanzig.";
        punctuation: "(07.10.2026), 03.10.2025!" => "(siebter Oktober zweitausendsechsundzwanzig), dritter Oktober zweitausendfünfundzwanzig!";
        unicode_space: "AM\u{a0}07.10.2026" => "AM\u{a0}siebten Oktober zweitausendsechsundzwanzig";
        iso_context: "Am 2026-10-07 ist ein Termin." => "Am siebten Oktober zweitausendsechsundzwanzig ist ein Termin.";
        version_protected: "Version 1.2.3 ist installiert." => "Version 1.2.3 ist installiert.";
        ip_protected: "Der Server läuft unter 192.0.2.100:10204." => "Der Server läuft unter 192.0.2.100:10204.";
    }
    #[test]
    fn all_month_names() {
        for (month, name) in MONTHS.iter().enumerate() {
            assert_eq!(
                normalize(&format!("01.{:02}.2026", month + 1)),
                format!("erster {name} zweitausendsechsundzwanzig")
            );
        }
    }
    #[test]
    fn all_day_ordinals() {
        let days = [
            "erst",
            "zweit",
            "dritt",
            "viert",
            "fünft",
            "sechst",
            "siebt",
            "acht",
            "neunt",
            "zehnt",
            "elft",
            "zwölft",
            "dreizehnt",
            "vierzehnt",
            "fünfzehnt",
            "sechzehnt",
            "siebzehnt",
            "achtzehnt",
            "neunzehnt",
            "zwanzigst",
            "einundzwanzigst",
            "zweiundzwanzigst",
            "dreiundzwanzigst",
            "vierundzwanzigst",
            "fünfundzwanzigst",
            "sechsundzwanzigst",
            "siebenundzwanzigst",
            "achtundzwanzigst",
            "neunundzwanzigst",
            "dreißigst",
            "einunddreißigst",
        ];
        for (day, stem) in days.iter().enumerate() {
            for (context, ending) in [
                ("", "er"),
                ("am ", "en"),
                ("vom ", "en"),
                ("zum ", "en"),
                ("bis zum ", "en"),
                ("der ", "e"),
            ] {
                assert_eq!(
                    normalize(&format!("{context}{}.10.2026", day + 1)),
                    format!("{context}{stem}{ending} Oktober zweitausendsechsundzwanzig")
                );
            }
        }
    }
    #[test]
    fn invalid_calendar_dates_unchanged() {
        for s in [
            "31.02.2026",
            "32.10.2026",
            "00.10.2026",
            "10.13.2026",
            "10.00.2026",
            "31.04.2026",
            "31.06.2026",
            "31.09.2026",
            "31.11.2026",
            "1.1.0000",
            "31.02.",
            "00.10.",
            "2026-02-31",
            "2026-13-07",
            "0000-01-01",
        ] {
            assert_eq!(normalize(s), s);
            assert_eq!(
                normalize(&format!("Termin am {s}; es sind 2 Tage.")),
                format!("Termin am {s}; es sind zwei Tage.")
            );
        }
    }
    #[test]
    fn gregorian_leap_years() {
        for y in [2024, 2000, 2400] {
            assert_eq!(
                normalize(&format!("29.02.{y}")),
                format!("neunundzwanzigster Februar {}", cardinal(y))
            );
        }
        for y in [2025, 1900, 2100] {
            let s = format!("29.02.{y}");
            assert_eq!(normalize(&s), s);
        }
        assert_eq!(
            normalize("29.02.24"),
            "neunundzwanzigster Februar zweitausendvierundzwanzig"
        );
        assert_eq!(normalize("29.02.25"), "29.02.25");
        assert_eq!(normalize("am 29.02."), "am neunundzwanzigsten Februar");
    }
    #[test]
    fn dates_inside_technical_tokens_unchanged() {
        for s in [
            "1.2.3",
            "v1.2.3",
            "2.4.1",
            "Version 07.10.2026",
            "Version 07.10.26",
            "IP 07.10.26",
            "https://example.org/07.10.2026",
            "/app/07.10.2026/voice",
            "./07.10.2026",
            "C:\\07.10.2026\\voice",
            "host.07.10.2026.example",
            "07.10.2026.example",
            "07.10.26:10204",
            "german_24l",
            "Q8_0",
            "model07.10.2026",
            "release-2026-10-07",
            "2026-10-07T12:30:00",
            "2026-10-07/file",
            "7.10.2026.1",
            "7.10.2026,5",
            "07.10.20261",
            "07.10.2",
            "2026-1-07",
            "7.10",
        ] {
            assert_eq!(normalize(s), s, "{s}");
        }
    }
}

#[cfg(test)]
mod named_date_regressions {
    use super::normalize;
    #[test]
    fn all_days_contexts_and_optional_year() {
        let stems = [
            "erst",
            "zweit",
            "dritt",
            "viert",
            "fünft",
            "sechst",
            "siebt",
            "acht",
            "neunt",
            "zehnt",
            "elft",
            "zwölft",
            "dreizehnt",
            "vierzehnt",
            "fünfzehnt",
            "sechzehnt",
            "siebzehnt",
            "achtzehnt",
            "neunzehnt",
            "zwanzigst",
            "einundzwanzigst",
            "zweiundzwanzigst",
            "dreiundzwanzigst",
            "vierundzwanzigst",
            "fünfundzwanzigst",
            "sechsundzwanzigst",
            "siebenundzwanzigst",
            "achtundzwanzigst",
            "neunundzwanzigst",
            "dreißigst",
            "einunddreißigst",
        ];
        for (index, stem) in stems.iter().enumerate() {
            for (prefix, ending) in [
                ("", "er"),
                ("Der ", "e"),
                ("am ", "en"),
                ("vom ", "en"),
                ("zum ", "en"),
                ("bis zum ", "en"),
            ] {
                for (year, spoken) in [("", ""), (" 2026", " zweitausendsechsundzwanzig")] {
                    let input = format!("{prefix}{}. Oktober{year}", index + 1);
                    assert_eq!(
                        normalize(&input),
                        format!("{prefix}{stem}{ending} Oktober{spoken}"),
                        "{input}"
                    );
                }
            }
        }
    }
    #[test]
    fn months_case_and_unicode_whitespace() {
        for month in super::MONTHS {
            for spelling in [
                month.to_string(),
                month.to_lowercase(),
                month.to_uppercase(),
            ] {
                assert_eq!(
                    normalize(&format!("1. {spelling}")),
                    format!("erster {month}")
                );
                assert_eq!(
                    normalize(&format!("am 1. {spelling} 2026")),
                    format!("am ersten {month} zweitausendsechsundzwanzig")
                );
            }
        }
        assert_eq!(
            normalize("Am 07.\u{a0}Oktober\u{202f}2026."),
            "Am siebten Oktober zweitausendsechsundzwanzig."
        );
    }
    #[test]
    fn actual_ha_sentences_and_range() {
        for (input, expected) in [
            (
                "Heute ist Mittwoch, der 7. Oktober 2026.",
                "Heute ist Mittwoch, der siebte Oktober zweitausendsechsundzwanzig.",
            ),
            (
                "Der nächste Termin ist am 12. Oktober 2026 um 14:30 Uhr.",
                "Der nächste Termin ist am zwölften Oktober zweitausendsechsundzwanzig um vierzehn Uhr dreißig.",
            ),
            (
                "Vom 7. Oktober 2026 bis zum 9. Oktober 2026 sind es 3 Tage.",
                "Vom siebten Oktober zweitausendsechsundzwanzig bis zum neunten Oktober zweitausendsechsundzwanzig sind es drei Tage.",
            ),
            (
                "Am 7. Oktober 2026 sind es 21,5 °C, 67 % und 1.250 Watt bei 2.450,5 kWh.",
                "Am siebten Oktober zweitausendsechsundzwanzig sind es einundzwanzig Komma fünf Grad Celsius, siebenundsechzig Prozent und eintausendzweihundertfünfzig Watt bei zweitausendvierhundertfünfzig Komma fünf Kilowattstunden.",
            ),
        ] {
            assert_eq!(normalize(input), expected);
        }
    }
    #[test]
    fn calendar_validation_and_technical_tokens() {
        for input in [
            "31. Februar 2026",
            "29. Februar 2025",
            "31. April 2026",
            "32. Oktober 2026",
            "0. Oktober 2026",
            "7. Oktober 0000",
            "192.0.2.100",
            "Version 1.2.3",
            "v1.2.3",
            "https://example.org/7.Oktober2026",
            "/app/7.Oktober2026",
            "german_24l",
            "Q8_0",
        ] {
            assert_eq!(normalize(input), input, "{input}");
        }
        assert_eq!(
            normalize("29. Februar 2024"),
            "neunundzwanzigster Februar zweitausendvierundzwanzig"
        );
        assert_eq!(normalize("29. Februar"), "neunundzwanzigster Februar");
        assert_eq!(normalize("7. Oktoberfest"), "sieben. Oktoberfest");
    }
}

#[cfg(test)]
mod span_priority_regressions {
    use super::*;
    #[test]
    fn protected_year_does_not_change_calendar_validity() {
        for original in [
            "am 29 Februar 2025:",
            "am 29 Februar 2024:",
            "7 Oktober 2026:",
            "am 29 Februar 1.2.3",
        ] {
            assert_eq!(normalize(original), original);
        }
    }
    #[test]
    fn named_dates_all_calendar_days_and_contexts() {
        let stems = [
            "erst",
            "zweit",
            "dritt",
            "viert",
            "fünft",
            "sechst",
            "siebt",
            "acht",
            "neunt",
            "zehnt",
            "elft",
            "zwölft",
            "dreizehnt",
            "vierzehnt",
            "fünfzehnt",
            "sechzehnt",
            "siebzehnt",
            "achtzehnt",
            "neunzehnt",
            "zwanzigst",
            "einundzwanzigst",
            "zweiundzwanzigst",
            "dreiundzwanzigst",
            "vierundzwanzigst",
            "fünfundzwanzigst",
            "sechsundzwanzigst",
            "siebenundzwanzigst",
            "achtundzwanzigst",
            "neunundzwanzigst",
            "dreißigst",
            "einunddreißigst",
        ];
        for (month, name) in MONTHS.iter().enumerate() {
            for (year, spoken) in [("", ""), (" 2026", " zweitausendsechsundzwanzig")] {
                let max = if month == 1 {
                    if year.is_empty() { 29 } else { 28 }
                } else if [3, 5, 8, 10].contains(&month) {
                    30
                } else {
                    31
                };
                for day in 1..=max {
                    for (prefix, ending) in [
                        ("", "er"),
                        ("der ", "e"),
                        ("am ", "en"),
                        ("vom ", "en"),
                        ("zum ", "en"),
                        ("bis zum ", "en"),
                    ] {
                        for digits in [day.to_string(), format!("{day:02}")] {
                            for point in ["", "."] {
                                let input = format!("{prefix}{digits}{point} {name}{year}");
                                let expected =
                                    format!("{prefix}{}{ending} {name}{spoken}", stems[day - 1]);
                                assert_eq!(normalize(&input), expected, "{input}");
                                assert_eq!(normalize(&expected), expected, "idempotence: {input}");
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn invalid_named_dates_are_committed_unchanged() {
        for input in [
            "31 Februar 2026",
            "31. Februar 2026",
            "29 Februar 2025",
            "00 Oktober 2026",
            "32 Oktober 2026",
            "31 April",
            "am 31 April 2026",
            "29 Februar 1900",
            "7 Oktober 0000",
        ] {
            assert_eq!(normalize(input), input);
            assert!(matches!(
                span_at(
                    input,
                    if input.starts_with("am ") { 3 } else { 0 },
                    input.len()
                )
                .unwrap()
                .kind,
                SpanKind::Protected
            ));
        }
        assert_eq!(
            normalize("am 29 Februar 2024"),
            "am neunundzwanzigsten Februar zweitausendvierundzwanzig"
        );
        assert_eq!(
            normalize("29 Februar 2000"),
            "neunundzwanzigster Februar zweitausend"
        );
    }
    #[test]
    fn requested_live_sentences_and_case() {
        for original in [
            "Es ist Mittwoch, der 7. Oktober 2026.",
            "Es ist Mittwoch, der 7 Oktober 2026.",
            "Es ist Mittwoch, der 07. Oktober 2026.",
            "Es ist Mittwoch, der 07 Oktober 2026.",
        ] {
            assert_eq!(
                normalize(original),
                "Es ist Mittwoch, der siebte Oktober zweitausendsechsundzwanzig."
            );
        }
        assert_eq!(
            normalize("Der nächste Termin ist am 12 Oktober 2026 um 14:30 Uhr."),
            "Der nächste Termin ist am zwölften Oktober zweitausendsechsundzwanzig um vierzehn Uhr dreißig."
        );
        assert_eq!(
            normalize("AM 07\u{a0}OKTOBER\u{202f}2026"),
            "AM siebten Oktober zweitausendsechsundzwanzig"
        );
        assert_eq!(
            normalize("vom 7 Oktober 2026 bis zum 9 Oktober 2026"),
            "vom siebten Oktober zweitausendsechsundzwanzig bis zum neunten Oktober zweitausendsechsundzwanzig"
        );
    }
    #[test]
    fn all_requested_values_and_protected_tokens() {
        assert_eq!(
            normalize("07.10.2026; 7.10.2026; 7. Oktober 2026; 7 Oktober 2026"),
            "siebter Oktober zweitausendsechsundzwanzig; siebter Oktober zweitausendsechsundzwanzig; siebter Oktober zweitausendsechsundzwanzig; siebter Oktober zweitausendsechsundzwanzig"
        );
        assert_eq!(
            normalize(
                "14:30 Uhr; 21,5 Grad Celsius; 67 %; 1.250 Watt; 2.450,5 kWh; 230 Volt; 4,2 Ampere; 1.013 hPa; 25 km/h"
            ),
            "vierzehn Uhr dreißig; einundzwanzig Komma fünf Grad Celsius; siebenundsechzig Prozent; eintausendzweihundertfünfzig Watt; zweitausendvierhundertfünfzig Komma fünf Kilowattstunden; zweihundertdreißig Volt; vier Komma zwei Ampere; eintausenddreizehn Hektopascal; fünfundzwanzig Kilometer pro Stunde"
        );
        let technical = "192.0.2.100:10204 Version 1.2.3 https://example.org/7/2026 /app/7/2026 Q8_0 german_24l";
        assert_eq!(normalize(technical), technical);
        assert_eq!(
            normalize("am 7 Oktober 2026; 3,2026 kWh, 1.234,00 W, 67 % und 21,5 °C."),
            "am siebten Oktober zweitausendsechsundzwanzig; drei Komma zwei null zwei sechs Kilowattstunden, eintausendzweihundertvierunddreißig Komma null null Watt, siebenundsechzig Prozent und einundzwanzig Komma fünf Grad Celsius."
        );
    }
    #[test]
    fn structured_spans_consume_entire_values_before_cardinals() {
        for (input, expected_end) in [
            ("7 Oktober 2026", 14),
            ("14:30 Uhr", 9),
            ("2.450,5 kWh", 11),
            ("67 %", 4),
            ("3,2026", 6),
            ("1.234,00", 8),
            ("42", 2),
        ] {
            let span = span_at(input, 0, input.len()).unwrap();
            assert_eq!(span.range, 0..expected_end, "{input}");
            match input {
                "7 Oktober 2026" => assert!(matches!(span.kind, SpanKind::Date(_))),
                "14:30 Uhr" => assert!(matches!(span.kind, SpanKind::Time { .. })),
                "2.450,5 kWh" => assert!(matches!(span.kind, SpanKind::Quantity(_))),
                "67 %" => assert!(matches!(span.kind, SpanKind::Percent(_))),
                "3,2026" => assert!(matches!(span.kind, SpanKind::DecimalNumber(_))),
                "1.234,00" => assert!(matches!(span.kind, SpanKind::GroupedNumber(_))),
                _ => assert!(matches!(span.kind, SpanKind::Cardinal(_))),
            }
        }
    }
    #[test]
    fn protected_horizon_prevents_crossing_and_rescanning() {
        let input = "7 Oktober /app/2026 21,5 °C";
        let ranges = protected(input);
        let limit = ranges[0].0;
        let span = span_at(input, 0, limit).unwrap();
        assert!(span.range.end <= limit);
        assert_eq!(
            normalize(input),
            "siebter Oktober /app/2026 einundzwanzig Komma fünf Grad Celsius"
        );
        assert_eq!(
            normalize("Telefon 7 Oktober 2026"),
            "Telefon 7 Oktober zweitausendsechsundzwanzig"
        );
        assert_eq!(
            normalize("https://example.org/7.Oktober2026 7 Oktober 2026"),
            "https://example.org/7.Oktober2026 siebter Oktober zweitausendsechsundzwanzig"
        );
    }
}
#[cfg(test)]
mod final_candidate_regressions {
    use super::*;
    #[test]
    fn successful_ha_prompt_is_identical_to_reference() {
        let raw = "Es ist Mittwoch der 7. Oktober 2026";
        let expected = "Es ist Mittwoch der siebte Oktober zweitausendsechsundzwanzig";
        assert_eq!(normalize_final(raw), expected);
        assert_eq!(crate::misaki_normalizer::normalize(raw), expected);
    }
    #[test]
    fn prewritten_cardinal_is_not_silently_repaired() {
        for s in [
            "Es ist Mittwoch, der sieben Oktober zweitausendsechsundzwanzig.",
            "am sieben Oktober zweitausendsechsundzwanzig",
        ] {
            assert_eq!(normalize_final(s), s);
            assert_eq!(crate::misaki_normalizer::normalize(s), s);
        }
    }
    #[test]
    fn existing_ha_cases_unchanged() {
        let cases: Vec<String> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/final-normalizer-live-cases.json"
        ))
        .unwrap();
        for s in cases {
            if !s.contains('€') && !s.contains("Dr.") {
                assert_eq!(normalize_final(&s), normalize(&s), "{s}");
            }
        }
    }
    #[test]
    fn calendar_dates_and_unicode_all_contexts() {
        let stems = [
            "erst",
            "zweit",
            "dritt",
            "viert",
            "fünft",
            "sechst",
            "siebt",
            "acht",
            "neunt",
            "zehnt",
            "elft",
            "zwölft",
            "dreizehnt",
            "vierzehnt",
            "fünfzehnt",
            "sechzehnt",
            "siebzehnt",
            "achtzehnt",
            "neunzehnt",
            "zwanzigst",
            "einundzwanzigst",
            "zweiundzwanzigst",
            "dreiundzwanzigst",
            "vierundzwanzigst",
            "fünfundzwanzigst",
            "sechsundzwanzigst",
            "siebenundzwanzigst",
            "achtundzwanzigst",
            "neunundzwanzigst",
            "dreißigst",
            "einunddreißigst",
        ];
        let mut checked = 0;
        for (year, y) in [("", None), (" 2026", Some(2026))] {
            for (m, name) in MONTHS.iter().enumerate() {
                for (day, stem) in stems.iter().enumerate() {
                    if !(Date {
                        len: 0,
                        day: day as u8 + 1,
                        month: m as u8 + 1,
                        year: y,
                    })
                    .valid()
                    {
                        continue;
                    }
                    for dot in ["", "."] {
                        for pad in [false, true] {
                            for space in [" ", "\u{a0}", "\u{202f}"] {
                                for (prefix, ending) in [
                                    ("", "er"),
                                    ("der ", "e"),
                                    ("am ", "en"),
                                    ("vom ", "en"),
                                    ("zum ", "en"),
                                    ("bis zum ", "en"),
                                ] {
                                    let input = format!(
                                        "{prefix}{}{dot}{space}{name}{year}",
                                        if pad {
                                            format!("{:02}", day + 1)
                                        } else {
                                            (day + 1).to_string()
                                        }
                                    );
                                    let expected = format!(
                                        "{prefix}{stem}{ending} {name}{}",
                                        if year.is_empty() {
                                            ""
                                        } else {
                                            " zweitausendsechsundzwanzig"
                                        }
                                    );
                                    assert_eq!(normalize_final(&input), expected, "{input}");
                                    checked += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 52632);
    }
    #[test]
    fn invalid_dates_still_protected() {
        for s in [
            "31. Februar 2026",
            "am 31.02.2026",
            "29.02.2025",
            "31 April 2026",
            "0. Oktober 2026",
        ] {
            assert_eq!(normalize_final(s), s);
        }
    }
    #[test]
    fn currency_rounding_and_punctuation() {
        for (a, b) in [
            ("€1,005", "eins Euro und eins Cent"),
            ("2,50 €", "zwei Euro und fünfzig Cent"),
            (
                "Kosten: €1.250,50.",
                "Kosten: eintausendzweihundertfünfzig Euro und fünfzig Cent.",
            ),
            ("£12,999", "dreizehn Pfund"),
            ("¥0,01", "null Yen und eins Cent"),
            ("$1.250", "eintausendzweihundertfünfzig Dollar"),
        ] {
            assert_eq!(normalize_final(a), b, "{a}");
        }
    }
    #[test]
    fn abbreviations_do_not_reprocess_date() {
        assert_eq!(
            normalize_final("Dr. Müller kommt z.B. am 7. Oktober 2026."),
            "Doktor Müller kommt zum Beispiel am siebten Oktober zweitausendsechsundzwanzig."
        );
        assert_eq!(normalize_final("Hr. Müller"), "Herr Müller");
        assert_eq!(
            normalize_final("7. Okt. 2026"),
            "siebter Oktober zweitausendsechsundzwanzig"
        );
    }
    #[test]
    fn technical_ranges_protect_extensions() {
        for s in [
            "192.0.2.100",
            "Version 1.2.3",
            "https://example.org:8080/42?value=1",
            "/app/voices/voice2.safetensors",
            "./foo42.wav",
            "https://example.org/Dr./1.250€",
            "/app/Dr./voice7.wav",
            "Q8_0 german_24l",
        ] {
            assert_eq!(normalize_final(s), s, "{s}");
        }
    }
    #[test]
    fn cardinal_properties() {
        for n in 0..100000 {
            let raw = format!("Wert {n}.");
            let expected = format!("Wert {}.", cardinal(n));
            assert_eq!(normalize_final(&raw), expected);
            assert_eq!(normalize_final(&expected), expected);
        }
    }
    #[test]
    fn span_results_are_not_reinterpreted() {
        for s in [
            "Dr. Müller zahlt €1,005 am 7. Oktober 2026 um 14:30 Uhr.",
            "67 % 2.450,5 kWh 25 km/h",
        ] {
            let n = normalize_final(s);
            assert_eq!(normalize_final(&n), n);
        }
    }
}
