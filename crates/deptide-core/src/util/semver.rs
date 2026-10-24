use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use regex::{Captures, Regex};

const MAX_LENGTH: usize = 256;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const MAX_SAFE_BUILD_LENGTH: usize = MAX_LENGTH - 6;
const NULL_SET: &str = "<0.0.0-0";
const JS_WHITESPACE: &str = r"[\t\n\x{0B}\x{0C}\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]";
const LETTER_DASH_NUMBER: &str = "[a-zA-Z0-9-]";

struct Patterns {
    full: Regex,
    xrange: Regex,
    tilde: Regex,
    tilde_trim: Regex,
    caret: Regex,
    caret_trim: Regex,
    comparator: Regex,
    comparator_trim: Regex,
    hyphen_range: Regex,
    build: Regex,
    star: Regex,
    greater_or_equal_zero: Regex,
    whitespace: Regex,
}

fn safe(source: &str) -> String {
    let bounded = [
        (r"\s", 1),
        (r"\d", MAX_LENGTH),
        (LETTER_DASH_NUMBER, MAX_SAFE_BUILD_LENGTH),
    ];
    let mut source = source.to_string();
    for (token, max) in bounded {
        source = source
            .replace(&format!("{token}*"), &format!("{token}{{0,{max}}}"))
            .replace(&format!("{token}+"), &format!("{token}{{1,{max}}}"));
    }
    source.replace(r"\d", "[0-9]").replace(r"\s", JS_WHITESPACE)
}

fn compile(source: &str) -> Regex {
    Regex::new(&safe(source)).expect("node-semver pattern compiles")
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let numeric = r"0|[1-9]\d*";
        let numeric_loose = r"\d+";
        let non_numeric = format!(r"\d*[a-zA-Z-]{LETTER_DASH_NUMBER}*");
        let main_version = format!(r"({numeric})\.({numeric})\.({numeric})");
        let main_version_loose = format!(r"({numeric_loose})\.({numeric_loose})\.({numeric_loose})");
        let prerelease_identifier = format!("(?:{non_numeric}|{numeric})");
        let prerelease_identifier_loose = format!("(?:{non_numeric}|{numeric_loose})");
        let prerelease = format!(r"(?:-({prerelease_identifier}(?:\.{prerelease_identifier})*))");
        let prerelease_loose =
            format!(r"(?:-?({prerelease_identifier_loose}(?:\.{prerelease_identifier_loose})*))");
        let build_identifier = format!("{LETTER_DASH_NUMBER}+");
        let build = format!(r"(?:\+({build_identifier}(?:\.{build_identifier})*))");
        let full_plain = format!("v?{main_version}{prerelease}?{build}?");
        let loose_plain = format!(r"[v=\s]*{main_version_loose}{prerelease_loose}?{build}?");
        let greater_less = "((?:<|>)?=?)";
        let xrange_identifier = format!(r"{numeric}|x|X|\*");
        let xrange_plain = format!(
            r"[v=\s]*({xrange_identifier})(?:\.({xrange_identifier})(?:\.({xrange_identifier})(?:{prerelease})?{build}?)?)?"
        );

        Patterns {
            full: compile(&format!("^{full_plain}$")),
            xrange: compile(&format!(r"^{greater_less}\s*{xrange_plain}$")),
            tilde: compile(&format!("^(?:~>?){xrange_plain}$")),
            tilde_trim: compile(r"(\s*)(?:~>?)\s+"),
            caret: compile(&format!(r"^(?:\^){xrange_plain}$")),
            caret_trim: compile(r"(\s*)(?:\^)\s+"),
            comparator: compile(&format!(r"^{greater_less}\s*({full_plain})$|^$")),
            comparator_trim: compile(&format!(
                r"(\s*){greater_less}\s*({loose_plain}|{xrange_plain})"
            )),
            hyphen_range: compile(&format!(
                r"^\s*({xrange_plain})\s+-\s+({xrange_plain})\s*$"
            )),
            build: compile(&build),
            star: compile(r"(<|>)?=?\s*\*"),
            greater_or_equal_zero: compile(r"^\s*>=\s*0\.0\.0\s*$"),
            whitespace: Regex::new(&format!("{JS_WHITESPACE}+")).expect("whitespace compiles"),
        }
    })
}

fn trim_like_javascript(text: &str) -> &str {
    text.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

fn collapse_whitespace(text: &str) -> String {
    patterns()
        .whitespace
        .split(trim_like_javascript(text))
        .collect::<Vec<_>>()
        .join(" ")
}

fn group<'t>(captures: &Captures<'t>, index: usize) -> &'t str {
    captures.get(index).map_or("", |found| found.as_str())
}

fn is_wildcard(part: &str) -> bool {
    part.is_empty() || part.eq_ignore_ascii_case("x") || part == "*"
}

fn plus_one(number: &str) -> String {
    number
        .parse::<u64>()
        .unwrap_or(u64::MAX)
        .saturating_add(1)
        .to_string()
}

fn compare_identifiers(left: &str, right: &str) -> Ordering {
    let is_numeric = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    match (is_numeric(left), is_numeric(right)) {
        (true, true) => {
            let as_number = |part: &str| part.parse::<f64>().unwrap_or(f64::INFINITY);
            as_number(left)
                .partial_cmp(&as_number(right))
                .unwrap_or(Ordering::Equal)
        }
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => left.cmp(right),
    }
}

#[derive(Debug, Clone)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    prerelease: Vec<String>,
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        if text.encode_utf16().count() > MAX_LENGTH {
            return None;
        }
        let captures = patterns().full.captures(trim_like_javascript(text))?;
        let number = |index: usize| -> Option<u64> {
            let value: u64 = group(&captures, index).parse().ok()?;
            (value <= MAX_SAFE_INTEGER).then_some(value)
        };
        let prerelease = match group(&captures, 4) {
            "" => Vec::new(),
            identifiers => identifiers.split('.').map(str::to_string).collect(),
        };

        Some(Self {
            major: number(1)?,
            minor: number(2)?,
            patch: number(3)?,
            prerelease,
        })
    }

    pub fn clean(text: &str) -> Option<Self> {
        Self::parse(trim_like_javascript(text).trim_start_matches(['=', 'v']))
    }

    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }

    fn same_release(&self, other: &Self) -> bool {
        (self.major, self.minor, self.patch) == (other.major, other.minor, other.patch)
    }

    fn compare_prerelease(&self, other: &Self) -> Ordering {
        match (self.is_prerelease(), other.is_prerelease()) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            (false, false) => return Ordering::Equal,
            (true, true) => {}
        }
        for index in 0.. {
            match (self.prerelease.get(index), other.prerelease.get(index)) {
                (None, None) => return Ordering::Equal,
                (Some(_), None) => return Ordering::Greater,
                (None, Some(_)) => return Ordering::Less,
                (Some(left), Some(right)) if left == right => continue,
                (Some(left), Some(right)) => return compare_identifiers(left, right),
            }
        }
        Ordering::Equal
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if self.is_prerelease() {
            write!(formatter, "-{}", self.prerelease.join("."))?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| self.compare_prerelease(other))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

#[derive(Debug, Clone)]
struct Comparator {
    operator: String,
    version: Option<Version>,
}

impl Comparator {
    fn parse(text: &str) -> Option<Self> {
        let text = collapse_whitespace(text);
        let captures = patterns().comparator.captures(&text)?;
        let operator = match group(&captures, 1) {
            "=" => "",
            operator => operator,
        };
        let version = match group(&captures, 2) {
            "" => None,
            version => Some(Version::parse(version)?),
        };
        Some(Self {
            operator: operator.to_string(),
            version,
        })
    }

    fn value(&self) -> String {
        match &self.version {
            Some(version) => format!("{}{version}", self.operator),
            None => String::new(),
        }
    }

    fn test(&self, candidate: &Version) -> bool {
        let Some(version) = &self.version else {
            return true;
        };
        let order = candidate.cmp(version);
        match self.operator.as_str() {
            "<" => order == Ordering::Less,
            "<=" => order != Ordering::Greater,
            ">" => order == Ordering::Greater,
            ">=" => order != Ordering::Less,
            _ => order == Ordering::Equal,
        }
    }
}

fn replace_caret(part: &str) -> String {
    patterns()
        .caret
        .replace(part, |captures: &Captures| {
            let (major, minor, patch, prerelease) = (
                group(captures, 1),
                group(captures, 2),
                group(captures, 3),
                group(captures, 4),
            );
            if is_wildcard(major) {
                String::new()
            } else if is_wildcard(minor) {
                format!(">={major}.0.0 <{}.0.0-0", plus_one(major))
            } else if is_wildcard(patch) {
                if major == "0" {
                    format!(">={major}.{minor}.0 <{major}.{}.0-0", plus_one(minor))
                } else {
                    format!(">={major}.{minor}.0 <{}.0.0-0", plus_one(major))
                }
            } else {
                let lower = match prerelease {
                    "" => format!(">={major}.{minor}.{patch}"),
                    prerelease => format!(">={major}.{minor}.{patch}-{prerelease}"),
                };
                let upper = if major != "0" {
                    format!("<{}.0.0-0", plus_one(major))
                } else if minor != "0" {
                    format!("<{major}.{}.0-0", plus_one(minor))
                } else {
                    format!("<{major}.{minor}.{}-0", plus_one(patch))
                };
                format!("{lower} {upper}")
            }
        })
        .into_owned()
}

fn replace_tilde(part: &str) -> String {
    patterns()
        .tilde
        .replace(part, |captures: &Captures| {
            let (major, minor, patch, prerelease) = (
                group(captures, 1),
                group(captures, 2),
                group(captures, 3),
                group(captures, 4),
            );
            if is_wildcard(major) {
                String::new()
            } else if is_wildcard(minor) {
                format!(">={major}.0.0 <{}.0.0-0", plus_one(major))
            } else if is_wildcard(patch) {
                format!(">={major}.{minor}.0 <{major}.{}.0-0", plus_one(minor))
            } else if !prerelease.is_empty() {
                format!(
                    ">={major}.{minor}.{patch}-{prerelease} <{major}.{}.0-0",
                    plus_one(minor)
                )
            } else {
                format!(">={major}.{minor}.{patch} <{major}.{}.0-0", plus_one(minor))
            }
        })
        .into_owned()
}

fn replace_xrange(part: &str) -> String {
    patterns()
        .xrange
        .replace(trim_like_javascript(part), |captures: &Captures| {
            let mut operator = group(captures, 1);
            let (mut major, mut minor, mut patch) = (
                group(captures, 2).to_string(),
                group(captures, 3).to_string(),
                group(captures, 4).to_string(),
            );
            let any_major = is_wildcard(&major);
            let any_minor = any_major || is_wildcard(&minor);
            let any_patch = any_minor || is_wildcard(&patch);
            if operator == "=" && any_patch {
                operator = "";
            }

            if any_major {
                if operator == ">" || operator == "<" {
                    NULL_SET.to_string()
                } else {
                    "*".to_string()
                }
            } else if !operator.is_empty() && any_patch {
                if any_minor {
                    minor = "0".to_string();
                }
                patch = "0".to_string();
                if operator == ">" {
                    operator = ">=";
                    if any_minor {
                        major = plus_one(&major);
                    } else {
                        minor = plus_one(&minor);
                    }
                } else if operator == "<=" {
                    operator = "<";
                    if any_minor {
                        major = plus_one(&major);
                    } else {
                        minor = plus_one(&minor);
                    }
                }
                let prerelease = if operator == "<" { "-0" } else { "" };
                format!("{operator}{major}.{minor}.{patch}{prerelease}")
            } else if any_minor {
                format!(">={major}.0.0 <{}.0.0-0", plus_one(&major))
            } else if any_patch {
                format!(">={major}.{minor}.0 <{major}.{}.0-0", plus_one(&minor))
            } else {
                captures[0].to_string()
            }
        })
        .into_owned()
}

fn replace_each(text: &str, replace: fn(&str) -> String) -> String {
    patterns()
        .whitespace
        .split(text)
        .map(replace)
        .collect::<Vec<_>>()
        .join(" ")
}

fn desugar_comparator(part: &str) -> String {
    let part = patterns().build.replace(part, "");
    let part = replace_each(trim_like_javascript(&part), replace_caret);
    let part = replace_each(trim_like_javascript(&part), replace_tilde);
    let part = replace_each(&part, replace_xrange);
    patterns()
        .star
        .replace(trim_like_javascript(&part), "")
        .into_owned()
}

fn replace_hyphen(captures: &Captures) -> String {
    let (from, from_major, from_minor, from_patch) = (
        group(captures, 1),
        group(captures, 2),
        group(captures, 3),
        group(captures, 4),
    );
    let (to, to_major, to_minor, to_patch, to_prerelease) = (
        group(captures, 7),
        group(captures, 8),
        group(captures, 9),
        group(captures, 10),
        group(captures, 11),
    );
    let lower = if is_wildcard(from_major) {
        String::new()
    } else if is_wildcard(from_minor) {
        format!(">={from_major}.0.0")
    } else if is_wildcard(from_patch) {
        format!(">={from_major}.{from_minor}.0")
    } else {
        format!(">={from}")
    };
    let upper = if is_wildcard(to_major) {
        String::new()
    } else if is_wildcard(to_minor) {
        format!("<{}.0.0-0", plus_one(to_major))
    } else if is_wildcard(to_patch) {
        format!("<{to_major}.{}.0-0", plus_one(to_minor))
    } else if !to_prerelease.is_empty() {
        format!("<={to_major}.{to_minor}.{to_patch}-{to_prerelease}")
    } else {
        format!("<={to}")
    };
    format!("{lower} {upper}").trim().to_string()
}

fn parse_comparator_set(text: &str) -> Option<Vec<Comparator>> {
    let patterns = patterns();
    let text = patterns.hyphen_range.replace(text, replace_hyphen);
    let text = patterns.comparator_trim.replace_all(&text, "${1}${2}${3}");
    let text = patterns.tilde_trim.replace_all(&text, "${1}~");
    let text = patterns.caret_trim.replace_all(&text, "${1}^");

    let desugared: Vec<String> = text.split(' ').map(desugar_comparator).collect();
    let joined = desugared.join(" ");
    let parsed: Vec<Comparator> = patterns
        .whitespace
        .split(&joined)
        .map(|part| {
            let part = patterns
                .greater_or_equal_zero
                .replace(trim_like_javascript(part), "");
            Comparator::parse(&part)
        })
        .collect::<Option<_>>()?;

    let mut by_value: BTreeMap<String, (usize, Comparator)> = BTreeMap::new();
    for comparator in parsed {
        let value = comparator.value();
        if value == NULL_SET {
            return Some(vec![comparator]);
        }
        let position = by_value.len();
        by_value
            .entry(value)
            .and_modify(|entry| entry.1 = comparator.clone())
            .or_insert((position, comparator));
    }
    if by_value.len() > 1 {
        by_value.remove("");
    }

    let mut comparators: Vec<(usize, Comparator)> = by_value.into_values().collect();
    comparators.sort_by_key(|(position, _)| *position);
    Some(
        comparators
            .into_iter()
            .map(|(_, comparator)| comparator)
            .collect(),
    )
}

#[derive(Debug, Clone)]
pub struct Range {
    sets: Vec<Vec<Comparator>>,
}

impl Range {
    pub fn parse(text: &str) -> Option<Self> {
        let raw = collapse_whitespace(text);
        let mut sets: Vec<Vec<Comparator>> = Vec::new();
        for part in raw.split("||") {
            let set = parse_comparator_set(trim_like_javascript(part))?;
            if !set.is_empty() {
                sets.push(set);
            }
        }
        let first = sets.first()?.clone();

        if sets.len() > 1 {
            sets.retain(|set| set[0].value() != NULL_SET);
            if sets.is_empty() {
                sets = vec![first];
            } else if sets.len() > 1 {
                if let Some(any) = sets
                    .iter()
                    .find(|set| set.len() == 1 && set[0].value().is_empty())
                {
                    sets = vec![any.clone()];
                }
            }
        }
        Some(Self { sets })
    }

    pub fn satisfies(&self, version: &Version) -> bool {
        self.sets.iter().any(|set| set_allows(set, version))
    }
}

fn set_allows(set: &[Comparator], version: &Version) -> bool {
    if !set.iter().all(|comparator| comparator.test(version)) {
        return false;
    }
    !version.is_prerelease()
        || set.iter().any(|comparator| {
            comparator
                .version
                .as_ref()
                .is_some_and(|allowed| allowed.is_prerelease() && allowed.same_release(version))
        })
}
