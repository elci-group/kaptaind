//! Minimal, dependency-free replacement for the `colored` crate.
//!
//! Provides the `Colorize` extension trait so existing code can keep using
//! `.red()`, `.bold()`, etc. The implementation respects `NO_COLOR` and disables
//! styling when stdout is not a terminal, matching `colored`'s default behavior.

use std::fmt::{self, Display};
use std::io::IsTerminal;

fn should_colorize() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

/// A string tagged with one or more ANSI SGR codes.
#[derive(Clone, Debug)]
pub struct StyledString {
    text: String,
    codes: Vec<u8>,
}

impl StyledString {
    fn new(s: impl Into<String>, code: u8) -> Self {
        Self {
            text: s.into(),
            codes: vec![code],
        }
    }

    fn with(mut self, code: u8) -> Self {
        self.codes.push(code);
        self
    }
}

impl Display for StyledString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !should_colorize() || self.codes.is_empty() {
            return self.text.fmt(f);
        }
        write!(f, "\x1b[")?;
        for (i, code) in self.codes.iter().enumerate() {
            if i > 0 {
                write!(f, ";")?;
            }
            write!(f, "{}", code)?;
        }
        write!(f, "m{}\x1b[0m", self.text)
    }
}

/// Extension trait that adds terminal styling methods to strings.
///
/// Implemented for `&str`, `String`, and `StyledString` so styles can be chained
/// (e.g. `"text".green().bold()`).
pub trait Colorize {
    fn red(self) -> StyledString;
    fn green(self) -> StyledString;
    fn blue(self) -> StyledString;
    fn cyan(self) -> StyledString;
    fn magenta(self) -> StyledString;
    fn yellow(self) -> StyledString;
    fn black(self) -> StyledString;
    fn white(self) -> StyledString;
    fn bright_black(self) -> StyledString;
    fn bold(self) -> StyledString;
    fn dimmed(self) -> StyledString;
    /// Reset/no-op style. Provided for compatibility with code that uses
    /// `colored`'s `.normal()` method.
    fn normal(self) -> StyledString;
}

macro_rules! impl_colorize {
    ($ty:ty) => {
        impl Colorize for $ty {
            fn red(self) -> StyledString {
                StyledString::new(self, 31)
            }
            fn green(self) -> StyledString {
                StyledString::new(self, 32)
            }
            fn blue(self) -> StyledString {
                StyledString::new(self, 34)
            }
            fn cyan(self) -> StyledString {
                StyledString::new(self, 36)
            }
            fn magenta(self) -> StyledString {
                StyledString::new(self, 35)
            }
            fn yellow(self) -> StyledString {
                StyledString::new(self, 33)
            }
            fn black(self) -> StyledString {
                StyledString::new(self, 30)
            }
            fn white(self) -> StyledString {
                StyledString::new(self, 37)
            }
            fn bright_black(self) -> StyledString {
                StyledString::new(self, 90)
            }
            fn bold(self) -> StyledString {
                StyledString::new(self, 1)
            }
            fn dimmed(self) -> StyledString {
                StyledString::new(self, 2)
            }
            fn normal(self) -> StyledString {
                StyledString::new(self, 0)
            }
        }
    };
}

impl_colorize!(&str);
impl_colorize!(String);

impl Colorize for &String {
    fn red(self) -> StyledString {
        self.as_str().red()
    }
    fn green(self) -> StyledString {
        self.as_str().green()
    }
    fn blue(self) -> StyledString {
        self.as_str().blue()
    }
    fn cyan(self) -> StyledString {
        self.as_str().cyan()
    }
    fn magenta(self) -> StyledString {
        self.as_str().magenta()
    }
    fn yellow(self) -> StyledString {
        self.as_str().yellow()
    }
    fn black(self) -> StyledString {
        self.as_str().black()
    }
    fn white(self) -> StyledString {
        self.as_str().white()
    }
    fn bright_black(self) -> StyledString {
        self.as_str().bright_black()
    }
    fn bold(self) -> StyledString {
        self.as_str().bold()
    }
    fn dimmed(self) -> StyledString {
        self.as_str().dimmed()
    }
    fn normal(self) -> StyledString {
        self.as_str().normal()
    }
}

impl Colorize for StyledString {
    fn red(self) -> StyledString {
        self.with(31)
    }
    fn green(self) -> StyledString {
        self.with(32)
    }
    fn blue(self) -> StyledString {
        self.with(34)
    }
    fn cyan(self) -> StyledString {
        self.with(36)
    }
    fn magenta(self) -> StyledString {
        self.with(35)
    }
    fn yellow(self) -> StyledString {
        self.with(33)
    }
    fn black(self) -> StyledString {
        self.with(30)
    }
    fn white(self) -> StyledString {
        self.with(37)
    }
    fn bright_black(self) -> StyledString {
        self.with(90)
    }
    fn bold(self) -> StyledString {
        self.with(1)
    }
    fn dimmed(self) -> StyledString {
        self.with(2)
    }
    fn normal(self) -> StyledString {
        Self {
            text: self.text,
            codes: Vec::new(),
        }
    }
}
