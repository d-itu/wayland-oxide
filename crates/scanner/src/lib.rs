#![deny(missing_docs)]
//! Generate rust protocol marshalling code from xml.
//!
//! This crate should be used as `build-dependency`

use std::{
    error, fmt, fs, io,
    path::{Path, PathBuf},
};

use quick_xml::{NsReader, de::Deserializer};
use quote::quote;
use serde::Deserialize;

use crate::{generate::WithSide, protocol::Protocol};

pub(crate) mod generate;
pub(crate) mod protocol;

/// Code generation kind
#[derive(Clone, Copy)]
pub enum Side {
    /// generate server code
    Server,
    /// generate client code
    Client,
}

/// An error while reading a protocol or generating Rust code.
#[derive(Debug)]
pub enum Error {
    /// A protocol file could not be read.
    Read {
        /// Input file path.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// A protocol file contains invalid XML or field values.
    Parse {
        /// Input file path.
        path: PathBuf,
        /// Underlying deserialization error.
        source: quick_xml::DeError,
    },
    /// Generated tokens could not be parsed as a Rust source file.
    Generate(syn::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(f, "failed to read {}: {source}", path.display()),
            Self::Parse { path, source } => {
                write!(f, "failed to parse {}: {source}", path.display())
            }
            Self::Generate(source) => write!(f, "invalid generated Rust: {source}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(match self {
            Self::Read { source, .. } => source,
            Self::Parse { source, .. } => source,
            Self::Generate(source) => source,
        })
    }
}

/// Code generator configurations.
#[non_exhaustive]
pub struct Scanner {
    pretty: bool,
}

fn normalize(input: &str) -> String {
    let mut result = String::new();
    let mut it = input.split_whitespace().peekable();
    while let Some(s) = it.next() {
        result.push_str(s);
        if it.peek().is_some() {
            result.push(' ');
        }
    }
    result
}

impl Scanner {
    /// Create scanner with default configurations.
    pub fn new() -> Self {
        Self { pretty: true }
    }
    /// Use [`prettyplease`] to format code, default to true.
    #[must_use]
    pub fn pretty(self, pretty: bool) -> Self {
        Self { pretty, ..self }
    }
    /// Generate code from XML files.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or deserializing an input fails, or if
    /// formatting the generated Rust fails.
    pub fn generate<P: AsRef<Path>>(
        &self,
        xmls: impl IntoIterator<Item = P>,
        side: Side,
    ) -> Result<String, Error> {
        let protocols = xmls
            .into_iter()
            .map(|path| {
                let path = path.as_ref();
                let input = fs::read_to_string(path).map_err(|source| Error::Read {
                    path: path.to_owned(),
                    source,
                })?;
                let s = normalize(&input);
                let mut reader = NsReader::from_str(&s);
                reader.config_mut().trim_text(true);
                let mut de = Deserializer::buffering(reader);
                Ok(WithSide {
                    side,
                    item: Protocol::deserialize(&mut de).map_err(|source| Error::Parse {
                        path: path.to_owned(),
                        source,
                    })?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let token_stream = quote! { #(#protocols)* };
        if self.pretty {
            Ok(prettyplease::unparse(
                &syn::parse2(token_stream).map_err(Error::Generate)?,
            ))
        } else {
            Ok(token_stream.to_string())
        }
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}
