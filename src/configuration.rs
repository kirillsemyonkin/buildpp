use std::fs;
use std::fs::File;
use std::io;
use std::process::Command;
use std::process::Stdio;
use std::rc::Rc;

use crate::dependency;
use crate::dependency::Dependency;
use crate::key;
use crate::lsd::LSDGetExt;
use crate::lsd::LSDParseError;
use crate::lsd::Map;
use crate::lsd::Value;
use crate::lsd::LSD;
use crate::profile;
use crate::util;
use crate::util::last_modified_recursive;
use crate::util::BoolGuardExt;
use crate::BuildError;
use crate::BuildType;
use crate::Dir;
use crate::Profile;
use crate::RunError;
use crate::Version;

//
// Run
//

struct Run {
    command: Value,
    arguments: Vec<Value>,
}

impl Run {
    fn parse(lsd: LSD) -> Result<Run, LoadError> {
        use LoadError::*;
        Ok(match lsd {
            // Parse `run "full command with spaces and with {} substitution"`
            LSD::Value(value) => {
                let mut value = value.split_whitespace();
                Run {
                    command: value
                        .next()
                        .unwrap_or("{}")
                        .into(),
                    arguments: value
                        .map(Value::from)
                        .collect(),
                }
            },

            LSD::Level(level) => match level.is_list() {
                // Parse `run [ full command with each list item being a command or arg with {} substitution ]`
                true => {
                    let mut list = level
                        .values()
                        .map(|arg| {
                            if arg
                                .to_level()
                                .is_some_and(|l| l.is_empty())
                            {
                                return Ok("{}".into());
                            }

                            arg.to_value()
                                .ok_or(RunPieceIsNotAValue)
                        })
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter();
                    Run {
                        command: list
                            .next()
                            .as_ref()
                            .map(Rc::as_ref)
                            .unwrap_or("{}")
                            .into(),
                        arguments: list.collect(),
                    }
                },

                // Parse `run { command command_name_or_{}   arguments ... }`
                false => Run {
                    command: level
                        .get_inner(key!(command))
                        .map(|command| {
                            if command
                                .to_level()
                                .is_some_and(|l| l.is_empty())
                            {
                                return Ok("{}".into());
                            }

                            command
                                .to_value()
                                .ok_or(RunCommandIsNotAValue)
                        })
                        .transpose()?
                        .ok_or(MissingCommandInRun)?,

                    arguments: {
                        level
                            .get_inner(key!(arguments))
                            .map(|inner| match inner {
                                // Parse `arguments "arguments with spaces and with {} substitution"`
                                LSD::Value(value) => Ok(value
                                    .split_whitespace()
                                    .map(Rc::from)
                                    .collect::<Vec<_>>()),

                                // Parse `arguments [ each list item being an arg with {} substitution ]`
                                LSD::Level(list) => list
                                    .values()
                                    .map(|arg| {
                                        if arg
                                            .to_level()
                                            .is_some_and(|l| l.is_empty())
                                        {
                                            return Ok("{}".into());
                                        }

                                        arg.to_value()
                                            .ok_or(RunPieceIsNotAValue)
                                    })
                                    .collect::<Result<Vec<_>, _>>(),
                            })
                            .transpose()?
                            .unwrap_or_default()
                    },
                },
            },
        })
    }
}

//
// Configuration
//

#[derive(Debug, Clone)]
pub enum LoadError {
    CouldNotOpenConfiguration(Rc<io::Error>),
    CouldNotParseLSD(LSDParseError),

    MissingProjectName,
    ProjectNameIsNotAValue,

    MissingVersion,
    VersionIsNotAValue,

    DependenciesIsNotALevel,
    DependenciesErrors(Vec<dependency::ParseError>),

    ProfilesIsNotALevel,
    ProfilesErrors(Vec<profile::ParseError>),

    MissingCommandInRun,
    RunCommandIsNotAValue,
    RunPieceIsNotAValue,
}

impl From<LSDParseError> for LoadError {
    fn from(value: LSDParseError) -> Self { Self::CouldNotParseLSD(value) }
}

pub struct Configuration {
    config_file: Dir,
    project_dir: Dir,

    name: Value,
    version: Version,

    dependencies: Map<dependency::Alias, Rc<dyn Dependency>>,
    profiles: Map<profile::Name, Rc<dyn Profile>>,

    run: Option<Run>,
}

impl Configuration {
    // Basic info

    pub fn load(project_dir: Dir) -> Result<Self, LoadError> {
        use LoadError::*;

        const CONFIG_FILENAME: &str = "build++.lsd";
        let config_file = project_dir
            .join(CONFIG_FILENAME)
            .into();

        let file = File::open(&config_file)
            .map_err(Rc::new)
            .map_err(CouldNotOpenConfiguration)?;
        let lsd = LSD::parse(file)?;

        Ok(Configuration {
            config_file,
            project_dir,

            name: lsd
                .get_value(
                    key!(name),
                    ProjectNameIsNotAValue,
                )?
                .ok_or(MissingProjectName)?,

            version: lsd
                .get_value(
                    key!(version),
                    VersionIsNotAValue,
                )?
                .ok_or(MissingVersion)?,

            dependencies: match lsd.get_level(
                key!(dependency),
                DependenciesIsNotALevel,
            )? {
                Some(dependency) =>
                    dependency::parse_all(dependency).map_err(DependenciesErrors)?,
                None => Map::default(),
            },

            profiles: match lsd.get_level(
                key!(profile),
                ProfilesIsNotALevel,
            )? {
                Some(profile) => profile::parse_all(profile).map_err(ProfilesErrors)?,
                None => Map::default(),
            },

            run: lsd
                .get_inner(key!(run))
                .map(Run::parse)
                .transpose()?,
        })
    }

    pub fn project_name(&self) -> Version {
        self.name
            .clone()
    }

    pub fn version(&self) -> Version {
        self.version
            .clone()
    }

    pub fn dependencies(&self) -> Map<Value, Rc<dyn Dependency>> {
        self.dependencies
            .clone()
    }

    pub fn profiles(&self) -> Map<Value, Rc<dyn Profile>> {
        self.profiles
            .clone()
    }

    pub fn profile(&self, value: &str) -> Option<&dyn Profile> {
        self.profiles
            .get(value)
            .map(Rc::as_ref)
    }

    pub fn run_command(&self, profile_name: &str, profile: &dyn Profile) -> String {
        self.run
            .as_ref()
            .map(|run| &run.command)
            .cloned()
            .unwrap_or_else(|| "{}".into())
            .replace(
                "{}", // TODO do not touch {{}}
                &self
                    .target_artifact_file(BuildType::Binary, profile_name, profile)
                    .display()
                    .to_string(),
            )
    }

    pub fn run_arguments(&self, profile_name: &str, profile: &dyn Profile) -> Vec<String> {
        self.run
            .as_ref()
            .map(|run| {
                run.arguments
                    .iter()
                    .map(|arg| {
                        arg.replace(
                            "{}", // TODO do not touch {{}}
                            &self
                                .target_artifact_file(BuildType::Binary, profile_name, profile)
                                .display()
                                .to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    // Dirs

    pub fn config_file(&self) -> Dir {
        self.config_file
            .clone()
    }

    pub fn project_dir(&self) -> Dir {
        self.project_dir
            .clone()
    }

    pub fn src_dir(&self) -> Dir {
        self.project_dir
            .join("src")
            .into()
    }

    pub fn src_file(&self, build_type: BuildType, profile: &dyn Profile) -> Dir {
        self.src_dir()
            .join(format!(
                "{}{}",
                build_type.src_filename(),
                profile.src_file_suffix()
            ))
            .into()
    }

    pub fn target_dir(&self, profile: &str) -> Dir {
        self.project_dir
            .join("target")
            .join(&*self.version)
            .join(profile)
            .into()
    }

    pub fn target_include_dir(&self, profile: &str) -> Dir {
        self.target_dir(profile)
            .join("include")
            .into()
    }

    pub fn target_artifact_dir(&self, profile: &str) -> Dir {
        self.target_dir(profile)
            .join("artifact")
            .into()
    }

    pub fn target_artifact_file(
        &self,
        build_type: BuildType,
        profile_name: &str,
        profile: &dyn Profile,
    ) -> Dir {
        self.target_artifact_dir(profile_name)
            .join(format!(
                "{}{}{}",
                profile.artifact_prefix(build_type),
                self.name,
                profile.artifact_suffix(build_type),
            ))
            .into()
    }

    pub fn cache_dir(&self) -> Dir {
        self.project_dir
            .join("cache")
            .into()
    }

    pub fn cache_dep_dir(
        &self,
        dependency: dependency::Alias,
        version: Version,
        profile: &str,
    ) -> Dir {
        let mut res = self
            .cache_dir()
            .join(&*dependency);
        if !version.is_empty() {
            res = res.join(&*version);
        }
        if !profile.is_empty() {
            res = res.join(profile);
        }
        res.into()
    }

    pub fn cache_dep_include_dir(
        &self,
        dependency: dependency::Alias,
        version: Version,
        profile: &str,
    ) -> Dir {
        self.cache_dep_dir(dependency, version, profile)
            .join("include")
            .into()
    }

    pub fn cache_dep_lib_dir(
        &self,
        dependency: dependency::Alias,
        version: Version,
        profile: &str,
    ) -> Dir {
        self.cache_dep_dir(dependency, version, profile)
            .join("lib")
            .into()
    }

    // Actions

    pub fn build(
        &self,
        build_type: Option<BuildType>,
        profile_name: &str,
    ) -> Result<&dyn Profile, BuildError> {
        use BuildError::*;
        use BuildType::*;

        // detect profile
        let profile = self
            .profile(&profile_name)
            .ok_or_else(|| InvalidProfile(profile_name.into()))?;

        // detect build_type
        let build_type = match (
            build_type,
            self.src_file(Binary, &*profile)
                .is_file(),
            self.src_file(Library, &*profile)
                .is_file(),
        ) {
            (Some(build_type), true, true) => build_type,
            (Some(Binary), true, _) => Binary,
            (Some(Library), _, true) => Library,
            (None, true, true) => return Err(BuildTypeNeedsToBeSpecified)?,
            (None, true, _) => Binary,
            (None, _, true) => Library,
            // also ensures that /src/ exists
            _ => return Err(CouldNotDetectSourceFile)?,
        };

        // cache dependencies
        // NOTE: do not make cache folder for no reason: every dep will do it themselves
        let mut any_recached = false;
        for (alias, dep) in self
            .dependencies
            .iter()
        {
            let version = dep
                .current_version()
                .map_err(Rc::new)
                .map_err(CacheCouldNotGetCurrentVersion)?;
            let current_profile = dep
                .current_profile(profile_name)
                .map_err(Rc::new)
                .map_err(CacheCouldNotGetCurrentProfile)?;

            let cache_dep_dir = self.cache_dep_dir(
                alias.clone(),
                version.clone(),
                &current_profile,
            );

            if cache_dep_dir.is_dir()
                && !dep
                    .needs_recaching(cache_dep_dir.clone())
                    .map_err(Rc::new)
                    .map_err(CacheCouldNotCheckIfNeedsRecaching)?
            {
                continue;
            }

            let include_dir = self.cache_dep_include_dir(
                alias.clone(),
                version.clone(),
                &current_profile,
            );
            let lib_dir = self.cache_dep_lib_dir(
                alias.clone(),
                version.clone(),
                &current_profile,
            );

            fs::create_dir_all(&cache_dep_dir)
                .map_err(Rc::new)
                .map_err(CacheCouldNotMakeCacheDirs)?;
            fs::create_dir_all(&include_dir)
                .map_err(Rc::new)
                .map_err(CacheCouldNotMakeCacheDirs)?;
            fs::create_dir_all(&lib_dir)
                .map_err(Rc::new)
                .map_err(CacheCouldNotMakeCacheDirs)?;

            dep.cache(
                &current_profile,
                include_dir,
                lib_dir,
            )?;
            any_recached = true;
        }

        // ensure needs a rebuild
        let target_dir = self.target_dir(&profile_name);
        if !any_recached
            && target_dir.is_dir()
            && last_modified_recursive(target_dir)
                .map_err(Rc::new)
                .map_err(TargetCouldNotReadChanges)?
                >= Ord::max(
                    last_modified_recursive(self.config_file())
                        .map_err(Rc::new)
                        .map_err(TargetCouldNotReadChanges)?,
                    last_modified_recursive(self.src_dir())
                        .map_err(Rc::new)
                        .map_err(TargetCouldNotReadChanges)?,
                )
        {
            return Ok(&*profile);
        }

        // prepare target dirs
        util::remove_dir_all(self.target_dir(&profile_name))
            .map_err(Rc::new)
            .map_err(TargetCouldNotPrepareDirs)?;
        fs::create_dir_all(self.target_dir(&profile_name))
            .map_err(Rc::new)
            .map_err(TargetCouldNotPrepareDirs)?;
        fs::create_dir_all(self.target_artifact_dir(&profile_name))
            .map_err(Rc::new)
            .map_err(TargetCouldNotPrepareDirs)?;
        fs::create_dir_all(self.target_include_dir(&profile_name))
            .map_err(Rc::new)
            .map_err(TargetCouldNotPrepareDirs)?;

        // run compiler
        let code = Command::new(profile.compiler_command())
            .args(
                profile
                    .compiler_arguments(
                        self,
                        build_type,
                        &profile_name,
                    )
                    .map_err(Rc::new)
                    .map_err(CompilerCouldNotCollectArguments)?
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
            )
            .current_dir(&self.target_artifact_dir(&profile_name))
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(Rc::new)
            .map_err(CompilerFailedSpawn)?
            .wait()
            .map_err(Rc::new)
            .map_err(CompilerFailedWait)?
            .code()
            .ok_or(CompilerKilled)?;

        (code == 0).ok_or(CompilerFailedExitCode(code))?;

        // copy over includes to resulting dir
        util::copy_dir_all_filter_extension(
            self.src_dir(),
            self.target_include_dir(&profile_name),
            &|extension| {
                // https://gcc.gnu.org/onlinedocs/gcc/Overall-Options.html
                extension == "h" // c
                    || extension == "cuh" // cuda
                    || extension == "hh"
                    || extension == "H"
                    || extension == "hp"
                    || extension == "hxx"
                    || extension == "hpp"
                    || extension == "HPP"
                    || extension == "h++"
                    || extension == "tcc"
            },
        )
        .map_err(Rc::new)
        .map_err(PostBuildCouldNotCopyIncludes)?;

        // remove .objs
        util::remove_dir_all_filter_extension(
            self.target_artifact_dir(&profile_name),
            &|extension| extension == "obj",
        )
        .map_err(Rc::new)
        .map_err(PostBuildCouldNotDeleteObjectFiles)?;

        // copy over cached libs to target
        for (alias, dep) in self
            .dependencies
            .iter()
        {
            let version = dep
                .current_version() // FIXME do not repeat this
                .map_err(Rc::new)
                .map_err(CacheCouldNotGetCurrentVersion)?;
            let profile = dep
                .current_profile(profile_name)
                .map_err(Rc::new)
                .map_err(CacheCouldNotGetCurrentProfile)?;

            let include_dir = self.cache_dep_include_dir(
                alias.clone(),
                version.clone(),
                &profile,
            );
            let lib_dir = self.cache_dep_lib_dir(
                alias.clone(),
                version.clone(),
                &profile,
            );

            util::copy_dir_all(
                include_dir,
                self.target_include_dir(&profile_name),
            )
            .map_err(Rc::new)
            .map_err(PostBuildCouldNotCopyDependencies)?;

            util::copy_dir_all(
                lib_dir,
                self.target_artifact_dir(&profile_name),
            )
            .map_err(Rc::new)
            .map_err(PostBuildCouldNotCopyDependencies)?;
        }

        Ok(&*profile)
    }

    pub fn run(
        &self,
        profile_name: profile::Name,
        additional_args: Rc<[Value]>,
    ) -> Result<i32, RunError> {
        use RunError::*;

        // build binary first (will error if not binary / not runnable)
        let profile = self.build(
            Some(BuildType::Binary),
            &profile_name,
        )?;

        // then run
        let command = self.run_command(&profile_name, profile);
        let mut args = self.run_arguments(&profile_name, profile);
        for add_arg in additional_args.iter() {
            args.push(add_arg.to_string());
        }
        println!(
            "running {} {}",
            command,
            args.join(" ")
        );
        let code = Command::new(command)
            .args(args)
            .current_dir(&self.project_dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(Rc::new)
            .map_err(FailedSpawn)?
            .wait()
            .map_err(Rc::new)
            .map_err(FailedWait)?
            .code()
            .ok_or(Killed)?;

        Ok(code)
    }
}
