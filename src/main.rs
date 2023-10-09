pub mod configuration;
pub mod dependency;
pub mod lsd;
pub mod profile;
mod subcommand;
pub mod util;

use std::env::args;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

use dependency::CacheError;
use lsd::Value;
use profile::Profile;

pub type Dir = Rc<Path>;
pub type Version = Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildType {
    Binary,
    Library,
}

impl BuildType {
    fn src_filename(&self) -> &'static str {
        use BuildType::*;
        match self {
            Binary => "main",
            Library => "lib",
        }
    }
}

impl FromStr for BuildType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use BuildType::*;
        let s = s.to_lowercase();
        if "binary".starts_with(&s) {
            return Ok(Binary);
        }
        if "library".starts_with(&s) {
            return Ok(Library);
        }
        Err(())
    }
}

//
// Main
//

#[derive(Debug, Clone)]
pub enum BuildError {
    CouldNotDetectSourceFile,
    RequiredBuildTypeDoesNotHaveMatchingSourceFile(BuildType),
    BuildTypeNeedsToBeSpecified,

    InvalidProfile(profile::Name),

    CacheCouldNotGetCurrentVersion(Rc<io::Error>),
    CacheCouldNotGetCurrentProfile(Rc<io::Error>),
    CacheCouldNotCheckIfNeedsRecaching(Rc<io::Error>),
    CacheCouldNotMakeCacheDirs(Rc<io::Error>),
    CacheError(CacheError),

    TargetCouldNotReadChanges(Rc<io::Error>),
    TargetCouldNotPrepareDirs(Rc<io::Error>),

    CompilerCouldNotCollectArguments(Rc<io::Error>),
    CompilerFailedSpawn(Rc<io::Error>),
    CompilerFailedWait(Rc<io::Error>),
    CompilerFailedExitCode(i32),
    CompilerKilled,

    PostBuildCouldNotCopyIncludes(Rc<io::Error>),
    PostBuildCouldNotDeleteObjectFiles(Rc<io::Error>),
    PostBuildCouldNotCopyDependencies(Rc<io::Error>),
}

impl From<CacheError> for BuildError {
    fn from(value: CacheError) -> Self { Self::CacheError(value) }
}

#[derive(Debug, Clone)]
pub enum RunError {
    BuildError(BuildError),
    FailedSpawn(Rc<io::Error>),
    FailedWait(Rc<io::Error>),
    Killed,
}

impl From<BuildError> for RunError {
    fn from(value: BuildError) -> Self { Self::BuildError(value) }
}

fn main_res() -> Result<(), subcommand::Error> {
    // process argv (split off after `-`, `/`, `--` for subcommands that may need it)
    let mut pre_dash_dash = Vec::new();
    let mut post_dash_dash = Vec::new();
    let mut hit_dash_dash = false;
    args()
        .skip(1)
        .for_each(|arg| match hit_dash_dash {
            true => post_dash_dash.push(arg),
            false => match arg.as_str() {
                "--" | "-" | "/" => hit_dash_dash = true,
                a if !a.is_empty() => pre_dash_dash.push(arg),
                _ => {},
            },
        });

    subcommand::parse_and_execute(
        pre_dash_dash.into_iter(),
        post_dash_dash.into_iter(),
    )?;

    Ok(())
}

fn main() {
    match main_res() {
        Ok(_) => {},
        Err(err) => Err(err).unwrap(),
    }
}
