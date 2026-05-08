use std::{fs::File, io::BufReader, path::Path};

use quote::quote;
use serde::Deserialize;

use crate::{generate::WithSide, protocol::Protocol};

pub mod generate;
pub mod protocol;

#[derive(Clone, Copy)]
pub enum Side {
    Server,
    Client,
}

pub fn generate<'a>(xmls: impl IntoIterator<Item = &'a Path>, side: Side, pretty: bool) -> String {
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
    if pretty {
        prettyplease::unparse(&syn::parse2(token_stream).unwrap())
    } else {
        token_stream.to_string()
    }
}
