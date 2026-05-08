use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};

use crate::{Side, protocol::*};

fn snake_to_pascal(str: &str) -> String {
    str.split("_")
        .map(|x| {
            x.chars()
                .enumerate()
                .map(|(idx, chr)| {
                    if idx == 0 {
                        chr.to_ascii_uppercase()
                    } else {
                        chr
                    }
                })
                .collect::<String>()
        })
        .fold(String::new(), |mut x, y| {
            x.push_str(&y);
            x
        })
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
            pub type #proxy = #name::Proxy;
            #description
            pub mod #name {
                #requests
                #events
                pub mod enums {
                    #(#enums)*
                }

                #[repr(u16)]
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
                unsafe impl ::wayland_oxide::Opcode for RequestOpcode {}

                #[repr(u16)]
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
                unsafe impl ::wayland_oxide::Opcode for EventOpcode {}

                #[repr(transparent)]
                #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                pub struct Proxy(::wayland_oxide::Object);
                impl Proxy {
                    #marshal_methods
                }
                unsafe impl<'a> ::wayland_oxide::HasRequests<'a> for Proxy {
                    type Request = Request<'a>;
                }
                unsafe impl<'a> ::wayland_oxide::HasEvents<'a> for Proxy {
                    type Event = Event<'a>;
                }
                unsafe impl ::wayland_oxide::Proxy for Proxy {
                    fn new(id: ::wayland_oxide::Object) -> Self {
                        Self(id)
                    }
                    fn id(self) -> ::wayland_oxide::Object {
                        self.0
                    }

                    const REQUEST_IS_DESTRUCTOR: &'static [bool] = &[#(#request_is_destructor),*];
                    const EVENT_IS_DESTRUCTOR: &'static [bool] = &[#(#event_is_destructor),*];

                    const INTERFACE: ::wayland_oxide::Interface = INTERFACE;
                }

                pub const INTERFACE: ::wayland_oxide::Interface = ::wayland_oxide::Interface {
                    name: #interface,
                    version: #version,
                };
            }
        });
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
                }
            }
            false => {
                let entries = entries.iter();
                quote! {
                    #description
                    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                    #[repr(u32)]
                    pub enum #name {
                        #(#entries)*
                    }
                }
            }
        });
        tokens.extend(quote! {
            unsafe impl ::wayland_oxide::Trivial for #name {}
        });
    }
}

impl ToTokens for Entry {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Entry {
            name,
            value,
            summary,
            description,
            ..
        } = self;
        let name = snake_to_pascal(name);
        let name = match name.as_bytes().get(0) {
            Some(b'0'..=b'9') => format!("_{name}"),
            _ => name,
        };
        let name = Ident(&name);

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
        let ty = match r#type {
            Type::Int => quote!(i32),
            Type::Uint => match r#enum {
                Some(x) => match x.split_once(".") {
                    Some((interface, name)) => {
                        let name = Ident(&snake_to_pascal(name));
                        let interface = Ident(interface);
                        quote!(super::#interface::enums::#name)
                    }
                    None => {
                        let name = Ident(&snake_to_pascal(x));
                        quote!(enums::#name)
                    }
                },
                None => quote!(u32),
            },
            Type::Fixed => quote!(::wayland_oxide::Fixed),
            Type::String => match allow_null {
                true => quote!(Option<&#lifetime str>),
                false => quote!(&#lifetime str),
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
                None => quote!(::wayland_oxide::GenericNewId<#lifetime>),
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
}

impl Message {
    fn struct_def(&self, kind: MessageKind) -> StructDef<'_> {
        let &Self {
            ref name,
            ref description,
            ref args,
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
        } = self;
        let args = args
            .iter()
            .map(|arg| {
                WithLifetime {
                    arg,
                    lifetime: quote!('a),
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
        } = message.struct_def(kind);
        struct_def.to_tokens(tokens);
        let args = args.iter().map(|arg| Ident(&arg.name));
        let args1 = args.clone();
        tokens.extend(quote! {
            unsafe impl #lifetime ::wayland_oxide::marshal::Marshal for #name #lifetime {
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
        let lifetime = args.iter().any(|x| x.is_reference()).then(|| quote!(<'_>));
        let params = args.iter().map(|arg| Ident(&arg.name));
        let args = args
            .iter()
            .map(|arg| {
                WithLifetime {
                    arg,
                    lifetime: quote!('_),
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
            pub fn #name(self, #(#args)*) -> ::wayland_oxide::Message<Proxy, #message #lifetime> {
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
        } = message.struct_def(kind);
        struct_def.to_tokens(tokens);
        let args = args.iter().map(|arg| Ident(&arg.name));
        tokens.extend(quote! {
            unsafe impl<'a> ::wayland_oxide::unmarshal::Unmarshal<'a> for #name #lifetime {
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
