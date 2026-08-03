#[path = "input.rs"]
mod input;
#[path = "script_generation.rs"]
mod script_generation;
#[path = "result_validation.rs"]
mod result_validation;
#[path = "command_generation.rs"]
mod command_generation;
#[path = "history_parser.rs"]
mod history_parser;
#[path = "url_validation.rs"]
mod url_validation;

#[cfg(test)]
#[path = "logic_tests.rs"]
mod logic_tests;
#[cfg(test)]
#[path = "logic_tests_2.rs"]
mod logic_tests_2;
#[cfg(test)]
#[path = "logic_tests_3.rs"]
mod logic_tests_3;
#[cfg(test)]
#[path = "logic_tests_4.rs"]
mod logic_tests_4;
#[cfg(test)]
#[path = "logic_tests_5.rs"]
mod logic_tests_5;

pub(crate) use input::*;
pub(crate) use script_generation::*;
pub(crate) use result_validation::*;
pub(crate) use command_generation::*;
pub(crate) use history_parser::*;
pub(crate) use url_validation::*;

#[cfg(test)]
const MAX_INPUT_LINE_BYTES: usize = 2_048;
#[cfg(test)]
const MAX_VERSION_CHARS: usize = 64;
#[cfg(test)]
const MAX_RESULT_MESSAGE_CHARS: usize = 512;
#[cfg(test)]
const MAX_HISTORY_SCAN_LINES: usize = crate::models::MAX_HISTORY_RECORDS;
#[cfg(test)]
const MAX_GENERATE_SEARCH_RESULTS: usize = crate::models::MAX_PACKAGE_LINES * 16;
