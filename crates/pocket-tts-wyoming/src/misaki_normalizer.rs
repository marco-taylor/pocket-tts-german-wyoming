// Rust adaptation of semidark/misaki misaki/de.py, bbaf917e7bf7fbbe830f74f4acda6583ce1c0caf.
// Original contributors: Nico Thomaier and apples-kksk. Apache-2.0; see licenses/misaki-APACHE-2.0.txt.
// Modified: pure text normalization translated to Rust. Deliberately retains upstream rule order and quirks.
use fancy_regex::{Captures, Regex};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};
static CACHE: LazyLock<Mutex<HashMap<String, Regex>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
fn sub(s: &str, pattern: &str, f: impl Fn(&Captures<'_>) -> String) -> String {
    let re = {
        let mut c = CACHE.lock().unwrap();
        c.entry(pattern.into())
            .or_insert_with(|| Regex::new(pattern).unwrap())
            .clone()
    };
    let mut out = String::new();
    let mut end = 0;
    for c in re.captures_iter(s) {
        let Ok(c) = c else { return s.into() };
        let m = c.get(0).unwrap();
        out.push_str(&s[end..m.start()]);
        out.push_str(&f(&c));
        end = m.end();
    }
    out.push_str(&s[end..]);
    out
}
fn g<'a>(c: &'a Captures<'a>, n: usize) -> &'a str {
    c.get(n).unwrap().as_str()
}
const ONES: [&str; 20] = [
    "",
    "ein",
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
fn integer(n: u128, standalone: bool) -> String {
    match n {
        0 => "null".into(),
        1 => if standalone { "eins" } else { "ein" }.into(),
        2..=19 => ONES[n as usize].into(),
        20..=99 => {
            if n % 10 == 0 {
                TENS[(n / 10) as usize].into()
            } else {
                format!("{}und{}", ONES[(n % 10) as usize], TENS[(n / 10) as usize])
            }
        }
        100..=999 => format!(
            "{}hundert{}",
            ONES[(n / 100) as usize],
            if n % 100 == 0 {
                String::new()
            } else {
                integer(n % 100, false)
            }
        ),
        1000..=999999 => format!(
            "{}tausend{}",
            integer(n / 1000, false),
            if n % 1000 == 0 {
                String::new()
            } else {
                integer(n % 1000, false)
            }
        ),
        _ => {
            let (scale, singular, plural) = if n < 1_000_000_000 {
                (1_000_000, "eine Million", "Millionen")
            } else {
                (1_000_000_000, "eine Milliarde", "Milliarden")
            };
            let mut s = if n / scale == 1 {
                singular.into()
            } else {
                format!("{} {plural}", integer(n / scale, false))
            };
            if n % scale > 0 {
                s.push(' ');
                s.push_str(&integer(n % scale, false));
            }
            s
        }
    }
}
fn num(s: &str) -> String {
    s.parse::<u128>()
        .map(|n| integer(n, true))
        .unwrap_or_else(|_| s.into())
}
fn ordinal(n: u128) -> String {
    match n {
        1 => "erst".into(),
        2 => "zweit".into(),
        3 => "dritt".into(),
        7 => "siebt".into(),
        8 => "acht".into(),
        _ => format!("{}{}", integer(n, false), if n < 20 { "t" } else { "st" }),
    }
}
fn year(n: u128) -> String {
    if (1100..2000).contains(&n) {
        format!(
            "{}hundert{}",
            integer(n / 100, false),
            if n % 100 == 0 {
                String::new()
            } else {
                integer(n % 100, false)
            }
        )
    } else {
        integer(n, true)
    }
}
const MONTHS: [&str; 13] = [
    "",
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
pub(crate) fn currency(sym: &str, amount: &str) -> String {
    let cleaned = amount.replace('.', "").replace(',', ".");
    let parts: Vec<_> = cleaned.split('.').collect();
    if parts.len() > 2 {
        return format!("{sym}{amount}");
    }
    let Ok(whole) = parts[0].parse::<u128>() else {
        return format!("{sym}{amount}");
    };
    let fraction = parts.get(1).copied().unwrap_or("");
    if !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return format!("{sym}{amount}");
    }
    let mut digits = fraction.bytes().chain(std::iter::repeat(b'0'));
    let cents =
        (digits.next().unwrap() - b'0') as u128 * 10 + (digits.next().unwrap() - b'0') as u128;
    let Some(total) = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(cents + u128::from(digits.next().unwrap() >= b'5')))
    else {
        return format!("{sym}{amount}");
    };
    let word = match sym {
        "€" => "Euro",
        "$" => "Dollar",
        "£" => "Pfund",
        _ => "Yen",
    };
    let mut s = format!("{} {word}", integer(total / 100, true));
    if total % 100 > 0 {
        s += &format!(" und {} Cent", integer(total % 100, true));
    }
    s
}
pub fn normalize(input: &str) -> String {
    let mut s = input
        .chars()
        .map(|c| match c {
            '„' | '“' | '«' | '»' | '‹' | '›' => '"',
            '‘' | '’' => '\'',
            c if c.is_whitespace() && c != ' ' && c != '\n' => ' ',
            c => c,
        })
        .collect::<String>();
    for &(p, r) in ABBREVIATIONS {
        s = sub(&s, p, |_| r.into());
    }
    for &(a, m) in &[
        ("Jan", "Januar"),
        ("Feb", "Februar"),
        ("Mär", "März"),
        ("Apr", "April"),
        ("Jun", "Juni"),
        ("Jul", "Juli"),
        ("Aug", "August"),
        ("Sep", "September"),
        ("Okt", "Oktober"),
        ("Nov", "November"),
        ("Dez", "Dezember"),
    ] {
        s = sub(&s, &format!(r"\b{a}\.(?=\s)"), |_| m.into());
    }
    s = sub(&s, r"([€$£¥])\s*(\d[\d.,]*)", |c| {
        currency(g(c, 1), g(c, 2))
    });
    s = sub(&s, r"(\d[\d.,]*)\s*([€$£¥])", |c| {
        currency(g(c, 2), g(c, 1))
    });
    s = sub(&s, r"\b(\d{1,2}):(\d{2})\b(?:\s*Uhr\b)?", |c| {
        let h = g(c, 1).parse::<u128>().unwrap_or(99);
        let m = g(c, 2).parse::<u128>().unwrap_or(99);
        if h > 23 || m > 59 {
            g(c, 0).into()
        } else {
            format!(
                "{} Uhr{}",
                integer(h, true),
                if m == 0 {
                    String::new()
                } else {
                    format!(" {}", integer(m, true))
                }
            )
        }
    });
    s = sub(&s, r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b", |c| {
        let d = g(c, 1).parse::<u128>().unwrap_or(0);
        let m = g(c, 2).parse::<usize>().unwrap_or(0);
        let y = g(c, 3).parse::<u128>().unwrap_or(0);
        if !(1..=31).contains(&d) || !(1..=12).contains(&m) {
            g(c, 0).into()
        } else {
            format!("{}e {} {}", ordinal(d), MONTHS[m], year(y))
        }
    });
    s = sub(&s, r"(?<!\d)(\d{1,2})\.\s", |c| {
        format!("{}e ", ordinal(g(c, 1).parse().unwrap_or(0)))
    });
    s = sub(&s, r"\b(\d{4})\b", |c| {
        let n = g(c, 1).parse().unwrap_or(0);
        if (1100..=2099).contains(&n) {
            year(n)
        } else {
            num(g(c, 1))
        }
    });
    s = sub(&s, r"\b\d{1,3}(?:\.\d{3})+(?:,\d+)?\b", |c| {
        let cleaned = g(c, 0).replace('.', "").replace(',', ".");
        let Ok(v) = cleaned.parse::<f64>() else {
            return g(c, 0).into();
        };
        if !v.is_finite() || v >= u128::MAX as f64 {
            return g(c, 0).into();
        };
        if v.fract() == 0.0 {
            integer(v as u128, true)
        } else {
            let (a, b) = cleaned.split_once('.').unwrap();
            format!(
                "{} Komma {}",
                num(a),
                b.chars()
                    .map(|d| num(&d.to_string()))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        }
    });
    s = sub(&s, r"\b(\d+),(\d+)\b", |c| {
        format!(
            "{} Komma {}",
            num(g(c, 1)),
            g(c, 2)
                .chars()
                .map(|d| num(&d.to_string()))
                .collect::<Vec<_>>()
                .join(" ")
        )
    });
    let original = s.clone();
    s = sub(&s, r"\b(\d+)\b", |c| {
        let m = c.get(0).unwrap();
        let start = original[..m.start()]
            .char_indices()
            .rev()
            .nth(2)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let end = original[m.end()..]
            .char_indices()
            .nth(7)
            .map(|(i, _)| m.end() + i)
            .unwrap_or(original.len());
        let re = Regex::new(r"\b\d{1,2}:\d{2}\b(?:\s*Uhr\b)?").unwrap();
        let protected = re
            .find_iter(&original[start..end])
            .filter_map(Result::ok)
            .any(|t| start + t.start() <= m.start() && m.end() <= start + t.end());
        if protected {
            g(c, 0).into()
        } else {
            num(g(c, 1))
        }
    });
    s = sub(&s, r"[ \t]{2,}", |_| " ".into());
    s = sub(&s, r"\n{3,}", |_| "\n\n".into());
    s.trim().into()
}

pub(crate) const ABBREVIATIONS: &[(&str, &str)] = &[
    (r"\bDr\.(?=\s)", "Doktor"),
    (r"\bProf\.(?=\s)", "Professor"),
    (r"\bHr\.(?=\s)", "Herr "),
    (r"\bFr\.(?=\s[A-ZÄÖÜ])", "Frau"),
    (r"\bDipl\.\s*-?\s*Ing\.", "Diplom-Ingenieur"),
    (r"\bStr\.(?=\s)", "Straße"),
    (r"\bNr\.(?=\s*\d)", "Nummer"),
    (r"\bTel\.(?=\s)", "Telefon"),
    (r"\bAbt\.(?=\s)", "Abteilung"),
    (r"\bGmbH\b", "Gesellschaft mit beschränkter Haftung"),
    (r"\bAG\b(?=[\s,.]|$)", "Aktiengesellschaft"),
    (r"(?i)\bz\.\s*B\.", "zum Beispiel"),
    (r"(?i)\bd\.\s*h\.", "das heißt"),
    (r"(?i)\bu\.\s*a\.", "unter anderem"),
    (r"(?i)\bbzw\.", "beziehungsweise"),
    (r"(?i)\busw\.", "und so weiter"),
    (r"(?i)\betc\.", "et cetera"),
    (r"(?i)\bca\.", "circa"),
    (r"(?i)\bvgl\.", "vergleiche"),
    (r"(?i)\binkl\.", "inklusive"),
    (r"(?i)\bexkl\.", "exklusive"),
    (r"(?i)\bggf\.", "gegebenenfalls"),
    (r"(?i)\bi\.\s*d\.\s*R\.", "in der Regel"),
    (r"(?i)\bo\.\s*ä\.", "oder ähnliches"),
    (r"(?i)\bu\.\s*U\.", "unter Umständen"),
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_cases() {
        for (raw, expected) in [
            (
                "Es ist Mittwoch, der 7. Oktober 2026.",
                "Es ist Mittwoch, der siebte Oktober zweitausendsechsundzwanzig.",
            ),
            (
                "Es ist Mittwoch, der 7 Oktober 2026.",
                "Es ist Mittwoch, der sieben Oktober zweitausendsechsundzwanzig.",
            ),
            (
                "Der nächste Termin ist am 12. Oktober 2026 um 14:30 Uhr.",
                "Der nächste Termin ist am zwölfte Oktober zweitausendsechsundzwanzig um vierzehn Uhr dreißig.",
            ),
            (
                "Der nächste Termin ist am 12 Oktober 2026 um 14:30 Uhr.",
                "Der nächste Termin ist am zwölf Oktober zweitausendsechsundzwanzig um vierzehn Uhr dreißig.",
            ),
            ("2026", "zweitausendsechsundzwanzig"),
            ("7. Oktober", "siebte Oktober"),
            ("am 7. Oktober", "am siebte Oktober"),
            ("67 %", "siebenundsechzig %"),
            ("21,5 Grad Celsius", "einundzwanzig Komma fünf Grad Celsius"),
            ("1.250 Watt", "eintausendzweihundertfünfzig Watt"),
            (
                "2.450,5 kWh",
                "zweitausendvierhundertfünfzig Komma fünf kWh",
            ),
            ("230 Volt", "zweihundertdreißig Volt"),
            ("4,2 Ampere", "vier Komma zwei Ampere"),
            ("1.013 hPa", "eintausenddreizehn hPa"),
            ("25 km/h", "fünfundzwanzig km/h"),
        ] {
            assert_eq!(normalize(raw), expected, "{raw}");
        }
    }
    #[test]
    fn currency_rounding() {
        for (a, b) in [
            ("€1,005", "eins Euro und eins Cent"),
            ("2,50 €", "zwei Euro und fünfzig Cent"),
            ("£12,999", "dreizehn Pfund"),
            ("¥0,01", "null Yen und eins Cent"),
            ("$1.250", "eintausendzweihundertfünfzig Dollar"),
        ] {
            assert_eq!(normalize(a), b);
        }
    }
    #[test]
    fn faithful_quirks() {
        for (a, b) in [
            (
                "am 31.02.2026",
                "am einunddreißigste Februar zweitausendsechsundzwanzig",
            ),
            ("101", "einhundertein"),
            ("1984", "neunzehnhundertvierundachtzig"),
            ("25:00 Uhr", "25:00 Uhr"),
            ("1.250,00", "eintausendzweihundertfünfzig"),
            ("3,2026", "drei,zweitausendsechsundzwanzig"),
        ] {
            assert_eq!(normalize(a), b, "{a}");
        }
    }
    #[test]
    fn whitespace_abbreviations() {
        assert_eq!(
            normalize("„Dr. Müller“\u{a0}z.B.\tNr. 7"),
            "\"Doktor Müller\" zum Beispiel Nummer sieben"
        );
        assert_eq!(
            normalize("7. Okt. 2026"),
            "siebte Oktober zweitausendsechsundzwanzig"
        );
    }
}
