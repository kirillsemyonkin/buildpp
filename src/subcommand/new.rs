use std::fs;
use std::fs::File;
use std::io;
use std::io::Write;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::lsd::Value;
use crate::util::format_multiline_code;
use crate::util::BoolGuardExt;
use crate::BuildType;
use crate::Dir;

pub struct Subcommand {
    build_type: BuildType,
    name: Value,
}

#[derive(Debug, Clone)]
enum InnerParseError {
    FoundExtraFlags(Rc<[Value]>),

    MissingBuildType,
    BuildTypeHasToHaveExactlyOneValue,
    UnknownBuildType,

    MissingProjectName,
    NameHasToHaveExactlyOneValue,
}

impl super::InnerParseError for InnerParseError {
}

impl From<InnerParseError> for Rc<dyn super::InnerParseError> {
    fn from(value: InnerParseError) -> Self { Rc::new(value) }
}

#[derive(Debug, Clone)]
enum InnerExecuteError {
    InvalidCurrentDir(Rc<io::Error>),

    CouldNotCheckProjectDir(Rc<io::Error>),
    ProjectDirAlreadyExistsAndHasFiles,
    CouldNotCreateProjectDir(Rc<io::Error>),
    CouldNotCreateConfigurationFile(Rc<io::Error>),
    CouldNotWriteConfigurationFile(Rc<io::Error>),

    CouldNotCreateSourceDir(Rc<io::Error>),
    CouldNotCreateSourceFile(Rc<io::Error>),
    CouldNotWriteSourceFile(Rc<io::Error>),
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

fn parse_name(name: Rc<[Value]>) -> Result<Value, InnerParseError> {
    use InnerParseError::*;

    let mut name_values = name.iter();
    let name = name_values
        .next()
        .ok_or(NameHasToHaveExactlyOneValue)?;
    name_values
        .next()
        .is_none()
        .ok_or(NameHasToHaveExactlyOneValue)?;

    Ok(name.clone())
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
            .transpose()?
            .ok_or(MissingBuildType)?;

        let name = flags
            .remove("name")
            .map(parse_name)
            .transpose()?
            .ok_or(MissingProjectName)?;

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
            name,
        }))
    }

    fn execute(&self) -> Result<(), Rc<dyn super::InnerExecuteError>> {
        use InnerExecuteError::*;

        // FIXME do not override anything

        // setup dir
        let parent_dir = Dir::from(
            std::env::current_dir()
                .map_err(Rc::new)
                .map_err(InvalidCurrentDir)?,
        );

        let project_dir = parent_dir.join(&*self.name);

        if project_dir.exists()
            && (project_dir.is_file()
                || fs::read_dir(&project_dir)
                    .map_err(Rc::new)
                    .map_err(CouldNotCheckProjectDir)?
                    .next()
                    .is_some())
        {
            return Err(ProjectDirAlreadyExistsAndHasFiles)?;
        }

        fs::create_dir_all(&project_dir)
            .map_err(Rc::new)
            .map_err(CouldNotCreateProjectDir)?;

        // create config
        let config_path = project_dir.join("build++.lsd");

        let mut config_file = File::create(config_path)
            .map_err(Rc::new)
            .map_err(CouldNotCreateConfigurationFile)?;

        // FIXME fill .lsd file properly
        writeln!(
            config_file,
            "name {}",
            self.name
        )
        .map_err(Rc::new)
        .map_err(CouldNotWriteConfigurationFile)?;
        writeln!(config_file, "version 0.1.0")
            .map_err(Rc::new)
            .map_err(CouldNotWriteConfigurationFile)?;

        // create main
        let src_dir = project_dir.join("src");

        fs::create_dir_all(&src_dir)
            .map_err(Rc::new)
            .map_err(CouldNotCreateSourceDir)?;

        let src_path = src_dir.join(format!(
            "{}.cpp",
            self.build_type
                .src_filename()
        ));

        let mut src_file = File::create(src_path)
            .map_err(Rc::new)
            .map_err(CouldNotCreateSourceFile)?;

        writeln!(
            src_file,
            "{}",
            format_multiline_code(
                r#"
                    #include <iostream>

                    using std::cout;
                    using std::endl;

                    int main() {
                        cout << "Hello world!" << endl;
                        return 0;
                    }
                "#
            )
        )
        .map_err(Rc::new)
        .map_err(CouldNotWriteSourceFile)?;

        // TODO init git

        Ok(())
    }
}
