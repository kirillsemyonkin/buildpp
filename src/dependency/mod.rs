mod local_build;
mod local_pair;

use std::fmt::Debug;
use std::io;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::key;
use crate::lsd::LSDGetExt;
use crate::lsd::Level;
use crate::lsd::Map;
use crate::lsd::Value;
use crate::lsd::LSD;
use crate::profile;
use crate::util::SplitIntoTwoWordsExt;
use crate::BuildError;
use crate::Dir;
use crate::Version;

pub type Alias = Value;

//
// Parse
//

#[derive(Debug, Clone)]
pub enum ParseError {
    CouldNotFindMatchingDependencyType,
    DependencyTypeIsNotAValue,

    Inner(Rc<dyn InnerParseError>),

    // TODO will have a default type for remote or smt
    DependenciesWithoutTypeAreNotSupportedYet,
    DependenciesAsVersionsAreNotSupportedYet,
}

impl From<Rc<dyn InnerParseError>> for ParseError {
    fn from(value: Rc<dyn InnerParseError>) -> Self { Self::Inner(value) }
}

pub fn parse_all(level: Level) -> Result<Map<Alias, Rc<dyn Dependency>>, Vec<ParseError>> {
    let mut dependencies = IndexMap::new();
    let mut dependencies_errors = Vec::new();

    for (alias, dependency_lsd) in level.iter() {
        match parse_one(dependency_lsd.clone()) {
            Ok(dep) => drop(dependencies.insert(alias.clone(), dep)),
            Err(err) => dependencies_errors.push(err),
        }
    }

    match dependencies_errors.is_empty() {
        true => Ok(Map::new(dependencies)),
        false => Err(dependencies_errors),
    }
}

fn parse_one(value: LSD) -> Result<Rc<dyn Dependency>, ParseError> {
    use ParseError::*;
    match value {
        LSD::Level(level) => {
            let dependency_type = level
                .get_value(
                    key!(is),
                    DependencyTypeIsNotAValue,
                )?
                .ok_or(DependenciesWithoutTypeAreNotSupportedYet)?;

            let dependency_type = dependency_type.to_lowercase();
            match dependency_type.as_str() {
                "local" => return Ok(local_build::Dependency::try_parse(&level)?),
                _ => {},
            }

            match dependency_type
                .split_into_words()
                .ok_or(CouldNotFindMatchingDependencyType)?
            {
                // Add more implementations here...
                ["local", "build"] | ["local", "build++"] | ["local", "buildpp"] =>
                    return Ok(local_build::Dependency::try_parse(&level)?),

                ["local", "pair"] | ["local", "include"] | ["local", "library"] =>
                    return Ok(local_pair::Dependency::try_parse(&level)?),

                _ => return Err(CouldNotFindMatchingDependencyType)?,
            }
        },
        LSD::Value(_) => Err(DependenciesAsVersionsAreNotSupportedYet),
    }
}

//
// Dependency
//

pub trait InnerParseError: Debug {}

#[derive(Debug, Clone)]
pub enum CacheError {
    IOError(Rc<io::Error>),
    BuildError(Rc<BuildError>),
}

impl From<io::Error> for CacheError {
    fn from(value: io::Error) -> Self { Self::IOError(value.into()) }
}

impl From<BuildError> for CacheError {
    fn from(value: BuildError) -> Self { Self::BuildError(value.into()) }
}

pub trait Dependency {
    // parse

    fn try_parse(level: &Level) -> Result<Rc<dyn Dependency>, Rc<dyn InnerParseError>>
    where
        Self: Sized;

    // caching

    /// Selected version of the dependency.
    ///
    /// In some cases, this may represent latest version, or the only possible version.
    ///
    /// In some cases, this may be different from some kind of setting that dependency provides,
    /// because there may be multiple releases matching it.
    fn current_version(&self) -> Result<Version, io::Error>;

    fn current_profile(&self, selected_profile: &str) -> Result<profile::Name, io::Error>;

    /// Whether should this dependency recache or not.
    ///
    /// Default implementation is `false`,
    /// because per-version caches are supposed to be immutable.
    /// However, some dependencies may recache in some cases (ex. snapshots support).
    fn needs_recaching(&self, _cache_dep_dir: Dir) -> Result<bool, io::Error> { Ok(false) }

    /// Download/Copy/Link version and pre-build it.
    ///
    /// If `output_dir` already exists, version is considered already cached.
    fn cache(
        &self,
        current_profile: &str,
        include_dir: Dir,
        lib_dir: Dir,
    ) -> Result<(), CacheError>;

    // TODO GitBuild
    // TODO PackageManagerOrSomething
}
