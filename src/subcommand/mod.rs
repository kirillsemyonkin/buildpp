use std::fmt::Debug;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::lsd::Value;
use crate::util::BoolGuardExt;

mod build;
mod help;
mod new;
mod run;
mod version;

#[derive(Debug, Clone)]
pub enum Error {
    ParseRepeatedFlag,
    ParseUnexpectedFlagValueBeforeAnyFlags(Value),
    ParseInvalidSubcommand(Value),
    ParseInner(Rc<dyn InnerParseError>),

    ExecuteInner(Rc<dyn InnerExecuteError>),
}

impl From<Rc<dyn InnerParseError>> for Error {
    fn from(value: Rc<dyn InnerParseError>) -> Self { Self::ParseInner(value) }
}

impl From<Rc<dyn InnerExecuteError>> for Error {
    fn from(value: Rc<dyn InnerExecuteError>) -> Self { Self::ExecuteInner(value) }
}

//
// Parse
//

pub fn parse_and_execute(
    mut pre_dash_dash: impl Iterator<Item = String>,
    post_dash_dash: impl Iterator<Item = String>,
) -> Result<(), Error> {
    use Error::*;

    // grab subcommand name (can be with --, -, /)
    let original_subcommand = pre_dash_dash.next();
    let subcommand = original_subcommand
        .as_ref()
        .map(|s| {
            s.trim_start_matches("--")
                .trim_start_matches("-")
                .trim_start_matches("/")
                .to_lowercase()
        });
    let subcommand = subcommand
        .as_ref()
        .map(String::as_str);

    // parse flags
    let mut flags = IndexMap::new();
    for arg in pre_dash_dash {
        match /* arg.starts_with("--") || */ arg.starts_with("-") || arg.starts_with("/") {
            true => {
                let flag = arg
                    .trim_start_matches("--")
                    .trim_start_matches("-")
                    .trim_start_matches("/")
                    .to_lowercase();

                let old = flags.insert(flag, Vec::new());
                old.is_none().ok_or(ParseRepeatedFlag)?;
            },
            false => {
                let arg = Value::from(arg);
                let (_, last_flag_values) = flags
                    .last_mut()
                    .ok_or(ParseUnexpectedFlagValueBeforeAnyFlags(arg.clone()))?;
                last_flag_values.push(arg);
            },
        }
    }
    let flags = flags
        .into_iter()
        .map(|(flag, values)| {
            (
                flag.into(),
                Rc::from(values.as_slice()),
            )
        })
        .collect();

    // parse subcommand
    let subcommand = match subcommand {
        // Add more implementations here...
        None | Some("help") | Some("h") => help::Subcommand::parse(flags, post_dash_dash)?,
        Some("version") | Some("ver") | Some("v") =>
            version::Subcommand::parse(flags, post_dash_dash)?,
        Some("build") | Some("b") => build::Subcommand::parse(flags, post_dash_dash)?,
        Some("run") | Some("r") => run::Subcommand::parse(flags, post_dash_dash)?,
        Some("new") | Some("n") | Some("create") | Some("c") =>
            new::Subcommand::parse(flags, post_dash_dash)?,

        Some(_) =>
            return Err(ParseInvalidSubcommand(
                original_subcommand
                    // optionality of original_subcommand matches optionality of subcommand
                    .unwrap()
                    .into(),
            ))?,
    };

    subcommand.execute()?;

    Ok(())
}

//
// Subcommand
//

pub trait InnerParseError: Debug {}

pub trait InnerExecuteError: Debug {}

trait Subcommand {
    fn parse(
        flags: IndexMap<Value, Rc<[Value]>>,
        post_dash_dash: impl Iterator<Item = String>,
    ) -> Result<Rc<dyn Subcommand>, Rc<dyn InnerParseError>>
    where
        Self: Sized;

    fn execute(&self) -> Result<(), Rc<dyn InnerExecuteError>>;
}
