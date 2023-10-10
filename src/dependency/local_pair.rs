use std::io;
use std::path::Path;
use std::rc::Rc;

use super::CacheError;
use crate::key;
use crate::lsd::LSDGetExt;
use crate::lsd::Level;
use crate::util;
use crate::util::last_modified_recursive;
use crate::util::BoolGuardExt;
use crate::Dir;
use crate::Version;

pub(crate) struct Dependency {
    include_dir: Dir,
    lib_dir: Dir,
}

#[derive(Debug, Clone)]
enum InnerParseError {
    MissingIncludePath,
    IncludePathIsNotAValue,
    IncludeDirIsNotADir,

    MissingLibraryPath,
    LibraryPathIsNotAValue,
    LibDirIsNotADir,
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

        // Read paths from level
        let include_path = level
            .get_value(
                key!(include),
                IncludePathIsNotAValue,
            )?
            .ok_or(MissingIncludePath)?;
        let include_dir = Dir::from(Path::new(&*include_path));

        let library_path = level
            .get_value(
                key!(library),
                LibraryPathIsNotAValue,
            )?
            .ok_or(MissingLibraryPath)?;
        let lib_dir = Dir::from(Path::new(&*library_path));

        // Ensure dirs exist
        include_dir
            .is_dir()
            .ok_or(IncludeDirIsNotADir)?;
        lib_dir
            .is_dir()
            .ok_or(LibDirIsNotADir)?;

        Ok(Rc::new(Dependency {
            include_dir,
            lib_dir,
        }))
    }

    fn current_version(&self) -> Result<Version, io::Error> { Ok("".into()) }

    fn current_profile(&self, _selected_profile: &str) -> Result<crate::profile::Name, io::Error> {
        Ok("".into())
    }

    fn needs_recaching(
        &self,
        _selected_profile: &str,
        cache_dep_dir: Dir,
    ) -> Result<bool, io::Error> {
        Ok(
            last_modified_recursive(cache_dep_dir)?
                < Ord::max(
                    last_modified_recursive(&self.include_dir)?,
                    last_modified_recursive(&self.lib_dir)?,
                ),
        )
    }

    fn cache(
        &self,
        _current_profile: &str,
        include_dir: Dir,
        lib_dir: Dir,
    ) -> Result<(), CacheError> {
        // just copy over (include_dir -> include_dir, lib_dir -> lib_dir)
        util::copy_dir_all(&self.include_dir, include_dir)?;
        util::copy_dir_all(&self.lib_dir, lib_dir)?;
        Ok(())
    }
}
