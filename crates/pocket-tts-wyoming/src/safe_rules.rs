//! Conservative independent native rules informed by Misaki analysis; no Misaki runtime.
//! Original Rust implementation is MIT; see THIRD_PARTY_NOTICES.md for reference attribution.
use crate::german_normalizer::{cardinal, normalize};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Euro,
    Cent,
    Example,
    Approx,
    Respectively,
    Inclusive,
    Number,
    Etc,
    Doctor,
    Professor,
    Friday,
    Company,
    Month,
}
pub const ACCEPTED: &[Rule] = &[
    Rule::Euro,
    Rule::Cent,
    Rule::Example,
    Rule::Approx,
    Rule::Respectively,
    Rule::Inclusive,
    Rule::Number,
    Rule::Etc,
    Rule::Month,
];
pub const ALL: &[Rule] = &[
    Rule::Euro,
    Rule::Cent,
    Rule::Example,
    Rule::Approx,
    Rule::Respectively,
    Rule::Inclusive,
    Rule::Number,
    Rule::Etc,
    Rule::Doctor,
    Rule::Professor,
    Rule::Friday,
    Rule::Company,
    Rule::Month,
];
pub(crate) fn safe_context(before: &str) -> bool {
    let word = before
        .trim_end()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|x| !x.is_empty())
        .next_back()
        .unwrap_or("")
        .to_lowercase();
    ![
        "name",
        "entität",
        "entity",
        "sensorname",
        "label",
        "status",
        "statuscode",
        "code",
        "firma",
        "modell",
        "version",
        "id",
        "token",
    ]
    .contains(&word.as_str())
        && before.matches('"').count() % 2 == 0
        && before.matches('\'').count() % 2 == 0
        && before.matches('‘').count() == before.matches('’').count()
        && before.matches('„').count() == before.matches('“').count()
        && before.matches('«').count() == before.matches('»').count()
}
fn suffix<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(word)?;
    if rest.chars().next().is_some_and(|c| {
        (!c.is_whitespace()
            && !matches!(
                c,
                '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '"' | '\'' | '“' | '»'
            ))
            || c == '_'
            || c == '/'
            || c == '-'
            || c == '.'
                && rest
                    .get(1..)
                    .is_some_and(|r| r.starts_with(|c: char| c.is_alphanumeric()))
    }) {
        None
    } else {
        Some(rest)
    }
}
pub(crate) fn at(text: &str, start: usize, limit: usize, rule: Rule) -> Option<(usize, String)> {
    let probe = &text[start..limit];
    let candidate = match rule {
        Rule::Euro | Rule::Cent => probe.starts_with(|c: char| c.is_ascii_digit()),
        Rule::Example => probe.starts_with("z. B."),
        Rule::Approx => probe.starts_with("ca."),
        Rule::Respectively => probe.starts_with("bzw."),
        Rule::Inclusive => probe.starts_with("inkl."),
        Rule::Number => probe.starts_with("Nr."),
        Rule::Etc => probe.starts_with("etc."),
        Rule::Doctor => probe.starts_with("Dr."),
        Rule::Professor => probe.starts_with("Prof."),
        Rule::Friday => probe.starts_with("Fr."),
        Rule::Company => probe.starts_with("AG"),
        Rule::Month => false,
    };
    if !candidate {
        return None;
    }
    if !safe_context(&text[..start])
        && !matches!(
            rule,
            Rule::Doctor | Rule::Professor | Rule::Friday | Rule::Company
        )
    {
        return None;
    }
    let s = &text[start..limit];
    if matches!(rule, Rule::Euro | Rule::Cent) {
        let len = s
            .bytes()
            .take_while(|c| c.is_ascii_digit() || matches!(c, b'.' | b','))
            .count();
        if len == 0 {
            return None;
        }
        let number = &s[..len];
        let space = s[len..].len() - s[len..].trim_start().len();
        let units = &s[len + space..];
        let unit = match rule {
            Rule::Euro => ["€", "Euro"]
                .into_iter()
                .find(|u| suffix(units, u).is_some()),
            _ => ["Cent", "ct"]
                .into_iter()
                .find(|u| suffix(units, u).is_some()),
        }?;
        let mut pieces = number.split(',');
        let whole = pieces.next()?;
        let fraction = pieces.next();
        if pieces.next().is_some()
            || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        let groups: Vec<_> = whole.split('.').collect();
        if groups
            .iter()
            .any(|g| g.is_empty() || !g.bytes().all(|c| c.is_ascii_digit()))
            || groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3))
        {
            return None;
        }
        let n = whole.replace('.', "").parse::<u64>().ok()?;
        let result = if rule == Rule::Cent {
            format!(
                "{} Cent",
                if fraction.is_none() && n == 1 {
                    "ein".into()
                } else {
                    normalize(number)
                }
            )
        } else if let Some(f) = fraction.filter(|f| f.len() > 2) {
            format!(
                "{} Komma {} Euro",
                cardinal(n),
                f.chars()
                    .map(|c| cardinal(c.to_digit(10).unwrap() as u64))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        } else {
            let euros = if n == 1 { "ein".into() } else { cardinal(n) };
            let cents = fraction
                .map(|f| f.parse::<u64>().unwrap() * if f.len() == 1 { 10 } else { 1 })
                .unwrap_or(0);
            if cents == 0 {
                format!("{euros} Euro")
            } else {
                let cents = if cents == 1 {
                    "ein".into()
                } else {
                    cardinal(cents)
                };
                if n == 0 {
                    format!("{cents} Cent")
                } else {
                    format!("{euros} Euro und {cents} Cent")
                }
            }
        };
        return Some((len + space + unit.len(), result));
    }
    let (raw, out) = match rule {
        Rule::Example => ("z. B.", "zum Beispiel"),
        Rule::Approx => ("ca.", "circa"),
        Rule::Respectively => ("bzw.", "beziehungsweise"),
        Rule::Inclusive => ("inkl.", "inklusive"),
        Rule::Number => ("Nr.", "Nummer"),
        Rule::Etc => ("etc.", "et cetera"),
        Rule::Doctor => ("Dr.", "Doktor"),
        Rule::Professor => ("Prof.", "Professor"),
        Rule::Friday => ("Fr.", "Frau"),
        Rule::Company => ("AG", "Aktiengesellschaft"),
        _ => return None,
    };
    let rest = s.strip_prefix(raw)?;
    if rest
        .chars()
        .next()
        .is_some_and(|c| !c.is_whitespace() && !matches!(c, ',' | ';' | '!' | '?' | '.'))
    {
        return None;
    }
    if rule == Rule::Approx
        && !rest
            .trim_start()
            .trim_start_matches('-')
            .starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    if rule == Rule::Number {
        let previous = text[..start].split_whitespace().next_back()?.to_lowercase();
        if ![
            "gerät",
            "geräte",
            "sensor",
            "lampe",
            "steckdose",
            "termin",
            "raum",
        ]
        .contains(&previous.as_str())
            || !rest.trim_start().starts_with(|c: char| c.is_ascii_digit())
        {
            return None;
        }
    }
    if rule == Rule::Friday && !rest.trim_start().starts_with(|c: char| c.is_uppercase()) {
        return None;
    }
    Some((raw.len(), out.into()))
}
