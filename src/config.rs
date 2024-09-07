use std::{str::FromStr, time::Duration};

/// Application configuration
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
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
        help = "sum | all_loads | if_greater:value:then[:else]",
        default_values = ["sum", "all_loads"]
    )]
    /// The list of output fields to print with each update.
    pub fields: Vec<Field>,
}

/// Specification of one field of information to print about a collection of PIDs.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    /// Print the sum of all process trees' CPU usage.
    Sum,
    /// Print CPU usage of each process tree.
    AllLoads,
    /// Print one string when total CPU usage of all process trees is above a threshold, or a
    /// different string otherwise.
    IfGreater {
        /// The threshold.
        value: f32,
        /// String to be printed when CPU usage above threshold.
        then: String,
        /// String to be printed otherwise.
        otherwise: String,
    },
}

impl FromStr for Field {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut tokens = value.splitn(2, ':');
        let field = tokens.next().expect("should produce at least 1 elment");
        match field {
            "" => Err("missing field name")?,
            "sum" => tokens
                .next()
                .is_none()
                .then_some(Ok(Field::Sum))
                .ok_or("extraneous arguments to sum")?,
            "all_loads" => tokens
                .next()
                .is_none()
                .then_some(Ok(Field::AllLoads))
                .ok_or("extraneous arguments to all_loads")?,
            "if_greater" => {
                let mut tokens = tokens.next().ok_or("missing value")?.splitn(3, ':');
                let value: f32 = tokens
                    .next()
                    .expect("there should be at least a value field")
                    .parse()
                    .map_err(|e| format!("wrong value: {e}"))?;
                let then = tokens.next().ok_or("missing then-clause")?.to_owned();
                let otherwise = tokens.next().unwrap_or_default().to_owned();
                Ok(Field::IfGreater {
                    value,
                    then,
                    otherwise,
                })
            }
            _ => Err(format!("unknown field {field}"))?,
        }
    }
}

fn parse_timeout_duration(arg: &str) -> Result<std::time::Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(std::time::Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fails_to_parse_bad() {
        let f: Result<Field, _> = "bad".parse();
        assert!(f.is_err());

        let f: Result<Field, _> = "".parse();
        assert!(f.is_err());
    }

    #[test]
    fn parses_simple() {
        for (spec, field) in [("sum", Field::Sum), ("all_loads", Field::AllLoads)] {
            let f: Field = spec.parse().unwrap();
            assert_eq!(f, field);
            let f: Result<Field, _> = format!("{spec}:sth").parse();
            assert!(f.is_err());
        }
    }

    #[test]
    fn parses_if_greater() {
        let f: Field = "if_greater:3:then".parse().unwrap();
        let Field::IfGreater {
            value,
            then,
            otherwise,
        } = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "then");
        assert_eq!(otherwise, "");

        let f: Field = "if_greater:3:then:".parse().unwrap();
        let Field::IfGreater {
            value,
            then,
            otherwise,
        } = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "then");
        assert_eq!(otherwise, "");

        let f: Field = "if_greater:3:then:x".parse().unwrap();
        let Field::IfGreater {
            value,
            then,
            otherwise,
        } = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "then");
        assert_eq!(otherwise, "x");

        let f: Field = "if_greater:3:then::".parse().unwrap();
        let Field::IfGreater {
            value,
            then,
            otherwise,
        } = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "then");
        assert_eq!(otherwise, ":");

        let f: Field = "if_greater:3:".parse().unwrap();
        let Field::IfGreater {
            value,
            then,
            otherwise,
        } = f
        else {
            panic!("should parse");
        };
        assert_eq!(value, 3.0);
        assert_eq!(then, "");
        assert_eq!(otherwise, "");

        let f: Result<Field, _> = "if_greater:3".parse();
        assert!(f.is_err())
    }
}
