#![deny(missing_docs)]
//! Generate rust protocol marshalling code from xml.
//!
//! This crate should be used as `build-dependency`

use std::{fs, path::Path};

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
    /// Generate code from xmls
    #[must_use]
    pub fn generate<P: AsRef<Path>>(
        &self,
        xmls: impl IntoIterator<Item = P>,
        side: Side,
    ) -> String {
        let protocols = xmls.into_iter().map(|path| {
            let s = normalize(&fs::read_to_string(path).unwrap());
            let mut reader = NsReader::from_str(&s);
            reader.config_mut().trim_text(true);
            let mut de = Deserializer::buffering(reader);
            WithSide {
                side,
                item: Protocol::deserialize(&mut de).unwrap(),
            }
        });
        let token_stream = quote! { #(#protocols)* };
        if self.pretty {
            prettyplease::unparse(&syn::parse2(token_stream).unwrap())
        } else {
            token_stream.to_string()
        }
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}
