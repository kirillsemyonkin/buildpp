use std::io;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::configuration;
use crate::configuration::Configuration;
use crate::lsd::Value;
use crate::profile;
use crate::profile::DEFAULT_PROFILE;
use crate::util::BoolGuardExt;
use crate::Dir;
use crate::RunError;

pub struct Subcommand {
    additional_args: Rc<[Value]>,

    profile_name: profile::Name,
}

#[derive(Debug, Clone)]
enum InnerParseError {
    FoundExtraFlags(Rc<[Value]>),

    ProfileHasToHaveExactlyOneValue,
}

impl super::InnerParseError for InnerParseError {
}

impl From<InnerParseError> for Rc<dyn super::InnerParseError> {
    fn from(value: InnerParseError) -> Self { Rc::new(value) }
}

#[derive(Debug, Clone)]
enum InnerExecuteError {
    InvalidCurrentDir(Rc<io::Error>),

    CannotLoadConfiguration(configuration::LoadError),

    RunError(RunError),
}

impl super::InnerExecuteError for InnerExecuteError {
}

impl From<InnerExecuteError> for Rc<dyn super::InnerExecuteError> {
    fn from(value: InnerExecuteError) -> Self { Rc::new(value) }
}

fn parse_profile(profile: Rc<[Value]>) -> Result<Rc<str>, InnerParseError> {
    use InnerParseError::*;

    let mut profile_values = profile.iter();
    let profile = profile_values
        .next()
        .ok_or(ProfileHasToHaveExactlyOneValue)?;
    profile_values
        .next()
        .is_none()
        .ok_or(ProfileHasToHaveExactlyOneValue)?;

    Ok(profile.clone())
}

impl super::Subcommand for Subcommand {
    fn parse(
        mut flags: IndexMap<Value, Rc<[Value]>>,
        post_dash_dash: impl Iterator<Item = String>,
    ) -> Result<Rc<dyn super::Subcommand>, Rc<dyn super::InnerParseError>> {
        use InnerParseError::*;

        let additional_args = post_dash_dash
            .map(Value::from)
            .collect();

        let profile = flags
            .remove("profile")
            .map(parse_profile)
            .transpose()?
            .unwrap_or_else(|| DEFAULT_PROFILE.into());

        let extra_flags = flags.into_keys();
        if extra_flags.len() > 0 {
            return Err(FoundExtraFlags(
                extra_flags
                    .collect::<Vec<_>>()
                    .into(),
            ))?;
        }

        Ok(Rc::new(Subcommand {
            additional_args,
            profile_name: profile,
        }))
    }

    fn execute(&self) -> Result<(), Rc<dyn super::InnerExecuteError>> {
        use InnerExecuteError::*;

        let project_dir = Dir::from(
            std::env::current_dir()
                .map_err(Rc::new)
                .map_err(InvalidCurrentDir)?,
        );

        let config = Configuration::load(project_dir).map_err(CannotLoadConfiguration)?;

        let exit_code = config
            .run(
                self.profile_name
                    .clone(),
                self.additional_args
                    .clone(),
            )
            .map_err(RunError)?;

        std::process::exit(exit_code)
    }
}
