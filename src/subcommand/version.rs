use std::rc::Rc;

use indexmap::IndexMap;

use crate::lsd::Value;
use crate::util::BoolGuardExt;

pub struct Subcommand {}

#[derive(Debug, Clone)]
enum InnerParseError {
    ExpectedNoFlags,
}

impl super::InnerParseError for InnerParseError {
}

impl From<InnerParseError> for Rc<dyn super::InnerParseError> {
    fn from(value: InnerParseError) -> Self { Rc::new(value) }
}

impl super::Subcommand for Subcommand {
    fn parse(
        flags: IndexMap<Value, Rc<[Value]>>,
        _post_dash_dash: impl Iterator<Item = String>,
    ) -> Result<Rc<dyn super::Subcommand>, Rc<dyn super::InnerParseError>> {
        use InnerParseError::*;

        flags
            .is_empty()
            .ok_or(ExpectedNoFlags)?;

        Ok(Rc::new(Subcommand {}))
    }

    fn execute(&self) -> Result<(), Rc<dyn super::InnerExecuteError>> {
        println!(
            "build++ version {}",
            env!("CARGO_PKG_VERSION")
        );
        Ok(())
    }
}
