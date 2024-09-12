use std::{fmt::Display, time::Duration};

use futures::{stream::unfold, StreamExt as _};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::UnixStream as TokioUnixStream,
    pin,
    time::Instant,
};

use crate::config::{Field, Format, Scale, Source};

/// Run the client for as long as configured.
pub async fn run(
    mut stream: TokioUnixStream,
    pids: Vec<i32>,
    timeout: Option<Duration>,
    fields: Vec<Field>,
    separator: String,
) -> Result<(), String> {
    for pid in &pids {
        stream
            .write_i32(*pid)
            .await
            .map_err(|e| format!("error writing to server: {e}"))?;
    }
    stream
        .flush()
        .await
        .map_err(|e| format!("error flushing stream: {e}"))?;
    stream
        .shutdown()
        .await
        .map_err(|e| format!("error shutting down stream: {e}"))?;
    let loads_stream = unfold(stream, |mut stream| async {
        stream.read_f32().await.ok().map(|load| (load, stream))
    })
    .chunks(pids.len());
    pin!(loads_stream);
    let deadline = timeout.map(|tmout| Instant::now() + tmout);
    while let Some(loads) = loads_stream.next().await {
        println!(
            "{}",
            OutputLine(&fields, &separator, num_cpus::get(), loads)
        );
        if deadline.is_some_and(|d| Instant::now() > d) {
            break;
        }
    }
    Ok(())
}

struct OutputLine<'a>(&'a Vec<Field>, &'a str, usize, Vec<f32>);

impl<'f> Display for OutputLine<'f> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let OutputLine(spec, sep, num_cores, loads) = self;
        let sum: f32 = loads
            .iter()
            .map(|l| if l.is_nan() { 0.0 } else { *l })
            .sum();
        let mut any_written = false;
        for Field(source, scale, format) in spec.iter() {
            let scale = match scale {
                Scale::OfCore => 1.0,
                Scale::OfTotal => *num_cores as f32,
            };
            let inputs = match source {
                Source::Sum => &vec![sum],
                Source::AllLoads => loads,
            };
            let inputs: Vec<f32> = inputs.iter().map(|i| i / scale).collect();
            for input in inputs {
                if any_written {
                    write!(f, "{sep}")?;
                }
                match format {
                    Format::Float(precision) | Format::Percent(precision) => {
                        let mul = match format {
                            Format::Float(_) => 1.0,
                            Format::Percent(_) => 100.0,
                            _ => panic!(),
                        };
                        write!(f, "{:.1$}", input * mul, *precision as usize)?
                    }
                    Format::IfThenElse {
                        test,
                        then,
                        otherwise,
                    } => {
                        if test.matches(input) {
                            write!(f, "{}", then)?
                        } else {
                            write!(f, "{}", otherwise)?
                        }
                    }
                }
                any_written = true;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Test;

    use super::*;

    #[test]
    fn test() {
        let fields = vec![
            Field(
                Source::Sum,
                Scale::OfCore,
                Format::IfThenElse {
                    test: Test::Range(Some(1.0), None),
                    then: "x".to_owned(),
                    otherwise: "y".to_owned(),
                },
            ),
            Field(
                Source::AllLoads,
                Scale::OfCore,
                Format::IfThenElse {
                    test: Test::Range(None, Some(1.0)),
                    then: "x".to_owned(),
                    otherwise: "y".to_owned(),
                },
            ),
            Field(Source::Sum, Scale::OfTotal, Format::Float(3)),
        ];
        let o = OutputLine(&fields, " ", 3, vec![0.5, 2.0, 3.5]);
        assert_eq!(o.to_string(), "x x y y 2.000");
        let o = OutputLine(&fields, "", 3, vec![0.0, 0.0, 1.5]);
        assert_eq!(o.to_string(), "xxxy0.500");
        let o = OutputLine(&fields, "xxx", 3, vec![]);
        assert_eq!(o.to_string(), "yxxx0.000");
    }
}
