use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

//
// split_file_name
//

pub fn split_file_name(filename: &str) -> (&str, &str) {
    let parts = filename
        .split('.')
        .collect::<Vec<_>>();
    match parts.len() {
        1 => ("", ""),
        _ => {
            let ext = parts
                .last()
                .unwrap();
            (
                filename
                    .split_at(filename.len() - (ext.len() + 1))
                    .0,
                ext,
            )
        },
    }
}

//
// copy_dir_all
//

pub fn copy_dir_all_filter_extension(
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    extension_filter: &impl Fn(&str) -> bool,
) -> Result<(), io::Error> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry
            .file_type()?
            .is_dir()
        {
            copy_dir_all_filter_extension(
                entry.path(),
                dst.as_ref()
                    .join(entry.file_name()),
                extension_filter,
            )?;
        } else if extension_filter(
            split_file_name(
                entry
                    .file_name()
                    .to_str()
                    .unwrap(),
            )
            .1,
        ) {
            fs::copy(
                entry.path(),
                dst.as_ref()
                    .join(entry.file_name()),
            )?;
        }
    }
    Ok(())
}

pub fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), io::Error> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if entry
            .file_type()?
            .is_dir()
        {
            copy_dir_all(
                entry.path(),
                dst.as_ref()
                    .join(entry.file_name()),
            )?;
        } else {
            fs::copy(
                entry.path(),
                dst.as_ref()
                    .join(entry.file_name()),
            )?;
        }
    }
    Ok(())
}

//
// remove_dir_all
//

pub fn remove_dir_all_filter_extension(
    dst: impl AsRef<Path>,
    extension_filter: &impl Fn(&str) -> bool,
) -> Result<(), io::Error> {
    if !dst
        .as_ref()
        .exists()
    {
        return Ok(());
    }
    for entry in fs::read_dir(dst)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            remove_dir_all(entry.path())?;
        } else if extension_filter(
            split_file_name(
                entry
                    .file_name()
                    .to_str()
                    .unwrap(),
            )
            .1,
        ) {
            fs::remove_file(entry.path())?;
        }
    }
    // fs::remove_dir(dst)?;
    Ok(())
}

pub fn remove_dir_all(dst: impl AsRef<Path>) -> Result<(), io::Error> {
    if !dst
        .as_ref()
        .exists()
    {
        return Ok(());
    }
    for entry in fs::read_dir(dst.as_ref())? {
        let entry = entry?;
        if entry
            .file_type()?
            .is_dir()
        {
            remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    fs::remove_dir(dst)?;
    Ok(())
}

//
// last_modified_recursive
//

pub fn last_modified_recursive(entry: impl AsRef<Path>) -> Result<SystemTime, io::Error> {
    let mut modified = entry
        .as_ref()
        .metadata()?
        .modified()?;
    if entry
        .as_ref()
        .is_dir()
    {
        for entry in fs::read_dir(entry)? {
            let entry = entry?;
            if entry
                .file_type()?
                .is_dir()
            {
                modified = modified.max(last_modified_recursive(
                    entry.path(),
                )?);
            } else {
                modified = modified.max(
                    entry
                        .metadata()?
                        .modified()?,
                );
            }
        }
    }
    Ok(modified)
}

//
// ok_or
//

pub trait BoolGuardExt {
    fn ok_or<E>(self, error: E) -> Result<(), E>;

    fn ok_or_else<E>(self, error: impl FnOnce() -> E) -> Result<(), E>;
}

impl BoolGuardExt for bool {
    fn ok_or<E>(self, error: E) -> Result<(), E> {
        self.then_some(())
            .ok_or(error)
    }

    fn ok_or_else<E>(self, error: impl FnOnce() -> E) -> Result<(), E> {
        self.then_some(())
            .ok_or_else(error)
    }
}

//
// TryReplace
//

pub trait TryReplace: Sized {
    type With: Into<Self>;

    fn try_replace(&mut self, opt: Option<Self::With>) {
        if let Some(value) = opt {
            *self = value.into();
        }
    }
}

impl<T> TryReplace for Option<T> {
    type With = T;
}

impl TryReplace for bool {
    type With = bool;
}

// TODO TryReplace for all primitives

//
// PushFrom
//

pub trait PushFrom<T> {
    fn push_from<E>(&mut self, value: E)
    where
        T: From<E>;
}

impl<T> PushFrom<T> for Vec<T> {
    fn push_from<E>(&mut self, value: E)
    where
        T: From<E>, {
        self.push(From::from(value))
    }
}

//
// SplitIntoTwoWordsExt
//

pub trait SplitIntoTwoWordsExt {
    fn split_into_words<const N: usize>(&self) -> Option<[&Self; N]>;
}

impl SplitIntoTwoWordsExt for str {
    fn split_into_words<const N: usize>(&self) -> Option<[&Self; N]> {
        let mut words = self.split_whitespace();
        let mut results = [""; N];
        for i in 0..N {
            results[i] = words.next()?;
        }
        words
            .next()
            .is_none()
            .then_some(())?;
        Some(results)
    }
}

//
// u16_from_4_hex_chars
//

pub fn u16_from_4_hex_chars(ch1: char, ch2: char, ch3: char, ch4: char) -> Result<u16, ()> {
    let ch1 = if ch1 >= 'a' && ch1 <= 'f' {
        ch1 as u16 - 'a' as u16 + 10
    } else if ch1 >= 'A' && ch1 <= 'F' {
        ch1 as u16 - 'A' as u16 + 10
    } else if ch1 >= '0' && ch1 <= '9' {
        ch1 as u16 - '0' as u16
    } else {
        return Err(());
    };
    let ch2 = if ch2 >= 'a' && ch2 <= 'f' {
        ch2 as u16 - 'a' as u16 + 10
    } else if ch2 >= 'A' && ch2 <= 'F' {
        ch2 as u16 - 'A' as u16 + 10
    } else if ch2 >= '0' && ch2 <= '9' {
        ch2 as u16 - '0' as u16
    } else {
        return Err(());
    };
    let ch3 = if ch3 >= 'a' && ch3 <= 'f' {
        ch3 as u16 - 'a' as u16 + 10
    } else if ch3 >= 'A' && ch3 <= 'F' {
        ch3 as u16 - 'A' as u16 + 10
    } else if ch3 >= '0' && ch3 <= '9' {
        ch3 as u16 - '0' as u16
    } else {
        return Err(());
    };
    let ch4 = if ch4 >= 'a' && ch4 <= 'f' {
        ch4 as u16 - 'a' as u16 + 10
    } else if ch4 >= 'A' && ch4 <= 'F' {
        ch4 as u16 - 'A' as u16 + 10
    } else if ch4 >= '0' && ch4 <= '9' {
        ch4 as u16 - '0' as u16
    } else {
        return Err(());
    };
    Ok(ch1 << 12 | ch2 << 8 | ch3 << 4 | ch4)
}

//
// format_multiline_code
//

pub fn count_indent(text: &str) -> usize {
    let trimmed = text.trim_start();
    text.len() - trimmed.len()
}

pub fn format_multiline_code(text: &str) -> String {
    let to_trim = text
        .lines()
        .skip(1)
        .map(count_indent)
        .min()
        .unwrap_or_default();
    text.lines()
        .map(|line| {
            line.split_at(to_trim)
                .1
        })
        .fold(String::new(), |a, b| {
            a + b + "\n"
        })
        .trim()
        .to_string()
}
