use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};

use crate::{Side, protocol::*};

fn snake_to_pascal(str: &str) -> String {
    str.split("_")
        .flat_map(|x| {
            x.chars().enumerate().map(|(idx, chr)| {
                if idx == 0 {
                    chr.to_ascii_uppercase()
                } else {
                    chr
                }
            })
        })
        .collect()
}

pub struct WithSide<T> {
    pub side: Side,
    pub item: T,
}

impl ToTokens for WithSide<Protocol> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let side = self.side;
        let interfaces = self
            .item
            .interfaces
            .iter()
            .map(|item| WithSide { side, item });
        tokens.extend(quote! {
            #(#interfaces)*
        })
    }
}

struct Marshal<T>(T);
struct Unmarshal<T>(T);
struct MarshalMethods<T>(T);

impl ToTokens for WithSide<&Interface> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Interface {
            name,
            version,
            description,
            requests,
            events,
            enums,
        } = self.item;
        let interface = name;
        let name = Ident(name);

        let requests_opcode = requests.iter().map(|msg| Ident(snake_to_pascal(&msg.name)));
        let events_opcode = events.iter().map(|msg| Ident(snake_to_pascal(&msg.name)));

        let requests_enum = requests.iter().map(MessageEnum);
        let events_enum = events.iter().map(MessageEnum);

        fn enum_match(messages: &[Message]) -> impl Iterator<Item = impl ToTokens> {
            messages.iter().enumerate().map(move |(idx, message)| {
                let message = Ident(snake_to_pascal(&message.name));
                let idx = idx as u16;
                quote! {
                    #idx => Self::#message(::wayland_oxide::Lazy::new(reader)),
                }
            })
        }

        let requests_enum_match = enum_match(requests);
        let events_enum_match = enum_match(events);
        let request_opcode_impl = opcode_impl("RequestOpcode");
        let event_opcode_impl = opcode_impl("EventOpcode");

        let request_is_destructor = requests
            .iter()
            .map(|msg| msg.r#type == Some(MessageType::Destructor));
        let event_is_destructor = events
            .iter()
            .map(|msg| msg.r#type == Some(MessageType::Destructor));

        let (requests, events, marshal_methods) = match self.side {
            Side::Server => {
                let marshal_methods = events.iter().map(MarshalMethods);
                let requests = requests
                    .iter()
                    .map(|x| Unmarshal((x, MessageKind::Request)));
                let events = events.iter().map(|x| Marshal((x, MessageKind::Event)));
                (
                    quote!(#(#requests)*),
                    quote!(#(#events)*),
                    quote!(#(#marshal_methods)*),
                )
            }
            Side::Client => {
                let marshal_methods = requests.iter().map(MarshalMethods);
                let requests = requests.iter().map(|x| Marshal((x, MessageKind::Request)));
                let events = events.iter().map(|x| Unmarshal((x, MessageKind::Event)));
                (
                    quote!(#(#requests)*),
                    quote!(#(#events)*),
                    quote!(#(#marshal_methods)*),
                )
            }
        };
        let proxy = Ident(snake_to_pascal(interface));
        tokens.extend(quote! {
            #[repr(transparent)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
            pub struct #proxy(::wayland_oxide::Object);

            #description
            pub mod #name {
                #requests
                #events
                pub mod enums {
                    #(#enums)*
                }

                #[repr(u16)]
                #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                pub enum RequestOpcode {
                    #(#requests_opcode,)*
                    SIZE,
                }
                pub enum Request<'a> {
                    #(#requests_enum)*
                    _Unknown(::wayland_oxide::Reader<'a>),
                }
                impl<'a> ::wayland_oxide::MessageEnum<'a> for Request<'a> {
                    fn new(opcode: u16, reader: ::wayland_oxide::Reader<'a>) -> Self {
                        match opcode {
                            #(#requests_enum_match)*
                            _ => Self::_Unknown(reader)
                        }
                    }
                }
                #request_opcode_impl

                #[repr(u16)]
                #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                pub enum EventOpcode {
                    #(#events_opcode,)*
                    SIZE,
                }
                pub enum Event<'a> {
                    #(#events_enum)*
                    _Unknown(::wayland_oxide::Reader<'a>),
                }
                impl<'a> ::wayland_oxide::MessageEnum<'a> for Event<'a> {
                    fn new(opcode: u16, reader: ::wayland_oxide::Reader<'a>) -> Self {
                        match opcode {
                            #(#events_enum_match)*
                            _ => Self::_Unknown(reader)
                        }
                    }
                }
                #event_opcode_impl

                pub type Proxy = super::#proxy;
                impl Proxy {
                    pub const fn new(object: ::wayland_oxide::Object) -> Self {
                        Self(object)
                    }

                    #marshal_methods
                }
                impl<'a> ::wayland_oxide::HasRequests<'a> for Proxy {
                    type Request = Request<'a>;
                }
                impl<'a> ::wayland_oxide::HasEvents<'a> for Proxy {
                    type Event = Event<'a>;
                }
                impl Into<::wayland_oxide::Object> for Proxy {
                    fn into(self) -> ::wayland_oxide::Object {
                        self.0
                    }
                }
                impl From<::wayland_oxide::Object> for Proxy {
                    fn from(value: ::wayland_oxide::Object) -> Self {
                        Self(value)
                    }
                }
                impl ::wayland_oxide::Proxy for Proxy {
                    const REQUEST_IS_DESTRUCTOR: &'static [bool] = &[#(#request_is_destructor),*];
                    const EVENT_IS_DESTRUCTOR: &'static [bool] = &[#(#event_is_destructor),*];

                    const INTERFACE: ::wayland_oxide::Interface = INTERFACE;
                }
                unsafe impl ::wayland_oxide::TrivialMarshal for Proxy {}
                impl ::wayland_oxide::Unmarshal<'_> for Proxy {
                    fn unmarshal(reader: &mut ::wayland_oxide::unmarshal::Reader) -> ::wayland_oxide::unmarshal::Result<Self> {
                        Ok(Self(reader.read()?))
                    }
                }

                pub const INTERFACE: ::wayland_oxide::Interface = ::wayland_oxide::Interface {
                    name: #interface,
                    version: #version,
                };
            }
        });
    }
}

fn opcode_impl(name: &str) -> TokenStream {
    let name = Ident(name);
    quote! {
        impl ::wayland_oxide::Opcode for #name {
            fn as_u16(&self) -> u16 {
                *self as u16
            }
        }
    }
}

impl ToTokens for Enum {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Enum {
            name,
            description,
            entries,
            ..
        } = self;
        let name = snake_to_pascal(name);
        let name = Ident(&name);
        tokens.extend(match self.bitfield {
            true => {
                let entries = entries.iter().map(BitField);
                quote! {
                    ::wayland_oxide::bitflags! {
                        #description
                        #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                        pub struct #name: u32 {
                            #(#entries)*
                        }
                    }
                    unsafe impl ::wayland_oxide::Trivial for #name {}
                    impl ::wayland_oxide::ProtocolEnum for #name {
                        type Wire = Self;
                    }
                }
            }
            false => {
                let cases = entries.iter().map(|entry| {
                    let variant = entry.ident();
                    let value = &entry.value;
                    quote!(#value => Ok(Self::#variant),)
                });
                let entries = entries.iter();
                quote! {
                    #description
                    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                    #[repr(u32)]
                    pub enum #name {
                        #(#entries)*
                    }
                    unsafe impl ::wayland_oxide::TrivialMarshal for #name {}
                    impl ::wayland_oxide::ProtocolEnum for #name {
                        type Wire = ::wayland_oxide::WireEnum<Self>;
                    }
                    impl TryFrom<u32> for #name {
                        type Error = u32;

                        fn try_from(value: u32) -> Result<Self, Self::Error> {
                            match value {
                                #(#cases)*
                                value => Err(value),
                            }
                        }
                    }
                }
            }
        });
    }
}

impl Entry {
    fn ident(&self) -> Ident<String> {
        let name = snake_to_pascal(&self.name);
        Ident(match name.as_bytes().first() {
            Some(b'0'..=b'9') => format!("_{name}"),
            _ => name,
        })
    }
}

impl ToTokens for Entry {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Entry {
            value,
            summary,
            description,
            ..
        } = self;
        let name = self.ident();

        let summary = summary.as_ref().map(|x| {
            quote! {
                #[doc = #x]
            }
        });
        tokens.extend(quote! {
            #summary
            #description
            #name = #value,
        });
    }
}

struct BitField<T>(T);

impl ToTokens for BitField<&Entry> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Entry {
            name,
            value,
            summary,
            description,
            ..
        } = self.0;
        let name = name.to_uppercase();
        let name = Ident(&name);

        let summary = summary.as_ref().map(|x| {
            quote! {
                #[doc = #x]
            }
        });
        tokens.extend(quote! {
            #summary
            #description
            const #name = #value;
        });
    }
}

impl ToTokens for Description {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let text = self
            .inner
            .as_deref()
            .or(self.summary.as_deref())
            .unwrap_or_default();
        tokens.extend(quote! {
            #[doc = #text]
        });
    }
}

struct WithLifetime<'a> {
    arg: &'a Arg,
    lifetime: TokenStream,
    decoding: bool,
}

struct ArgResult<'a> {
    summary: &'a str,
    name: Ident<&'a str>,
    ty: TokenStream,
}

impl<'a> WithLifetime<'a> {
    fn generate(&self) -> ArgResult<'a> {
        let &Arg {
            ref name,
            ref r#type,
            ref summary,
            ref interface,
            allow_null,
            ref r#enum,
        } = self.arg;
        let lifetime = &self.lifetime;
        let string = if self.decoding {
            quote!(::wayland_oxide::WireString<#lifetime>)
        } else {
            quote!(&#lifetime str)
        };
        let ty = match r#type {
            Type::Int => quote!(i32),
            Type::Uint => match r#enum {
                Some(x) => {
                    let ty = match x.split_once(".") {
                        Some((interface, name)) => {
                            let name = Ident(&snake_to_pascal(name));
                            let interface = Ident(interface);
                            quote!(super::#interface::enums::#name)
                        }
                        None => {
                            let name = Ident(&snake_to_pascal(x));
                            quote!(enums::#name)
                        }
                    };
                    if self.decoding {
                        quote!(<#ty as ::wayland_oxide::ProtocolEnum>::Wire)
                    } else {
                        ty
                    }
                }
                None => quote!(u32),
            },
            Type::Fixed => quote!(::wayland_oxide::Fixed),
            Type::String => match allow_null {
                true => quote!(Option<#string>),
                false => string.clone(),
            },
            Type::Object => {
                let proxy = match interface {
                    Some(iface) => {
                        let iface = Ident(iface);
                        quote!(super::#iface::Proxy)
                    }
                    None => quote!(::wayland_oxide::Object),
                };
                match allow_null {
                    true => quote!(Option<#proxy>),
                    false => proxy,
                }
            }
            Type::NewId => match interface {
                Some(iface) => {
                    let iface = Ident(iface);
                    quote!(super::#iface::Proxy)
                }
                None => quote!(::wayland_oxide::GenericNewId<#string>),
            },
            Type::Array => quote!(&#lifetime [u8]),
            Type::Fd => quote!(::wayland_oxide::Fd),
        };
        ArgResult {
            summary,
            name: Ident(name),
            ty,
        }
    }
}

#[derive(Clone, Copy)]
enum MessageKind {
    Request,
    Event,
}

impl ToTokens for MessageKind {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(match self {
            MessageKind::Request => quote!(Request),
            MessageKind::Event => quote!(Event),
        });
    }
}

struct StructDef<'a> {
    name: Ident<String>,
    kind: MessageKind,
    description: Option<&'a Description>,
    lifetime: Option<TokenStream>,
    args: &'a [Arg],
    decoding: bool,
}

impl Message {
    fn struct_def(&self, kind: MessageKind, decoding: bool) -> StructDef<'_> {
        let Self {
            name,
            description,
            args,
            ..
        } = self;
        let name = Ident(snake_to_pascal(name));
        let lifetime = self
            .args
            .iter()
            .any(Arg::is_reference)
            .then_some(quote!(<'a>));
        StructDef {
            name,
            kind,
            description: description.as_ref(),
            lifetime,
            args,
            decoding,
        }
    }
}

impl ToTokens for StructDef<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let StructDef {
            name,
            kind,
            description,
            lifetime,
            args,
            decoding,
        } = self;
        let args = args
            .iter()
            .map(|arg| {
                WithLifetime {
                    arg,
                    lifetime: quote!('a),
                    decoding: *decoding,
                }
                .generate()
            })
            .map(|ArgResult { summary, name, ty }| {
                quote! {
                    #[doc = #summary]
                    pub #name: #ty,
                }
            });
        let kind = match kind {
            MessageKind::Request => quote!(RequestOpcode),
            MessageKind::Event => quote!(EventOpcode),
        };
        tokens.extend(quote! {
            #description
            #[derive(Debug)]
            pub struct #name #lifetime {
                #(#args)*
            }

            impl #lifetime ::wayland_oxide::MessageBody for #name #lifetime {
                type Opcode = #kind;
                const OPCODE: Self::Opcode = #kind::#name;
            }
        })
    }
}

impl ToTokens for Marshal<(&Message, MessageKind)> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let (message, kind) = self.0;
        let ref struct_def @ StructDef {
            ref name,
            ref lifetime,
            args,
            ..
        } = message.struct_def(kind, false);
        struct_def.to_tokens(tokens);
        let args = args.iter().map(|arg| Ident(&arg.name));
        let args1 = args.clone();
        tokens.extend(quote! {
            impl #lifetime ::wayland_oxide::marshal::Marshal for #name #lifetime {
                fn calc_size(&self) -> usize {
                    0 #(+self.#args.calc_size())*
                }
                unsafe fn marshal(&self, _writer: &mut ::wayland_oxide::marshal::Writer) {
                    #(unsafe { _writer.write(&self.#args1) };)*
                }
            }
        });
    }
}

impl ToTokens for MarshalMethods<&Message> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Message { name, args, .. } = self.0;
        let message = Ident(snake_to_pascal(name));
        let name = Ident(name);
        let lifetime = args.iter().any(|x| x.is_reference()).then(|| quote!(<'a>));
        let params = args.iter().map(|arg| Ident(&arg.name));
        let args = args
            .iter()
            .map(|arg| {
                WithLifetime {
                    arg,
                    lifetime: quote!('a),
                    decoding: false,
                }
                .generate()
            })
            .map(|ArgResult { name, ty, .. }| {
                quote! {
                    #name: #ty,
                }
            });
        tokens.extend(quote! {
            #[must_use]
            pub fn #name #lifetime(self, #(#args)*) -> ::wayland_oxide::Message<Proxy, #message #lifetime> {
                ::wayland_oxide::Message::new(self, #message { #(#params,)* })
            }
        });
    }
}

struct MessageEnum<T>(T);

impl ToTokens for MessageEnum<&Message> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let message = self.0;
        let lifetime = message
            .args
            .iter()
            .any(Arg::is_reference)
            .then_some(quote!(<'a>));
        let message = Ident(snake_to_pascal(&message.name));
        tokens.extend(quote! {
            #message(wayland_oxide::Lazy<'a, #message #lifetime>),
        });
    }
}

impl ToTokens for Unmarshal<(&Message, MessageKind)> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let (message, kind) = self.0;
        let ref struct_def @ StructDef {
            ref name,
            ref lifetime,
            args,
            ..
        } = message.struct_def(kind, true);
        struct_def.to_tokens(tokens);
        let args = args.iter().map(|arg| Ident(&arg.name));
        tokens.extend(quote! {
            impl<'a> ::wayland_oxide::unmarshal::Unmarshal<'a> for #name #lifetime {
                fn unmarshal(_reader: &mut ::wayland_oxide::unmarshal::Reader<'a>) -> ::wayland_oxide::unmarshal::Result<Self> {
                    Ok(Self {
                        #(#args: _reader.read()?,)*
                    })
                }
            }
        });
    }
}

impl ToTokens for Number {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens)
    }
}

struct Ident<T>(T);

impl<T: AsRef<str>> ToTokens for Ident<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let id = syn::parse_str::<syn::Ident>(self.0.as_ref())
            .unwrap_or_else(|_| syn::Ident::new_raw(self.0.as_ref(), Span::call_site()));
        tokens.extend(Some(id));
    }
}
