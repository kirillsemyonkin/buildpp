mod msvc;
mod nvcc;

use std::io;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::configuration::Configuration;
use crate::key;
use crate::lsd::LSDGetExt;
use crate::lsd::Level;
use crate::lsd::Map;
use crate::lsd::Value;
use crate::lsd::LSD;
use crate::BuildType;

pub type Name = Value;

pub const DEFAULT_PROFILE: &str = "default";

#[derive(Debug, Clone)]
pub enum ParseError {
    CouldNotFindMatchingCompiler,

    InheritingFromNonExistentProfile(Value),
    InheritIsNotAValue,

    MissingProfileType,
    ProfileTypeIsNotAValue,

    InvalidValueForKey(&'static str),
}

pub fn parse_all(level: Level) -> Result<Map<Name, Rc<dyn Profile>>, Vec<ParseError>> {
    let mut profiles = IndexMap::new();
    let mut profiles_errors = Vec::new();

    for (key, profile_lsd) in level.iter() {
        match parse_one(&profiles, profile_lsd.clone()) {
            Ok(compiler) => drop(profiles.insert(key.clone(), compiler)),
            Err(err) => profiles_errors.push(err),
        }
    }

    if profiles_errors.is_empty() {
        Ok(Map::new(profiles))
    } else {
        Err(profiles_errors)
    }
}

fn parse_one(
    profiles: &IndexMap<Name, Rc<dyn Profile>>,
    entry: LSD,
) -> Result<Rc<dyn Profile>, ParseError> {
    use ParseError::*;
    match entry {
        LSD::Level(level) => {
            // Try inheriting
            if let Some(inherit) = level.get_value(
                key!("inherit"),
                InheritIsNotAValue,
            )? {
                let profile = profiles
                    .get(&inherit)
                    .ok_or(InheritingFromNonExistentProfile(inherit))?;
                return profile.inherit_with(level);
            }

            // No inherit, base profile, check profile type (`is`)
            let is = level
                .get_value(
                    key!(is),
                    ProfileTypeIsNotAValue,
                )?
                .ok_or(MissingProfileType)?;

            match is
                .to_lowercase()
                .as_str()
            {
                // Add more implementations here...
                "nvcc" | "cuda" => nvcc::Profile::create_default().inherit_with(level),
                "msvc" => msvc::Profile::create_default().inherit_with(level),
                _ => Err(CouldNotFindMatchingCompiler),
            }
        },

        // Profile is just type without extra options, make_default
        LSD::Value(value) => match value
            .to_lowercase()
            .as_str()
        {
            // Add more implementations here...
            "nvcc" | "cuda" => Ok(nvcc::Profile::create_default()),
            "msvc" => Ok(msvc::Profile::create_default()),
            // TODO allow inline inherit too
            _ => Err(CouldNotFindMatchingCompiler),
        },
    }
}

pub trait Profile {
    // parse

    fn create_default() -> Rc<dyn Profile>
    where
        Self: Sized;

    fn apply(&mut self, level: Level) -> Result<(), ParseError>;

    fn inherit_with(&self, level: Level) -> Result<Rc<dyn Profile>, ParseError>;

    // pre-build

    fn src_file_suffix(&self) -> &'static str;

    // build

    fn artifact_prefix(&self, build_type: BuildType) -> &'static str;

    fn artifact_suffix(&self, build_type: BuildType) -> &'static str;

    fn compiler_command(&self) -> &str;

    fn compiler_arguments(
        &self,
        config: &Configuration,
        build_type: BuildType,
        selected_profile: &str,
    ) -> Result<Vec<Value>, io::Error>;

    // TODO gnu_cpp::Profile
    // TODO clang::Profile
}
