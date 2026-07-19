use serde::Deserialize;

#[allow(unused)]
#[derive(Deserialize, Debug)]
pub struct Protocol {
    #[serde(rename = "@name")]
    pub name: String,
    pub copyright: Option<Copyright>,
    pub description: Option<Description>,
    #[serde(rename = "interface", default)]
    pub interfaces: Vec<Interface>,
}

#[allow(unused)]
#[derive(Deserialize, Debug)]
pub struct Copyright(#[serde(rename = "$text", default)] pub String);

#[allow(unused)]
#[derive(Deserialize, Debug)]
pub struct Interface {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@version")]
    pub version: Number,
    pub description: Option<Description>,
    #[serde(rename = "request", default)]
    pub requests: Vec<Message>,
    #[serde(rename = "event", default)]
    pub events: Vec<Message>,
    #[serde(rename = "enum", default)]
    pub enums: Vec<Enum>,
}

#[doc(alias = "Request")]
#[doc(alias = "Event")]
#[derive(Deserialize, Debug)]
#[allow(unused)]
pub struct Message {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@type")]
    pub r#type: Option<MessageType>,
    #[serde(rename = "@since")]
    pub since: Option<Number>,
    #[serde(rename = "@deprecated-since")]
    pub deprecated_since: Option<Number>,
    pub description: Option<Description>,
    #[serde(rename = "arg", default)]
    pub args: Vec<Arg>,
}

#[derive(Deserialize, Debug, PartialEq)]
pub enum MessageType {
    #[serde(rename = "destructor")]
    Destructor,
}

#[derive(Deserialize, Debug)]
#[allow(unused)]
pub struct Enum {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@since")]
    pub since: Option<Number>,
    #[serde(rename = "@bitfield", default)]
    pub bitfield: bool,
    pub description: Option<Description>,
    #[serde(rename = "entry", default)]
    pub entries: Vec<Entry>,
}

#[derive(Deserialize, Debug)]
#[allow(unused)]
pub struct Entry {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@value")]
    pub value: Number,
    #[serde(rename = "@summary")]
    pub summary: Option<String>,
    #[serde(rename = "@since")]
    pub since: Option<Number>,
    #[serde(rename = "@deprecated-since")]
    pub deprecated_since: Option<Number>,
    pub description: Option<Description>,
}

#[derive(Deserialize, Debug)]
#[allow(unused)]
pub struct Arg {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@type")]
    pub r#type: Type,
    #[serde(rename = "@summary", default)]
    pub summary: String,
    #[serde(rename = "@interface")]
    pub interface: Option<String>,
    #[serde(rename = "@allow-null", default)]
    pub allow_null: bool,
    #[serde(rename = "@enum")]
    pub r#enum: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
#[allow(unused)]
pub struct Description {
    #[serde(rename = "@summary")]
    pub summary: Option<String>,
    #[serde(rename = "$text")]
    pub inner: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Type {
    Int,
    Uint,
    Fixed,
    String,
    Object,
    NewId,
    Array,
    Fd,
}

impl Arg {
    pub const fn is_reference(&self) -> bool {
        match self.r#type {
            Type::String => true,
            Type::Array => true,
            Type::NewId => self.interface.is_none(),
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct Number(pub u32);

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = s.as_bytes();
        if bytes[0] == b'0' && bytes.get(1).copied() == Some(b'x') {
            u32::from_str_radix(&s[2..], 16)
        } else {
            s.parse()
        }
        .map(Number)
        .map_err(serde::de::Error::custom)
    }
}
