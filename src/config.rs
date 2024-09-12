use std::{ops::Not as _, str::FromStr, time::Duration};

/// Application configuration
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None, after_help = concat!(
    "Explanation of fields\n",
    "\n",
    "Multiple fields can be passed via -f/--field. A basic field can be:\n",
    " * `sum' - sum of loads of all provided process trees,\n",
    " * `all_loads' - produces multiple fields, one for each process tree.\n",
    "\n",
    "The values are scaled per-core, so n means n whole cores are being used.\n",
    "Adding `_t' to either field scales the loads according to the total computing power,\n",
    "1 being the maximum.\n",
    "\n",
    "A format specifier can be added after colon:\n",
    " * .N - prints with N digits after decimal point,\n",
    " * %N - prints with N digits after decimal point, scaled up by a factor of 100,\n",
    " * if_range:[L]..[H]:then[:else] - produces `then` if field value is between `L' and `H',\n",
    "                                   `else` otherwise, `L`, `H` and `else` are optional,\n",
    " * if_greater:thr:then[:else]    - like if_range, but field value must be greater than `thr`,\n",
    "                                   DEPRECATED\n",
    "\n",
    "Additionally, the last two specifiers can be used alone, without a preceding value,\n",
    "in this case, the value defaults to `sum`.",
))]
pub struct Config {
    /// The collection of PIDs to monitor.
    #[arg(name = "pid", required = true, num_args = 1..)]
    pub pids: Vec<i32>,
    /// The maximum time to collect statistics.
    #[arg(short, long, value_parser = parse_timeout_duration)]
    pub timeout: Option<Duration>,
    #[arg(
        name = "field",
        short,
        long,
        help = concat!(
            "sum[_t][:FMT] | all_loads[_t][:FMT] | TEST\n",
            "FMT := .N | %N | TEST\n",
            "TEST := if_range:[L]..[H]:then[:else] | if_greater:thr:then[:else]\n"
        ),
        default_values = ["sum", "all_loads"]
    )]
    /// The list of output fields to print with each update.
    pub fields: Vec<Field>,
    /// The field separator.
    #[arg(short, long, default_value = " ")]
    pub separator: String,
}

fn parse_timeout_duration(arg: &str) -> Result<std::time::Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(std::time::Duration::from_secs(seconds))
}

/// Specification of one or more fields of information to print about a collection of PIDs.
#[derive(Clone, Debug, PartialEq)]
pub struct Field(pub Source, pub Scale, pub Format);

/// Source of load values for a field specification.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// The sum of all process trees' CPU usage as a field
    Sum,
    /// CPU usage of each process tree, one in each field
    AllLoads,
}

/// How to scale load values.
#[derive(Clone, Debug, PartialEq)]
pub enum Scale {
    /// As a fraction of a single core
    OfCore,
    /// As a fraction of the total computing power ([Scale::OfCore], but divided by number of cores)
    OfTotal,
}

/// Formatting specification of a single field.
#[derive(Clone, Debug, PartialEq)]
pub enum Format {
    /// Print load as a floating-point number with a certain precision.
    Float(u8),
    /// Print load as a percent of its base value ([Scale]) with a certain precision.
    Percent(u8),
    /// Print one string when load is above a threshold, or a different string otherwise.
    IfThenElse {
        /// The test.
        test: Test,
        /// String to be printed when CPU usage above threshold.
        then: String,
        /// String to be printed otherwise.
        otherwise: String,
    },
}

impl Default for Format {
    fn default() -> Self {
        Self::Float(2)
    }
}

/// A condition to evaluate on an input value.
#[derive(Clone, Debug, PartialEq)]
pub enum Test {
    /// Test if value is in range.
    ///
    /// Evaluates to true iff value is in given range, left-inclusive, right-exclusive. If either
    /// boundar is `None`, this boundary is not tested.
    Range(Option<f32>, Option<f32>),
}

impl Test {
    pub fn matches(&self, value: f32) -> bool {
        match &self {
            Test::Range(lo, hi) => {
                lo.map_or(true, |lo| lo <= value) && hi.map_or(true, |hi| value < hi)
            }
        }
    }
}

impl FromStr for Field {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut tokens = value.splitn(2, ':');
        let field = tokens
            .next()
            .expect("splitn should produce at least 1 elment");
        match field {
            "" => Err("missing field name")?,
            "sum" | "all_loads" | "sum_t" | "all_loads_t" => {
                let (source, scale) = match field {
                    "sum" => (Source::Sum, Scale::OfCore),
                    "sum_t" => (Source::Sum, Scale::OfTotal),
                    "all_loads" => (Source::AllLoads, Scale::OfCore),
                    "all_loads_t" => (Source::AllLoads, Scale::OfTotal),
                    _ => panic!(),
                };
                let format = tokens
                    .next()
                    .map(parse_format)
                    .transpose()?
                    .unwrap_or_default();
                Ok(Field(source, scale, format))
            }
            "if_range" | "if_greater" => {
                let args = tokens
                    .next()
                    .ok_or(format!("missing arguments to {field}"))?;
                Ok(Field(
                    Source::Sum,
                    Scale::OfCore,
                    parse_test_format(field, args)?,
                ))
            }
            _ => Err(format!("unrecognized field {field}"))?,
        }
    }
}

fn parse_format(s: &str) -> Result<Format, String> {
    let mut tokens = s.splitn(2, ':');
    let field = tokens
        .next()
        .expect("splitn should produce at least 1 elment");
    match field {
        "if_range" | "if_greater" => {
            let args = tokens
                .next()
                .ok_or(format!("missing arguments to {field}"))?;
            parse_test_format(field, args)
        }
        numeric => {
            let prefix = numeric
                .get(..1)
                .ok_or_else(|| format!("unrecognized format specifier `{numeric}`"))?;
            let digits = numeric
                .get(1..)
                .expect("rest should exist")
                .parse()
                .map_err(|e| format!("cannot parse precision: {e}"));
            match prefix {
                "." => Ok(Format::Float(digits?)),
                "%" => Ok(Format::Percent(digits?)),
                _ => Err(format!("unrecognized format specifier `{numeric}`")),
            }
        }
    }
}

fn parse_test_format(format: &str, args: &str) -> Result<Format, String> {
    let mut tokens = args.splitn(3, ':');
    let test = tokens
        .next()
        .expect("there should be at least a threshold/range field");
    let test = match format {
        "if_greater" => {
            let threshold = test
                .parse()
                .map_err(|e| format!("wrong threshold format: {e}"))?;
            Test::Range(Some(threshold), None)
        }
        "if_range" => test
            .parse()
            .map_err(|e| format!("wrong range format: {e}"))?,
        _ => panic!("bad format"),
    };
    let then = tokens.next().ok_or("missing then-clause")?.to_owned();
    let otherwise = tokens.next().unwrap_or_default().to_owned();
    Ok(Format::IfThenElse {
        test,
        then,
        otherwise,
    })
}

impl FromStr for Test {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (lo, hi) = value
            .split_once("..")
            .ok_or("must be in format [lo]..[hi]")?;
        let lo = lo
            .is_empty()
            .not()
            .then(|| lo.parse().map_err(|e| format!("bad low value: {e}")))
            .transpose()?;
        let hi = hi
            .is_empty()
            .not()
            .then(|| hi.parse().map_err(|e| format!("bad high value: {e}")))
            .transpose()?;
        Ok(Self::Range(lo, hi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_full_range() {
        let t = Test::Range(Some(1.0), Some(2.0));
        assert!(!t.matches(0.5));
        assert!(t.matches(1.0));
        assert!(t.matches(1.5));
        assert!(!t.matches(2.0));
        assert!(!t.matches(2.5));
    }

    #[test]
    fn test_matches_partial_range() {
        let t = Test::Range(None, Some(2.0));
        assert!(t.matches(0.5));
        assert!(t.matches(1.0));
        assert!(t.matches(1.5));
        assert!(!t.matches(2.0));
        assert!(!t.matches(2.5));

        let t = Test::Range(Some(1.0), None);
        assert!(!t.matches(0.5));
        assert!(t.matches(1.0));
        assert!(t.matches(1.5));
        assert!(t.matches(2.0));
        assert!(t.matches(2.5));
    }

    #[test]
    fn test_matches_degenerated() {
        let t = Test::Range(None, None);
        assert!(t.matches(0.5));
        assert!(t.matches(1.0));
        assert!(t.matches(1.5));
        assert!(t.matches(2.0));
        assert!(t.matches(2.5));

        let t = Test::Range(Some(1.0), Some(1.0));
        assert!(!t.matches(0.5));
        assert!(!t.matches(1.0));
        assert!(!t.matches(1.5));

        let t = Test::Range(Some(2.0), Some(1.0));
        assert!(!t.matches(0.5));
        assert!(!t.matches(1.0));
        assert!(!t.matches(1.5));
    }

    #[test]
    fn fails_to_parse_bad() {
        let f: Result<Field, _> = "bad".parse();
        assert!(f.is_err());

        let f: Result<Field, _> = "".parse();
        assert!(f.is_err());

        let f: Result<Field, _> = "if_greater".parse();
        assert!(f.is_err());
        let f: Result<Field, _> = "if_greater:".parse();
        assert!(f.is_err());
        let f: Result<Field, _> = "if_greater:abc".parse();
        assert!(f.is_err());
        let f: Result<Field, _> = "if_greater:13".parse();
        assert!(f.is_err());
    }

    #[test]
    fn parses_simple() {
        for (spec, field) in [
            ("sum", Field(Source::Sum, Scale::OfCore, Format::Float(2))),
            (
                "all_loads",
                Field(Source::AllLoads, Scale::OfCore, Format::Float(2)),
            ),
        ] {
            let f: Field = spec.parse().unwrap();
            assert_eq!(f, field);
            let f: Result<Field, _> = format!("{spec}:sth").parse();
            assert!(f.is_err());
        }
    }

    #[test]
    fn parses_if_greater() {
        let f: Field = "if_greater:3:then".parse().unwrap();
        let Field(
            Source::Sum,
            Scale::OfCore,
            Format::IfThenElse {
                test: Test::Range(Some(value), None),
                then,
                otherwise,
            },
        ) = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "then");
        assert_eq!(otherwise, "");

        let f: Field = "if_greater:3:then:".parse().unwrap();
        let Field(
            Source::Sum,
            Scale::OfCore,
            Format::IfThenElse {
                test: Test::Range(Some(value), None),
                then,
                otherwise,
            },
        ) = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "then");
        assert_eq!(otherwise, "");

        let f: Field = "if_greater:3:then:x".parse().unwrap();
        let Field(
            Source::Sum,
            Scale::OfCore,
            Format::IfThenElse {
                test: Test::Range(Some(value), None),
                then,
                otherwise,
            },
        ) = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "then");
        assert_eq!(otherwise, "x");

        let f: Field = "if_greater:3:then::".parse().unwrap();
        let Field(
            Source::Sum,
            Scale::OfCore,
            Format::IfThenElse {
                test: Test::Range(Some(value), None),
                then,
                otherwise,
            },
        ) = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "then");
        assert_eq!(otherwise, ":");

        let f: Field = "if_greater:3:".parse().unwrap();
        let Field(
            Source::Sum,
            Scale::OfCore,
            Format::IfThenElse {
                test: Test::Range(Some(value), None),
                then,
                otherwise,
            },
        ) = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "");
        assert_eq!(otherwise, "");
    }

    #[test]
    fn parses_all_loads() {
        let f: Field = "all_loads".parse().unwrap();
        assert!(matches!(
            f,
            Field(Source::AllLoads, Scale::OfCore, Format::Float(2))
        ));

        let f: Field = "all_loads:.3".parse().unwrap();
        assert!(matches!(
            f,
            Field(Source::AllLoads, Scale::OfCore, Format::Float(3))
        ));

        let f: Field = "all_loads:%0".parse().unwrap();
        assert!(matches!(
            f,
            Field(Source::AllLoads, Scale::OfCore, Format::Percent(0))
        ));

        let f: Result<Field, _> = "all_loads:%0d".parse();
        assert!(f.is_err());

        let f: Field = "all_loads:if_greater:2.0:x:y::".parse().unwrap();
        let Field(
            Source::AllLoads,
            Scale::OfCore,
            Format::IfThenElse {
                test: Test::Range(Some(value), None),
                then,
                otherwise,
            },
        ) = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 2.0);
        assert_eq!(then, "x");
        assert_eq!(otherwise, "y::");
    }
}
