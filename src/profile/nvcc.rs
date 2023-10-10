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
    CPP03,
    CPP11,
    CPP14,
    CPP17,
    CPP20,
}

impl Display for Standard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Standard::*;
        write!(
            f,
            "{}",
            match self {
                CPP03 => "c++03",
                CPP11 => "c++11",
                CPP14 => "c++14",
                CPP17 => "c++17",
                CPP20 => "c++20",
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
            "c++03" | "cpp03" => return Ok(CPP03),
            "c++11" | "cpp11" => return Ok(CPP11),
            "c++14" | "cpp14" => return Ok(CPP14),
            "c++17" | "cpp17" => return Ok(CPP17),
            "c++20" | "cpp20" => return Ok(CPP20),
            _ => {},
        }

        match s
            .split_into_words()
            .ok_or(())?
        {
            ["c++", "03"] | ["cpp", "03"] => return Ok(CPP03),
            ["c++", "11"] | ["cpp", "11"] => return Ok(CPP11),
            ["c++", "14"] | ["cpp", "14"] => return Ok(CPP14),
            ["c++", "17"] | ["cpp", "17"] => return Ok(CPP17),
            ["c++", "20"] | ["cpp", "20"] => return Ok(CPP20),
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
    No,
    Yes,
    EvenMore,
    YetMore,
    Size,
    UncompliantFast,
    Debug,
    SizeAggressive,
}

impl Display for Optimize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Optimize::*;
        write!(
            f,
            "{}",
            match self {
                No => "0",
                Yes => "1",
                EvenMore => "2",
                YetMore => "3",
                Size => "s",
                UncompliantFast => "fast",
                Debug => "g",
                SizeAggressive => "z",
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
            "0" | "o0" | "no" | "n" | "off" | "false" | "none" => return Ok(No),
            "1" | "o1" | "yes" | "y" | "on" | "true" | "o" | "optimize" => return Ok(Yes),
            "2" | "o2" => return Ok(EvenMore),
            "3" | "o3" => return Ok(YetMore),
            "s" | "os" | "size" => return Ok(Size),
            "fast" | "ofast" => return Ok(UncompliantFast),
            "g" | "og" | "debug" | "odebug" => return Ok(Debug),
            "z" | "oz" => return Ok(SizeAggressive),
            _ => {},
        }

        match s
            .split_into_words()
            .ok_or(())?
        {
            ["o", "0"] => return Ok(No),
            ["o", "1"] => return Ok(Yes),
            ["o", "2"] => return Ok(EvenMore),
            ["o", "3"] => return Ok(YetMore),
            ["o", "fast"] => return Ok(UncompliantFast),
            ["o", "g"] | ["o", "debug"] => return Ok(Debug),
            ["o", "size"] => return Ok(Size),
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
    optimize: Option<Optimize>,
    optimize_device: bool,
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

        self.optimize_device
            .try_replace(level.get_parse(
                key!(dopt),
                InvalidValueForKey("dopt"),
            )?);

        self.library_type
            .try_replace(level.get_parse(
                key!(library),
                InvalidValueForKey("library"),
            )?);

        Ok(())
    }

    fn src_file_suffix(&self) -> &'static str { ".cu" }

    #[cfg(target_os = "windows")]
    fn artifact_prefix(&self, _build_type: BuildType) -> &'static str { "" }

    #[cfg(target_os = "windows")]
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

    #[cfg(target_os = "linux")]
    fn artifact_prefix(&self, build_type: BuildType) -> &'static str {
        use BuildType::*;
        match build_type {
            Binary => "",
            Library => "lib",
        }
    }

    #[cfg(target_os = "linux")]
    fn artifact_suffix(&self, build_type: BuildType) -> &'static str {
        use BuildType::*;
        use LibraryType::*;
        match build_type {
            Binary => "",
            Library => match self.library_type {
                Shared => ".so",
                Static => ".a",
            },
        }
    }

    fn compiler_command(&self) -> &str {
        self.compiler_path
            .as_ref()
            .map(Rc::as_ref)
            .unwrap_or("nvcc")
    }

    fn compiler_arguments(
        &self,
        config: &Configuration,
        build_type: BuildType,
        selected_profile: &str,
    ) -> Result<Vec<Value>, io::Error> {
        let mut args = Vec::new();

        if let Some(opt_level) = &self.optimize {
            args.push_from("--optimize");
            args.push_from(format!("{}", opt_level));
        }

        if self.optimize_device {
            args.push_from("--dopt");
        }

        if let Some(std) = &self.standard {
            args.push_from("--std");
            args.push_from(format!("{}", std));
        }

        if build_type == BuildType::Library {
            use LibraryType::*;
            args.push_from(match self.library_type {
                Shared => "--shared",
                Static => todo!("static nvcc libs"),
            });
        }

        for (alias, dep) in config
            .dependencies()
            .iter()
        {
            let version = dep.current_version()?;
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

            args.push_from(format!(
                "--include-path=\"{}\"",
                include_dir.display()
            ));
            args.push_from(format!(
                "--library-path=\"{}\"",
                lib_dir.display()
            ));

            for lib in fs::read_dir(lib_dir)? {
                let filename = lib?.file_name();
                let (filename, ext) = split_file_name(
                    filename
                        .to_str()
                        .unwrap(),
                );
                if ext == "lib" || ext == "a" || ext == "exp" {
                    args.push_from(format!(
                        "--library=\"{}\"",
                        filename
                    ));
                }
            }
        }

        args.push_from("--output-file");
        args.push_from(
            config
                .target_artifact_file(
                    build_type,
                    selected_profile,
                    self,
                )
                .to_string_lossy(),
        );

        args.push_from(
            config
                .src_file(build_type, self)
                .display()
                .to_string(),
        );

        Ok(args)
    }
}
