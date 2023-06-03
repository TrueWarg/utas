use configparser::ini::Ini;
use const_format::concatcp;
use indexmap::{map::Entry, IndexMap};
use lazy_static::lazy_static;
use regex::{Captures, Match, Regex};
use std::{borrow::Cow, fmt::format, path::Path};

#[derive(Debug)]
pub struct File {
    pub sections: Vec<Section>,
}

#[derive(Debug)]
pub struct Section {
    pub keys: Vec<Key>,
}

/// Represents a string resource key with its localizations
#[derive(Debug)]
pub struct Key {
    pub name: String,
    pub localizations: Vec<LocalizedString>,
}

#[derive(Debug)]
pub struct LocalizedString {
    pub language_code: String,
    pub value: StringValue,
}

#[derive(Debug)]
pub enum StringValue {
    Single(String),
    Plural { quantities: Vec<PluralValue> },
}

#[derive(Debug, PartialEq)]
pub struct PluralValue {
    /// quantity can be: "zero", "one", "two", "few", "many", and "other"
    quantity: String,
    text: String,
}

pub fn parse<T: AsRef<Path>>(path: T) -> Result<File, String> {
    let mut config = Ini::new_cs();
    let map = config.load(path)?;
    // NOTE: twine has this structure
    // [[Section1]]
    // [subsection1]
    //   key1 = value1
    //   key2 = value2
    // [[Section2]]
    // [subsection1]
    //   key1 = value1
    //   key2 = value2
    // but configparser lib will ignore [[SectionX]] sections (see https://github.com/QEDK/configparser-rs/issues/37),
    // so here we will only see [subsection1, subsection2] returned by `config.sections()` and these will be
    // string resource keys.
    // We still will create a single "twine-section" struct in hopes of a future issue fix (seen above), then we'll
    // be able to group "subsections" in "twine-section".
    let mut section = Section {
        keys: Vec::with_capacity(map.len()),
    };
    // Parses
    // [login_screen_title]
    // en = Login
    // ru = Логин
    for (resource_key_name, localizations) in map {
        let key = key_from_locale_value_map(resource_key_name, localizations)?;
        section.keys.push(key);
    }
    Ok(File {
        // For now only supporting a single section, see the comment above
        sections: vec![section],
    })
}

const PLACEHOLDER_FLAGS_WIDTH_PRECISION_LENGTH: &str =
    r"([-+0#,])?(\d+|\*)?(\.(\d+|\*))?(hh?|ll?|L|z|j|t|q)?";
const PLACEHOLDER_PARAMETER_FLAGS_WIDTH_PRECISION_LENGTH: &str =
    concatcp!(r"(\d+\$)?", PLACEHOLDER_FLAGS_WIDTH_PRECISION_LENGTH);
const PLACEHOLDER_TYPES: &str = "[diufFeEgGxXoscpaA@]";
const PLACEHOLDER_REGEX: &str = concatcp!(
    "%",
    PLACEHOLDER_PARAMETER_FLAGS_WIDTH_PRECISION_LENGTH,
    PLACEHOLDER_TYPES
);
const NON_NUMBERED_PLACEHOLDER_REGEX: &str = concatcp!(
    "%(",
    PLACEHOLDER_FLAGS_WIDTH_PRECISION_LENGTH,
    PLACEHOLDER_TYPES,
    ")"
);
const SINGLE_PERCENT_REGEX: &str = r"([^%][%][^%]|[^%][%]$|^[%]$)";

fn key_from_locale_value_map(
    name: String,
    raw_localizations: IndexMap<String, Option<String>>,
) -> Result<Key, String> {
    if raw_localizations.keys().any(|l| l.contains(':')) {
        key_from_locale_plural_value_map(name, raw_localizations)
    } else {
        key_from_locale_single_value_map(name, raw_localizations)
    }
}

fn key_from_locale_single_value_map(
    name: String,
    raw_localizations: IndexMap<String, Option<String>>,
) -> Result<Key, String> {
    let mut localizations: Vec<LocalizedString> = Vec::with_capacity(raw_localizations.len());
    for (locale_name, string_value_opt) in raw_localizations {
        let Some(string_value) = string_value_opt else {
            println!("skipped key \"{}\" because it's empty", locale_name);
            continue;
        };
        let loc_str = LocalizedString {
            language_code: locale_name,
            value: StringValue::Single(parse_localized_string_value(string_value)?),
        };
        localizations.push(loc_str)
    }
    let key = Key {
        name,
        localizations,
    };
    Ok(key)
}

fn key_from_locale_plural_value_map(
    name: String,
    raw_localizations: IndexMap<String, Option<String>>,
) -> Result<Key, String> {
    let mut localizations: IndexMap<String, LocalizedString> =
        IndexMap::with_capacity(raw_localizations.len());
    for (locale_name_and_quantity, string_value_opt) in raw_localizations {
        let Some(string_value) = string_value_opt else {
            println!("skipped key \"{}\" because it's empty", locale_name_and_quantity);
            continue;
        };
        let Some((locale_name, quantity)) = locale_name_and_quantity.split_once(':') else {
            println!("skipped key \"{}\" because can't split into locale and quantity", locale_name_and_quantity);
            continue;
        };
        let entry = localizations
            .entry(locale_name.to_string())
            .or_insert(LocalizedString {
                language_code: locale_name.to_string(),
                value: StringValue::Plural {
                    quantities: Vec::new(),
                },
            });
        let loc_str_value = &mut entry.value;
        let StringValue::Plural { quantities } = loc_str_value else {
            continue;
        };
        quantities.push(PluralValue {
            quantity: quantity.to_string(),
            text: parse_localized_string_value(string_value)?,
        });
    }
    let key = Key {
        name,
        localizations: localizations.into_iter().map(|(_, value)| value).collect(),
    };
    Ok(key)
}

fn parse_localized_string_value(raw_value: String) -> Result<String, String> {
    lazy_static! {
        static ref PLACEHOLDER_REGEX_RE: Regex = Regex::new(PLACEHOLDER_REGEX).unwrap();
    }
    let mut value = raw_value;
    value = maybe_escape_characters(&value).to_string();
    value = maybe_replace_single_percent_with_double_percent(&value).to_string();
    if !PLACEHOLDER_REGEX_RE.is_match(&value) {
        return Ok(value);
    }
    value = convert_twine_string_placeholder(&value).to_string();
    value = maybe_add_positional_numbers(&value).to_string();
    Ok(value)
}

fn convert_twine_string_placeholder(raw_value: &str) -> Cow<str> {
    lazy_static! {
        static ref TWINE_STRING_REPLACE_REGEX: Regex = Regex::new(
            format!(
                r"%({})@",
                PLACEHOLDER_PARAMETER_FLAGS_WIDTH_PRECISION_LENGTH
            )
            .as_str()
        )
        .unwrap();
    }
    // TODO @dz @Parse avoid allocating new string if there's no match
    TWINE_STRING_REPLACE_REGEX.replace_all(&raw_value, r"%${1}s")
}

fn maybe_add_positional_numbers(input: &str) -> Cow<str> {
    lazy_static! {
        static ref NON_NUMBERED_PLACEHOLDER_REGEX_RE: Regex =
            Regex::new(NON_NUMBERED_PLACEHOLDER_REGEX).unwrap();
    }
    let non_numbered_count = NON_NUMBERED_PLACEHOLDER_REGEX_RE.find_iter(&input).count();
    if non_numbered_count <= 1 {
        return Cow::from(input);
    }
    let mut i = 0;
    NON_NUMBERED_PLACEHOLDER_REGEX_RE.replace_all(&input, |caps: &Captures| {
        i += 1;
        format!("%{}${}", i, &caps[1])
    })
}

fn maybe_replace_single_percent_with_double_percent(input: &str) -> Cow<str> {
    lazy_static! {
        static ref SINGLE_PERCENT_REGEX_RE: Regex = Regex::new(SINGLE_PERCENT_REGEX).unwrap();
        static ref PLACEHOLDER_REGEX_RE: Regex = Regex::new(PLACEHOLDER_REGEX).unwrap();
    }
    // Regex crate doesn't support negative lookahead which is used in
    // twine/placholder.rb for this case, so something else is invented here.
    // - use two Regexes: r1 = SINGLE_PERCENT_REGEX, r2 = PLACEHOLDER_REGEX
    // - iterate the matches of r1 and use r2.find_at(match) == match.start
    //   to see if this is a placholder-match
    // - if it is not a placeholder match, then it is a percent match,
    SINGLE_PERCENT_REGEX_RE.replace_all(input, |caps: &Captures| {
        let whole_match = caps.get(0).unwrap();
        // NOTE "percent match" can have first character not exactly being "%", for example
        // for "100% hello" it will be "% ".
        // So additional index adjustement is needed to correctly compare with "placeholder match" start
        let start = percent_start(&whole_match);
        let is_placeholder =
            matches!(PLACEHOLDER_REGEX_RE.find_at(input, start), Some(m) if m.start() == start);
        if is_placeholder {
            whole_match.as_str().to_string()
        } else {
            whole_match.as_str().replace('%', "%%")
        }
    })
}

fn percent_start(m: &Match) -> usize {
    m.start() + m.as_str().find('%').unwrap()
}

fn maybe_escape_characters(input: &str) -> Cow<str> {
    let needs_escaping = input.contains("&") || input.contains("<");
    if needs_escaping {
        Cow::Owned(input.replace("&", "&amp;").replace("<", "&lt;"))
    } else {
        Cow::Borrowed(input)
    }
}

#[test]
fn parses_simple_string() {
    let input = "Lorem ipsum".to_string();
    let result = parse_localized_string_value(input).unwrap();
    assert_eq!(result, "Lorem ipsum".to_string());
}

#[test]
fn parses_single_placeholder() {
    let input = "Lorem %d ipsum".to_string();
    let result = parse_localized_string_value(input).unwrap();
    assert_eq!(result, "Lorem %d ipsum",);
}

#[test]
fn parses_single_string_placeholder() {
    let input = "Lorem %@ ipsum".to_string();
    let result = parse_localized_string_value(input).unwrap();
    assert_eq!(result, "Lorem %s ipsum".to_string(),);
}

#[test]
fn parses_multiple_placeholders() {
    let input = "Lorem %@ ipsum %.2f sir %,d amet %%".to_string();
    let result = parse_localized_string_value(input).unwrap();
    assert_eq!(result, "Lorem %1$s ipsum %2$.2f sir %3$,d amet %%");
}

#[test]
fn parses_multiple_placeholders_keeping_order_if_present() {
    let input = "Lorem %3$@ ipsum %1$.2f sir %2$,d amet".to_string();
    let result = parse_localized_string_value(input).unwrap();
    assert_eq!(result, "Lorem %3$s ipsum %1$.2f sir %2$,d amet",);
}

#[test]
fn parses_html_tags_and_related_characters_with_proper_escaping() {
    let input = "У нас было <b>38</b> попугаев в <i>чистой</i> упаковке, на которой было указано: 38 < 89 && 88 >= 55".to_string();
    let result = parse_localized_string_value(input).unwrap();
    assert_eq!(result, "У нас было &lt;b>38&lt;/b> попугаев в &lt;i>чистой&lt;/i> упаковке, на которой было указано: 38 &lt; 89 &amp;&amp; 88 >= 55");
}

#[test]
fn replaces_percent_with_double_percent() {
    let input =
        "100% Lorem %@ ipsum %.2f 20% sir %d amet 8% and %% untouched, ending with 42%".to_string();
    let result = parse_localized_string_value(input).unwrap();
    assert_eq!(
        result,
        "100%% Lorem %1$s ipsum %2$.2f 20%% sir %3$d amet 8%% and %% untouched, ending with 42%%"
    );
}

#[test]
fn replaces_percent_with_double_percent_wihout_placeholders() {
    let input = "100% Lorem ipsum amet 8% and %% untouched, ending with 42%".to_string();
    let result = parse_localized_string_value(input).unwrap();
    assert_eq!(
        result,
        "100%% Lorem ipsum amet 8%% and %% untouched, ending with 42%%"
    );
}

#[test]
fn parses_plural_form_keys() {
    let mut input = IndexMap::new();
    input.insert(
        "en:one".to_string(),
        Some("%d ruble %d bear and 1 vodka".to_string()),
    );
    input.insert(
        "en:many".to_string(),
        Some("%d rubles %d bears and 1 vodka".to_string()),
    );
    input.insert(
        "ru:one".to_string(),
        Some("%d рубль %d медведь и 1 водка".to_string()),
    );
    input.insert(
        "ru:zero".to_string(),
        Some("нет рублей нет медведей и 1 водка".to_string()),
    );
    input.insert(
        "ru:other".to_string(),
        Some("много рублей много медведей и 2 водки".to_string()),
    );
    let result = key_from_locale_value_map("receipt_example".to_string(), input).unwrap();
    let loc = result.localizations;

    assert_eq!(loc.len(), 2);
    assert_eq!(loc[0].language_code, "en".to_string());
    match &loc[0].value {
        StringValue::Plural { quantities } => {
            assert_eq!(
                quantities[0],
                PluralValue {
                    quantity: "one".to_string(),
                    text: "%1$d ruble %2$d bear and 1 vodka".to_string()
                }
            );
            assert_eq!(
                quantities[1],
                PluralValue {
                    quantity: "many".to_string(),
                    text: "%1$d rubles %2$d bears and 1 vodka".to_string()
                }
            )
        }
        StringValue::Single(_) => panic!("expected plural value"),
    }
    assert_eq!(loc[1].language_code, "ru".to_string());
    match &loc[1].value {
        StringValue::Plural { quantities } => {
            assert_eq!(
                quantities[0],
                PluralValue {
                    quantity: "one".to_string(),
                    text: "%1$d рубль %2$d медведь и 1 водка".to_string()
                }
            );
            assert_eq!(
                quantities[1],
                PluralValue {
                    quantity: "zero".to_string(),
                    text: "нет рублей нет медведей и 1 водка".to_string()
                }
            );
            assert_eq!(
                quantities[2],
                PluralValue {
                    quantity: "other".to_string(),
                    text: "много рублей много медведей и 2 водки".to_string()
                }
            )
        }
        StringValue::Single(_) => panic!("expected plural value"),
    }
}
