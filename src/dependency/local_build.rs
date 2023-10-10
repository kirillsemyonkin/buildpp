use std::convert::Infallible;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

use super::CacheError;
use crate::configuration::Configuration;
use crate::configuration::LoadError;
use crate::key;
use crate::lsd::LSDGetExt;
use crate::lsd::Level;
use crate::profile;
use crate::profile::DEFAULT_PROFILE;
use crate::util;
use crate::util::last_modified_recursive;
use crate::BuildType;
use crate::Dir;
use crate::Version;

pub(crate) struct Dependency {
    config: Configuration,
    profile: Profile,
}

#[derive(Debug, Clone)]
enum Profile {
    Inherit,
    OfName(profile::Name),
}

impl FromStr for Profile {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Profile::*;
        Ok(
            match s
                .to_lowercase()
                .as_str()
            {
                "inherit" => Inherit,
                name => OfName(name.into()),
            },
        )
    }
}

#[derive(Debug, Clone)]
enum InnerParseError {
    MissingProjectPath,
    ProjectPathIsNotAValue,

    ConfigurationLoadError(LoadError),

    ProfileIsNotAValue,
}

impl super::InnerParseError for InnerParseError {
}

impl From<InnerParseError> for Rc<dyn super::InnerParseError> {
    fn from(value: InnerParseError) -> Self { Rc::new(value) }
}

impl super::Dependency for Dependency {
    fn try_parse(
        level: &Level,
    ) -> Result<Rc<dyn super::Dependency>, Rc<dyn super::InnerParseError>>
    where
        Self: Sized, {
        use InnerParseError::*;

        // 1. try reading directory
        let project_dir = level
            .get_value(
                key!(path),
                ProjectPathIsNotAValue,
            )?
            .ok_or(MissingProjectPath)?;
        let project_dir = Dir::from(Path::new(&*project_dir));

        // 2. try loading configuration file at location
        let config = Configuration::load(project_dir).map_err(ConfigurationLoadError)?;

        // 3. try grabbing profile
        let profile = level
            .get_value(
                key!(profile),
                ProfileIsNotAValue,
            )?
            .map(|s| {
                s.parse()
                    .unwrap()
            })
            .unwrap_or(Profile::OfName(
                DEFAULT_PROFILE.into(),
            ));

        Ok(Rc::new(Dependency {
            config,
            profile,
        }))
    }

    fn current_version(&self) -> Result<Version, io::Error> {
        Ok(self
            .config
            .version())
    }

    fn current_profile(&self, selected_profile: &str) -> Result<profile::Name, io::Error> {
        Ok(match &self.profile {
            Profile::Inherit => selected_profile.into(),
            Profile::OfName(name) => name.clone(),
        })
    }

    fn needs_recaching(
        &self,
        selected_profile: &str,
        cache_dep_dir: Dir,
    ) -> Result<bool, io::Error> {
        let target_dir = self
            .config
            .target_dir(selected_profile);
        Ok(!target_dir.is_dir()
            || last_modified_recursive(cache_dep_dir)?
                < [
                    last_modified_recursive(
                        &self
                            .config
                            .config_file(),
                    )?,
                    last_modified_recursive(
                        &self
                            .config
                            .src_dir(),
                    )?,
                    last_modified_recursive(target_dir)?,
                ]
                .into_iter()
                .max()
                .unwrap())
    }

    fn cache(
        &self,
        selected_profile: &str,
        include_dir: Dir,
        lib_dir: Dir,
    ) -> Result<(), CacheError> {
        // 1. ensure dependency is built
        self.config
            .build(
                Some(BuildType::Library),
                selected_profile,
                false,
            )?;

        // 2. copy over results (include -> include_dir, artifact -> lib_dir)
        util::copy_dir_all(
            self.config
                .target_include_dir(selected_profile),
            include_dir,
        )?;
        util::copy_dir_all(
            self.config
                .target_artifact_dir(selected_profile),
            lib_dir,
        )?;

        // now the version is considered cached, so:
        // - include_dir can be -I'd,
        // - lib_dir can be -L'd.

        Ok(())
    }
}
