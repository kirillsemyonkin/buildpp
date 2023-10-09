use std::io;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::configuration;
use crate::configuration::Configuration;
use crate::lsd::Value;
use crate::profile;
use crate::profile::DEFAULT_PROFILE;
use crate::util::BoolGuardExt;
use crate::BuildError;
use crate::BuildType;
use crate::Dir;

pub struct Subcommand {
    build_type: Option<BuildType>,

    profile: profile::Name,
}

#[derive(Debug, Clone)]
enum InnerParseError {
    FoundExtraFlags(Rc<[Value]>),

    BuildTypeHasToHaveExactlyOneValue,
    UnknownBuildType,

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

    BuildError(BuildError),
}

impl super::InnerExecuteError for InnerExecuteError {
}

impl From<InnerExecuteError> for Rc<dyn super::InnerExecuteError> {
    fn from(value: InnerExecuteError) -> Self { Rc::new(value) }
}

fn parse_build_type(build_type: Rc<[Value]>) -> Result<BuildType, InnerParseError> {
    use InnerParseError::*;

    let mut build_type_values = build_type.iter();
    let build_type = build_type_values
        .next()
        .ok_or(BuildTypeHasToHaveExactlyOneValue)?;
    build_type_values
        .next()
        .is_none()
        .ok_or(BuildTypeHasToHaveExactlyOneValue)?;

    Ok(build_type
        .parse()
        .map_err(|()| UnknownBuildType)?)
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
        _post_dash_dash: impl Iterator<Item = String>,
    ) -> Result<Rc<dyn super::Subcommand>, Rc<dyn super::InnerParseError>> {
        use InnerParseError::*;

        let build_type = flags
            .remove("is")
            .map(parse_build_type)
            .transpose()?;

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
            build_type,
            profile,
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

        config
            .build(self.build_type, &self.profile)
            .map_err(BuildError)?;

        Ok(())
    }
}
