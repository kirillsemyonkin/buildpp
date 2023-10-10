use std::fmt::Display;
use std::fs;
use std::io;
use std::rc::Rc;
use std::str::FromStr;

use super::ParseError;
use crate::configuration::Configuration;
use crate::key;
use crate::lsd::LSDGetExt;
use crate::lsd::Level;
use crate::lsd::Value;
use crate::util::split_file_name;
use crate::util::PushFrom;
use crate::util::SplitIntoTwoWordsExt;
use crate::util::TryReplace;
use crate::BuildType;

//
// Standard
//

#[derive(Clone, Copy, PartialEq, Eq)]
enum Standard {
    CPP14,
    CPP17,
    CPP20,
    CPPLatest,
    C11,
    C17,
}

impl Display for Standard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Standard::*;
        write!(
            f,
            "{}",
            match self {
                CPP14 => "c++14",
                CPP17 => "c++17",
                CPP20 => "c++20",
                CPPLatest => "c++latest",
                C11 => "c11",
                C17 => "c17",
            }
        )
    }
}

impl FromStr for Standard {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Standard::*;

        let s = s.to_lowercase();
        match s.as_str() {
            "c++14" | "cpp14" => return Ok(CPP14),
            "c++17" | "cpp17" => return Ok(CPP17),
            "c++20" | "cpp20" => return Ok(CPP20),
            "c++latest" | "cpplatest" => return Ok(CPPLatest),
            "c11" => return Ok(C11),
            "c17" => return Ok(C17),
            _ => {},
        }

        match s
            .split_into_words()
            .ok_or(())?
        {
            ["c++", "14"] | ["cpp", "14"] => return Ok(CPP14),
            ["c++", "17"] | ["cpp", "17"] => return Ok(CPP17),
            ["c++", "20"] | ["cpp", "20"] => return Ok(CPP20),
            ["c++", "latest"] | ["cpp", "latest"] => return Ok(CPPLatest),
            ["c", "11"] => return Ok(C11),
            ["c", "17"] => return Ok(C17),
            _ => {},
        }

        Err(())
    }
}

//
// Optimization
//

#[derive(Clone, Copy)]
enum Optimize {
    MinimizeSize,
    MaximizeSpeed,
}

impl Display for Optimize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Optimize::*;
        write!(
            f,
            "{}",
            match self {
                MinimizeSize => "1",
                MaximizeSpeed => "2",
            }
        )
    }
}

impl FromStr for Optimize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Optimize::*;

        let s = s.to_lowercase();
        match s.as_str() {
            "1" | "o1" | "minsize" | "minimumsize" | "minimizesize" | "size" =>
                return Ok(MinimizeSize),
            "2" | "o2" | "maxspeed" | "maximumspeed" | "maximizespeed" | "speed" =>
                return Ok(MaximizeSpeed),
            _ => {},
        }

        match s
            .split_into_words()
            .ok_or(())?
        {
            ["o", "1"] | ["min", "size"] | ["minimum", "size"] | ["minimize", "size"] =>
                return Ok(MinimizeSize),
            ["o", "2"] | ["max", "speed"] | ["maximum", "speed"] | ["maximize", "speed"] =>
                return Ok(MaximizeSpeed),
            _ => {},
        }

        Err(())
    }
}

//
// LibraryType
//

#[derive(Default, Clone, Copy)]
enum LibraryType {
    #[default]
    Shared,
    Static,
}

impl TryReplace for LibraryType {
    type With = LibraryType;
}

impl FromStr for LibraryType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use LibraryType::*;
        let s = s.to_lowercase();
        match s.as_str() {
            "static" | "lib" | "a" => Ok(Static),
            "shared" | "dll" | "so" => Ok(Shared),
            _ => Err(()),
        }
    }
}

//
// Profile
//

#[derive(Default, Clone)]
pub(crate) struct Profile {
    compiler_path: Option<Value>,
    standard: Option<Standard>,
    optimize: Option<Optimize>, // optional because we can omit flag
    openmp: bool,
    library_type: LibraryType,
}

impl super::Profile for Profile {
    fn create_default() -> Rc<dyn super::Profile>
    where
        Self: Sized, {
        Rc::new(Self::default())
    }

    fn inherit_with(&self, level: Level) -> Result<Rc<dyn super::Profile>, ParseError> {
        let mut res = self.clone();
        res.apply(level)?;
        Ok(Rc::new(res))
    }

    fn apply(&mut self, level: Level) -> Result<(), ParseError> {
        use ParseError::*;

        self.compiler_path
            .try_replace(level.get_value(
                key!(compiler_path),
                InvalidValueForKey("compiler_path"),
            )?);

        self.standard
            .try_replace(level.get_parse(
                key!(standard),
                InvalidValueForKey("standard"),
            )?);

        self.optimize
            .try_replace(level.get_parse(
                key!(optimize),
                InvalidValueForKey("optimize"),
            )?);

        self.openmp
            .try_replace(level.get_parse(
                key!(openmp),
                InvalidValueForKey("openmp"),
            )?);

        self.library_type
            .try_replace(level.get_parse(
                key!(library),
                InvalidValueForKey("library"),
            )?);

        Ok(())
    }

    fn src_file_suffix(&self) -> &'static str { ".cpp" }

    fn artifact_prefix(&self, _build_type: BuildType) -> &'static str { "" }

    fn artifact_suffix(&self, build_type: BuildType) -> &'static str {
        use BuildType::*;
        use LibraryType::*;
        match build_type {
            Binary => ".exe",
            Library => match self.library_type {
                Shared => ".dll",
                Static => ".lib",
            },
        }
    }

    fn compiler_command(&self) -> &str {
        self.compiler_path
            .as_ref()
            .map(Rc::as_ref)
            .unwrap_or("cl")
    }

    fn compiler_arguments(
        &self,
        config: &Configuration,
        build_type: BuildType,
        selected_profile: &str,
    ) -> Result<Vec<Value>, io::Error> {
        let mut args = Vec::new();

        // Compiler

        if self.openmp {
            args.push_from("/openmp");
        }

        if let Some(opt_level) = &self.optimize {
            args.push_from(format!("/O{}", opt_level));
        }

        if let Some(std) = &self.standard {
            args.push_from(format!("/std:{}", std));
        }

        let mut include_dirs = Vec::new();
        let mut lib_dirs = Vec::new();
        let mut libs = Vec::new();

        for (alias, dep) in config
            .dependencies()
            .iter()
        {
            let version = dep.current_version()?; // TODO move this to dep's parse
            let profile = dep.current_profile(selected_profile)?;

            let include_dir = config.cache_dep_include_dir(
                alias.clone(),
                version.clone(),
                &profile,
            );
            let lib_dir = config.cache_dep_lib_dir(
                alias.clone(),
                version.clone(),
                &profile,
            );

            include_dirs.push(format!(
                "{}",
                include_dir.display(),
            ));
            lib_dirs.push(format!(
                "{}",
                lib_dir.display(),
            ));

            for lib in fs::read_dir(lib_dir)? {
                let filename = lib?
                    .file_name()
                    .to_str()
                    .unwrap()
                    .to_string();
                let (_, ext) = split_file_name(&filename);
                if ext == "lib" || ext == "a" || ext == "exp" {
                    libs.push(filename.to_string());
                }
            }
        }

        for include in include_dirs {
            args.push_from("/I");
            args.push_from(include);
        }

        args.push_from(
            config
                .src_file(build_type, self)
                .display()
                .to_string(),
        );

        // Linker

        for lib in libs {
            args.push_from(lib);
        }

        args.push_from("/link");

        args.push_from(format!(
            "/OUT:{}",
            config
                .target_artifact_file(build_type, selected_profile, self)
                .display(),
        ));

        if build_type == BuildType::Library {
            use LibraryType::*;
            args.push_from(match self.library_type {
                Shared => "/DLL",
                Static => todo!("static msvc libs"),
            });
        }

        for lib_dir in lib_dirs {
            args.push_from(format!(
                "/LIBPATH:{}",
                lib_dir
            ));
        }

        Ok(args)
    }
}
