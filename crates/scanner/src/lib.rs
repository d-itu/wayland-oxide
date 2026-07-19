#![deny(missing_docs)]
//! Generate rust protocol marshalling code from xml.
//!
//! This crate should be used as `build-dependency`
//!
//! # Usage
//!
//! `build.rs`
//! ```
//! use std::{
//!     env, fs,
//!     path::{Path, PathBuf},
//! };
//!
//! use wayland_oxide_scanner::{Scanner, Side};
//!
//! fn main() {
//!     let code = Scanner::new().generate([Path::new("/usr/share/wayland/wayland.xml")], Side::Client);
//!
//!     let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
//!     fs::write(out_dir.join("wayland.rs"), code).unwrap()
//! }
//! ```
//!
//! `main.rs`
//! ```
//! include!(concat!(env!("OUT_DIR"), "/wayland.rs"));
//! ```

use std::{fs::File, io::BufReader, path::Path};

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
        let protocols = xmls
            .into_iter()
            .map(File::open)
            .map(Result::unwrap)
            .map(BufReader::new)
            .map(quick_xml::de::Deserializer::from_reader)
            .map(|mut x| Protocol::deserialize(&mut x).unwrap())
            .map(|protocol| WithSide {
                side,
                item: protocol,
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
