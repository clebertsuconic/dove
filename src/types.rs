/*
 * Copyright 2019, Ulf Lilleengen
 * License: Apache License 2.0 (see the file LICENSE or http://apache.org/licenses/LICENSE-2.0.html).
 */

use byteorder::NetworkEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::io::Read;
use std::io::Write;
use std::iter::FromIterator;
use std::vec::Vec;

use crate::error::*;

const DESC_ERROR: Value = Value::Ulong(0x1D);

pub trait Encoder {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode>;
}

#[derive(Clone, PartialEq, Debug, PartialOrd, Ord, Eq)]
pub struct Symbol {
    data: Vec<u8>,
}

impl Symbol {
    pub fn from_slice(data: &[u8]) -> Symbol {
        let mut vec = Vec::new();
        vec.extend_from_slice(data);
        return Symbol { data: vec };
    }

    pub fn from_str(data: &str) -> Symbol {
        let mut vec = Vec::new();
        vec.extend_from_slice(data.as_bytes());
        return Symbol { data: vec };
    }

    pub fn from_vec(data: Vec<u8>) -> Symbol {
        return Symbol { data };
    }

    pub fn from_string(data: &str) -> Symbol {
        let mut vec = Vec::new();
        vec.extend_from_slice(data.as_bytes());
        return Symbol { data: vec };
    }

    pub fn to_slice(&self) -> &[u8] {
        return &self.data[..];
    }
}

#[derive(Clone, PartialEq, Debug, PartialOrd, Ord, Eq)]
pub enum ValueRef<'a> {
    Described(Box<ValueRef<'a>>, Box<ValueRef<'a>>),
    Null,
    Bool(&'a bool),
    Ubyte(&'a u8),
    Ushort(&'a u16),
    Uint(&'a u32),
    Ulong(&'a u64),
    Byte(&'a i8),
    Short(&'a i16),
    Int(&'a i32),
    Long(&'a i64),
    String(&'a str),
    Binary(&'a [u8]),
    Symbol(&'a Symbol),
    SymbolRef(&'a str),
    Array(&'a Vec<ValueRef<'a>>),
    List(&'a Vec<ValueRef<'a>>),
    Map(&'a BTreeMap<ValueRef<'a>, ValueRef<'a>>),
}

#[derive(Clone, PartialEq, Debug, PartialOrd, Ord, Eq)]
pub enum Value {
    Described(Box<Value>, Box<Value>),
    Null,
    Bool(bool),
    Ubyte(u8),
    Ushort(u16),
    Uint(u32),
    Ulong(u64),
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    String(String),
    Binary(Vec<u8>),
    Symbol(Symbol),
    Array(Vec<Value>),
    List(Vec<Value>),
    Map(BTreeMap<Value, Value>),
}

#[derive(Clone, PartialEq, Debug, PartialOrd, Ord, Eq, Copy)]
pub enum ValueKind {
    Null,
    Bool,
    Ubyte,
    Ushort,
    Uint,
    Ulong,
    Byte,
    Short,
    Int,
    Long,
    String,
    Binary,
    Symbol,
}

impl Value {
    pub fn try_to_u32(self: &Self) -> Option<u32> {
        return Some(0);
    }
    pub fn try_to_u16(self: &Self) -> Option<u16> {
        return Some(0);
    }
    pub fn try_to_string(self: &Self) -> Option<String> {
        match self {
            Value::String(v) => Some(v.clone()),
            Value::Symbol(v) => {
                Some(String::from_utf8(v.data.clone()).expect("Error decoding symbol"))
            }
            _ => None,
        }
    }
    pub fn to_string(self: &Self) -> String {
        self.try_to_string().expect("Unexpected type!")
    }

    fn value_ref(&self) -> ValueRef {
        match self {
            Value::Described(ref descriptor, ref value) => ValueRef::Described(
                Box::new(descriptor.value_ref()),
                Box::new(value.value_ref()),
            ),
            Value::Null => ValueRef::Null,
            Value::Bool(ref value) => ValueRef::Bool(value),
            Value::String(ref value) => ValueRef::String(value),
            Value::Symbol(ref value) => ValueRef::Symbol(value),
            Value::Ulong(ref value) => ValueRef::Ulong(value),
            //            Value::List(ref value) => ValueRef::List(value),
            _ => ValueRef::Null,
        }
    }
}

#[allow(dead_code)]
pub struct FrameDecoder<'a> {
    desc: &'a Value,
    args: &'a mut Vec<Value>,
}

impl<'a> FrameDecoder<'a> {
    pub fn new(desc: &'a Value, input: &'a mut Value) -> Result<FrameDecoder<'a>> {
        if let Value::List(args) = input {
            return Ok(FrameDecoder {
                desc: desc,
                args: args,
            });
        } else {
            return Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Error decoding frame arguments"),
            ));
        }
    }

    pub fn decode_required<T: TryFrom<Value, Error = AmqpError>>(
        &mut self,
        value: &mut T,
    ) -> Result<()> {
        self.decode(value, true)
    }

    pub fn decode_optional<T: TryFrom<Value, Error = AmqpError>>(
        &mut self,
        value: &mut T,
    ) -> Result<()> {
        self.decode(value, false)
    }

    pub fn decode<T: TryFrom<Value, Error = AmqpError>>(
        &mut self,
        value: &mut T,
        required: bool,
    ) -> Result<()> {
        let mut drained = self.args.drain(0..0);
        if let Some(arg) = drained.next() {
            let v = arg;
            *value = T::try_from(v)?;
        } else if required {
            return Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Decoded null value for required argument"),
            ));
        }
        Ok(())
    }
}

pub struct FrameEncoder {
    desc: Value,
    args: Vec<u8>,
    nelems: usize,
}

impl FrameEncoder {
    pub fn new(desc: Value) -> FrameEncoder {
        return FrameEncoder {
            desc: desc,
            args: Vec::new(),
            nelems: 0,
        };
    }

    pub fn encode_arg(&mut self, arg: &dyn Encoder) -> Result<()> {
        arg.encode(&mut self.args)?;
        self.nelems += 1;
        Ok(())
    }

    /*
    pub fn encode_arg(&mut self, arg: &dyn Encoder, kind: ValueKind) -> Result<()> {
        // F(arg).encode(self.buffer)?;
        match kind {
            Null => Value::Null.encode(writer),
            Bool =>
        }
        self.nelems += 1;
        Ok(())
    }
    */
}

const U8_MAX: usize = std::u8::MAX as usize;
const I8_MAX: usize = std::i8::MAX as usize;
const LIST8_MAX: usize = (std::u8::MAX as usize) - 1;
const LIST32_MAX: usize = (std::u32::MAX as usize) - 4;

impl std::convert::TryFrom<Value> for u32 {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::Uint(v) => return Ok(v),
            _ => Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Error converting value to u32"),
            )),
        }
    }
}

impl std::convert::TryFrom<Value> for Option<u32> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::Null => Ok(None),
            v => Ok(Some(u32::try_from(v)?)),
        }
    }
}

impl std::convert::TryFrom<Value> for u16 {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::Ushort(v) => return Ok(v),
            _ => Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Error converting value to u32"),
            )),
        }
    }
}

impl std::convert::TryFrom<Value> for Option<u16> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        Ok(match value {
            Value::Null => None,
            v => Some(u16::try_from(v)?),
        })
    }
}

impl std::convert::TryFrom<Value> for String {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::String(v) => return Ok(v),
            _ => Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Error converting value to String"),
            )),
        }
    }
}

impl std::convert::TryFrom<Value> for Option<String> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        Ok(match value {
            Value::Null => None,
            v => Some(String::try_from(v)?),
        })
    }
}

impl std::convert::TryFrom<Value> for Symbol {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::Symbol(v) => Ok(v),
            _ => Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Error converting value to Symbol"),
            )),
        }
    }
}

impl std::convert::TryFrom<Value> for Option<Symbol> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        Ok(match value {
            Value::Null => None,
            v => Some(Symbol::try_from(v)?),
        })
    }
}

impl std::convert::TryFrom<Value> for Vec<Symbol> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::Array(v) => {
                let (results, errors): (Vec<_>, Vec<_>) = v
                    .into_iter()
                    .map(|f| Symbol::try_from(f))
                    .partition(Result::is_ok);
                if errors.len() > 0 {
                    return Err(AmqpError::amqp_error(
                        condition::DECODE_ERROR,
                        Some("Error decoding some elements"),
                    ));
                } else {
                    return Ok(results.into_iter().map(Result::unwrap).collect());
                }
            }
            _ => Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Error converting value to Vec<Symbol>"),
            )),
        }
    }
}

impl std::convert::TryFrom<Value> for Option<Vec<Symbol>> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        Ok(match value {
            Value::Null => None,
            v => Some(Vec::try_from(v)?),
        })
    }
}

impl std::convert::TryFrom<Value> for BTreeMap<String, Value> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::Map(v) => {
                let mut m = BTreeMap::new();
                for (key, value) in v.into_iter() {
                    m.insert(String::try_from(key)?, value);
                }
                Ok(m)
            }
            _ => Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Error converting value to Vec<Symbol>"),
            )),
        }
    }
}

impl std::convert::TryFrom<Value> for Option<BTreeMap<String, Value>> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        Ok(match value {
            Value::Null => None,
            v => Some(BTreeMap::try_from(v)?),
        })
    }
}

impl std::convert::TryFrom<Value> for ErrorCondition {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        if let Value::Described(descriptor, list) = value {
            match *descriptor {
                DESC_ERROR => {
                    if let Value::List(args) = *list {
                        let mut it = args.iter();
                        let mut error_condition = ErrorCondition {
                            condition: String::new(),
                            description: String::new(),
                        };

                        if let Some(condition) = it.next() {
                            error_condition.condition = condition.to_string();
                        }

                        if let Some(description) = it.next() {
                            error_condition.description = description.to_string();
                        }
                        Ok(error_condition)
                    } else {
                        Err(AmqpError::decode_error(Some(
                            "Expected list with condition and description",
                        )))
                    }
                }
                _ => Err(AmqpError::decode_error(Some(
                    format!("Expected error descriptor but found {:?}", *descriptor).as_str(),
                ))),
            }
        } else {
            Err(AmqpError::decode_error(Some(
                "Missing expected error descriptor",
            )))
        }
    }
}

impl std::convert::TryFrom<Value> for Option<ErrorCondition> {
    type Error = AmqpError;
    fn try_from(value: Value) -> Result<Self> {
        Ok(match value {
            Value::Null => None,
            v => Some(ErrorCondition::try_from(v)?),
        })
    }
}

impl Encoder for Symbol {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        ValueRef::Symbol(self).encode(writer)
    }
}

impl Encoder for Vec<String> {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        let mut values = Vec::new();
        for s in self.iter() {
            values.push(ValueRef::String(s));
        }
        ValueRef::Array(&values).encode(writer)
    }
}

impl Encoder for Vec<Symbol> {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        let mut values = Vec::new();
        for sym in self.iter() {
            values.push(ValueRef::Symbol(sym));
        }
        ValueRef::Array(&values).encode(writer)
    }
}

impl Encoder for ErrorCondition {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        let mut encoder = FrameEncoder::new(DESC_ERROR);
        encoder.encode_arg(&self.condition)?;
        encoder.encode_arg(&self.description)?;
        encoder.encode(writer)
    }
}

impl Encoder for Vec<u8> {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        ValueRef::Binary(self).encode(writer)
    }
}

impl Encoder for &[u8] {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        ValueRef::Binary(self).encode(writer)
    }
}

impl Encoder for String {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        ValueRef::String(self).encode(writer)
    }
}

impl Encoder for bool {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        ValueRef::Bool(self).encode(writer)
    }
}

impl Encoder for u64 {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        ValueRef::Ulong(self).encode(writer)
    }
}

impl Encoder for u32 {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        ValueRef::Uint(self).encode(writer)
    }
}

impl Encoder for u16 {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        ValueRef::Ushort(self).encode(writer)
    }
}

impl<T: Encoder> Encoder for Option<T> {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        match self {
            Some(value) => value.encode(writer),
            _ => Value::Null.encode(writer),
        }
    }
}

impl Encoder for BTreeMap<String, Value> {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        let m = BTreeMap::from_iter(
            self.iter()
                .map(|(k, v)| (ValueRef::String(k), v.value_ref())),
        );
        ValueRef::Map(&m).encode(writer)
    }
}

impl Encoder for BTreeMap<Value, Value> {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        let m = BTreeMap::from_iter(self.iter().map(|(k, v)| (k.value_ref(), v.value_ref())));
        ValueRef::Map(&m).encode(writer)
    }
}

impl Encoder for FrameEncoder {
    /**
     * Function duplicated from the list encoding to allow more efficient
     * encoding of frames.
     */
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        self.desc.encode(writer)?;
        if self.args.len() > LIST32_MAX {
            return Err(AmqpError::amqp_error(
                condition::DECODE_ERROR,
                Some("Encoded list size cannot be longer than 4294967291 bytes"),
            ));
        } else if self.args.len() > LIST8_MAX {
            writer.write_u8(TypeCode::List32 as u8)?;
            writer.write_u32::<NetworkEndian>((4 + self.args.len()) as u32)?;
            writer.write_u32::<NetworkEndian>(self.nelems as u32)?;
            writer.write(&self.args[..])?;
        } else if self.args.len() > 0 {
            writer.write_u8(TypeCode::List8 as u8)?;
            writer.write_u8((1 + self.args.len()) as u8)?;
            writer.write_u8(self.nelems as u8)?;
            writer.write(&self.args[..])?;
        } else {
            writer.write_u8(TypeCode::List0 as u8)?;
        }
        Ok(TypeCode::Described)
    }
}

impl Encoder for ValueRef<'_> {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        let value = self;
        match *value {
            ValueRef::Described(ref descriptor, ref value) => {
                writer.write_u8(0)?;
                descriptor.encode(writer)?;
                value.encode(writer)?;
                Ok(TypeCode::Described)
            }
            ValueRef::Null => {
                writer.write_u8(TypeCode::Null as u8)?;
                Ok(TypeCode::Null)
            }
            ValueRef::Bool(value) => {
                let code = if *value {
                    TypeCode::Booleantrue
                } else {
                    TypeCode::Booleanfalse
                };
                writer.write_u8(code as u8)?;

                Ok(code)
            }
            ValueRef::String(val) => {
                if val.len() > U8_MAX {
                    writer.write_u8(TypeCode::Str32 as u8)?;
                    writer.write_u32::<NetworkEndian>(val.len() as u32)?;
                    writer.write(val.as_bytes())?;
                    Ok(TypeCode::Str32)
                } else {
                    writer.write_u8(TypeCode::Str8 as u8)?;
                    writer.write_u8(val.len() as u8)?;
                    writer.write(val.as_bytes())?;
                    Ok(TypeCode::Str8)
                }
            }
            ValueRef::SymbolRef(val) => {
                if val.len() > U8_MAX {
                    writer.write_u8(TypeCode::Sym32 as u8)?;
                    writer.write_u32::<NetworkEndian>(val.len() as u32)?;
                    writer.write(&val.as_bytes()[..])?;
                    Ok(TypeCode::Sym32)
                } else {
                    writer.write_u8(TypeCode::Sym8 as u8)?;
                    writer.write_u8(val.len() as u8)?;
                    writer.write(&val.as_bytes()[..])?;
                    Ok(TypeCode::Sym8)
                }
            }
            ValueRef::Symbol(val) => {
                if val.data.len() > U8_MAX {
                    writer.write_u8(TypeCode::Sym32 as u8)?;
                    writer.write_u32::<NetworkEndian>(val.data.len() as u32)?;
                    writer.write(&val.data[..])?;
                    Ok(TypeCode::Sym32)
                } else {
                    writer.write_u8(TypeCode::Sym8 as u8)?;
                    writer.write_u8(val.data.len() as u8)?;
                    writer.write(&val.data[..])?;
                    Ok(TypeCode::Sym8)
                }
            }
            ValueRef::Binary(val) => {
                if val.len() > U8_MAX {
                    writer.write_u8(TypeCode::Bin32 as u8)?;
                    writer.write_u32::<NetworkEndian>(val.len() as u32)?;
                    writer.write(&val[..])?;
                    Ok(TypeCode::Bin32)
                } else {
                    writer.write_u8(TypeCode::Bin8 as u8)?;
                    writer.write_u8(val.len() as u8)?;
                    writer.write(&val[..])?;
                    Ok(TypeCode::Bin8)
                }
            }
            ValueRef::Ubyte(val) => {
                writer.write_u8(TypeCode::Ubyte as u8)?;
                writer.write_u8(*val)?;
                Ok(TypeCode::Ubyte)
            }
            ValueRef::Ushort(val) => {
                writer.write_u8(TypeCode::Ushort as u8)?;
                writer.write_u16::<NetworkEndian>(*val)?;
                Ok(TypeCode::Ushort)
            }
            ValueRef::Uint(val) => {
                if *val > U8_MAX as u32 {
                    writer.write_u8(TypeCode::Uint as u8)?;
                    writer.write_u32::<NetworkEndian>(*val)?;
                    Ok(TypeCode::Uint)
                } else if *val > 0 {
                    writer.write_u8(TypeCode::Uintsmall as u8)?;
                    writer.write_u8(*val as u8)?;
                    Ok(TypeCode::Uintsmall)
                } else {
                    writer.write_u8(TypeCode::Uint0 as u8)?;
                    Ok(TypeCode::Uint0)
                }
            }
            ValueRef::Ulong(val) => {
                if *val > U8_MAX as u64 {
                    writer.write_u8(TypeCode::Ulong as u8)?;
                    writer.write_u64::<NetworkEndian>(*val)?;
                    Ok(TypeCode::Ulong)
                } else if *val > 0 {
                    writer.write_u8(TypeCode::Ulongsmall as u8)?;
                    writer.write_u8(*val as u8)?;
                    Ok(TypeCode::Ulongsmall)
                } else {
                    writer.write_u8(TypeCode::Ulong0 as u8)?;
                    Ok(TypeCode::Ulong0)
                }
            }
            ValueRef::Byte(val) => {
                writer.write_u8(TypeCode::Byte as u8)?;
                writer.write_i8(*val)?;
                Ok(TypeCode::Byte)
            }
            ValueRef::Short(val) => {
                writer.write_u8(TypeCode::Short as u8)?;
                writer.write_i16::<NetworkEndian>(*val)?;
                Ok(TypeCode::Short)
            }
            ValueRef::Int(val) => {
                if *val > I8_MAX as i32 {
                    writer.write_u8(TypeCode::Int as u8)?;
                    writer.write_i32::<NetworkEndian>(*val)?;
                    Ok(TypeCode::Int)
                } else {
                    writer.write_u8(TypeCode::Intsmall as u8)?;
                    writer.write_i8(*val as i8)?;
                    Ok(TypeCode::Intsmall)
                }
            }
            ValueRef::Long(val) => {
                if *val > I8_MAX as i64 {
                    writer.write_u8(TypeCode::Long as u8)?;
                    writer.write_i64::<NetworkEndian>(*val)?;
                    Ok(TypeCode::Long)
                } else {
                    writer.write_u8(TypeCode::Longsmall as u8)?;
                    writer.write_i8(*val as i8)?;
                    Ok(TypeCode::Longsmall)
                }
            }
            ValueRef::Array(vec) => {
                let mut arraybuf = Vec::new();
                let mut code = 0;
                for v in vec.iter() {
                    let mut valuebuf = Vec::new();
                    v.encode(&mut valuebuf)?;
                    if code == 0 {
                        code = valuebuf[0];
                    }
                    arraybuf.extend_from_slice(&valuebuf[1..]);
                }

                if arraybuf.len() > LIST32_MAX {
                    Err(AmqpError::amqp_error(
                        condition::DECODE_ERROR,
                        Some("Encoded array size cannot be longer than 4294967291 bytes"),
                    ))
                } else if arraybuf.len() > LIST8_MAX {
                    writer.write_u8(TypeCode::Array32 as u8)?;
                    writer.write_u32::<NetworkEndian>((5 + arraybuf.len()) as u32)?;
                    writer.write_u32::<NetworkEndian>(vec.len() as u32)?;
                    writer.write_u8(code)?;
                    writer.write(&arraybuf[..])?;
                    Ok(TypeCode::Array32)
                } else if arraybuf.len() > 0 {
                    writer.write_u8(TypeCode::Array8 as u8)?;
                    writer.write_u8((2 + arraybuf.len()) as u8)?;
                    writer.write_u8(vec.len() as u8)?;
                    writer.write_u8(code)?;
                    writer.write(&arraybuf[..])?;
                    Ok(TypeCode::Array8)
                } else {
                    writer.write_u8(TypeCode::Null as u8)?;
                    Ok(TypeCode::Null)
                }
            }
            ValueRef::List(vec) => {
                let mut listbuf = Vec::new();
                for v in vec.iter() {
                    v.encode(&mut listbuf)?;
                }

                if listbuf.len() > LIST32_MAX {
                    Err(AmqpError::amqp_error(
                        condition::DECODE_ERROR,
                        Some("Encoded list size cannot be longer than 4294967291 bytes"),
                    ))
                } else if listbuf.len() > LIST8_MAX {
                    writer.write_u8(TypeCode::List32 as u8)?;
                    writer.write_u32::<NetworkEndian>((4 + listbuf.len()) as u32)?;
                    writer.write_u32::<NetworkEndian>(vec.len() as u32)?;
                    writer.write(&listbuf[..])?;
                    Ok(TypeCode::List32)
                } else if listbuf.len() > 0 {
                    writer.write_u8(TypeCode::List8 as u8)?;
                    writer.write_u8((1 + listbuf.len()) as u8)?;
                    writer.write_u8(vec.len() as u8)?;
                    writer.write(&listbuf[..])?;
                    Ok(TypeCode::List8)
                } else {
                    writer.write_u8(TypeCode::List0 as u8)?;
                    Ok(TypeCode::List0)
                }
            }
            ValueRef::Map(m) => {
                let mut listbuf = Vec::new();
                for (key, value) in m {
                    key.encode(&mut listbuf)?;
                    value.encode(&mut listbuf)?;
                }

                let n_items = m.len() * 2;

                if listbuf.len() > LIST32_MAX {
                    Err(AmqpError::amqp_error(
                        condition::DECODE_ERROR,
                        Some("Encoded map size cannot be longer than 4294967291 bytes"),
                    ))
                } else if listbuf.len() > LIST8_MAX || n_items > U8_MAX {
                    writer.write_u8(TypeCode::Map32 as u8)?;
                    writer.write_u32::<NetworkEndian>((4 + listbuf.len()) as u32)?;
                    writer.write_u32::<NetworkEndian>(n_items as u32)?;
                    writer.write(&listbuf[..])?;
                    Ok(TypeCode::Map32)
                } else {
                    writer.write_u8(TypeCode::Map8 as u8)?;
                    writer.write_u8((1 + listbuf.len()) as u8)?;
                    writer.write_u8(n_items as u8)?;
                    writer.write(&listbuf[..])?;
                    Ok(TypeCode::Map8)
                }
            }
        }
    }
}

impl Encoder for Value {
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        let value = self;
        value.value_ref().encode(writer)
    }
}

pub fn decode_value(reader: &mut dyn Read) -> Result<Value> {
    let raw_code: u8 = reader.read_u8()?;
    decode_value_with_ctor(raw_code, reader)
}

fn decode_value_with_ctor(raw_code: u8, reader: &mut dyn Read) -> Result<Value> {
    let code = decode_type(raw_code)?;
    match code {
        TypeCode::Described => {
            let descriptor = decode_value(reader)?;
            let value = decode_value(reader)?;
            Ok(Value::Described(Box::new(descriptor), Box::new(value)))
        }
        TypeCode::Null => Ok(Value::Null),
        TypeCode::Boolean => {
            let val = reader.read_u8()?;
            Ok(Value::Bool(val == 1))
        }
        TypeCode::Booleantrue => Ok(Value::Bool(true)),
        TypeCode::Booleanfalse => Ok(Value::Bool(false)),
        TypeCode::Ubyte => {
            let val = reader.read_u8()?;
            Ok(Value::Ubyte(val))
        }
        TypeCode::Ushort => {
            let val = reader.read_u16::<NetworkEndian>()?;
            Ok(Value::Ushort(val))
        }
        TypeCode::Uint => {
            let val = reader.read_u32::<NetworkEndian>()?;
            Ok(Value::Uint(val))
        }
        TypeCode::Uintsmall => {
            let val = reader.read_u8()? as u32;
            Ok(Value::Uint(val))
        }
        TypeCode::Uint0 => Ok(Value::Uint(0)),
        TypeCode::Ulong => {
            let val = reader.read_u64::<NetworkEndian>()?;
            Ok(Value::Ulong(val))
        }
        TypeCode::Ulongsmall => {
            let val = reader.read_u8()? as u64;
            Ok(Value::Ulong(val))
        }
        TypeCode::Ulong0 => Ok(Value::Ulong(0)),
        TypeCode::Byte => {
            let val = reader.read_i8()?;
            Ok(Value::Byte(val))
        }
        TypeCode::Short => {
            let val = reader.read_i16::<NetworkEndian>()?;
            Ok(Value::Short(val))
        }
        TypeCode::Int => {
            let val = reader.read_i32::<NetworkEndian>()?;
            Ok(Value::Int(val))
        }
        TypeCode::Intsmall => {
            let val = reader.read_i8()? as i32;
            Ok(Value::Int(val))
        }
        TypeCode::Long => {
            let val = reader.read_i64::<NetworkEndian>()?;
            Ok(Value::Long(val))
        }
        TypeCode::Longsmall => {
            let val = reader.read_i8()? as i64;
            Ok(Value::Long(val))
        }
        TypeCode::Str8 => {
            let len = reader.read_u8()? as usize;
            let mut buffer = vec![0u8; len];
            reader.read_exact(&mut buffer)?;
            let s = String::from_utf8(buffer)?;
            Ok(Value::String(s))
        }
        TypeCode::Str32 => {
            let len = reader.read_u32::<NetworkEndian>()? as usize;
            let mut buffer = vec![0u8; len];
            reader.read_exact(&mut buffer)?;
            let s = String::from_utf8(buffer)?;
            Ok(Value::String(s))
        }
        TypeCode::Sym8 => {
            let len = reader.read_u8()? as usize;
            let mut buffer = vec![0u8; len];
            reader.read_exact(&mut buffer)?;
            Ok(Value::Symbol(Symbol::from_vec(buffer)))
        }
        TypeCode::Sym32 => {
            let len = reader.read_u32::<NetworkEndian>()? as usize;
            let mut buffer = vec![0u8; len];
            reader.read_exact(&mut buffer)?;
            Ok(Value::Symbol(Symbol::from_vec(buffer)))
        }
        TypeCode::Bin8 => {
            let len = reader.read_u8()? as usize;
            let mut buffer = vec![0u8; len];
            reader.read_exact(&mut buffer)?;
            Ok(Value::Binary(buffer))
        }
        TypeCode::Bin32 => {
            let len = reader.read_u32::<NetworkEndian>()? as usize;
            let mut buffer = vec![0u8; len];
            reader.read_exact(&mut buffer)?;
            Ok(Value::Binary(buffer))
        }
        TypeCode::List0 => Ok(Value::List(Vec::new())),
        TypeCode::List8 => {
            let _sz = reader.read_u8()? as usize;
            let count = reader.read_u8()? as usize;
            let mut data: Vec<Value> = Vec::new();
            for _num in 0..count {
                let result = decode_value(reader)?;
                data.push(result);
            }
            Ok(Value::List(data))
        }
        TypeCode::List32 => {
            let _sz = reader.read_u32::<NetworkEndian>()? as usize;
            let count = reader.read_u32::<NetworkEndian>()? as usize;
            let mut data: Vec<Value> = Vec::new();
            for _num in 0..count {
                let result = decode_value(reader)?;
                data.push(result);
            }
            Ok(Value::List(data))
        }
        TypeCode::Array8 => {
            let _sz = reader.read_u8()? as usize;
            let count = reader.read_u8()? as usize;
            let ctype = reader.read_u8()?;
            let mut data: Vec<Value> = Vec::new();
            for _num in 0..count {
                let result = decode_value_with_ctor(ctype, reader)?;
                data.push(result);
            }
            Ok(Value::Array(data))
        }
        TypeCode::Array32 => {
            let _sz = reader.read_u32::<NetworkEndian>()? as usize;
            let count = reader.read_u32::<NetworkEndian>()? as usize;
            let ctype = reader.read_u8()?;
            let mut data: Vec<Value> = Vec::new();
            for _num in 0..count {
                let result = decode_value_with_ctor(ctype, reader)?;
                data.push(result);
            }
            Ok(Value::Array(data))
        }
        TypeCode::Map8 => {
            let _sz = reader.read_u8()? as usize;
            let count = reader.read_u8()? as usize / 2;
            let mut data: BTreeMap<Value, Value> = BTreeMap::new();
            for _num in 0..count {
                let key = decode_value(reader)?;
                let value = decode_value(reader)?;
                data.insert(key, value);
            }
            Ok(Value::Map(data))
        }
        TypeCode::Map32 => {
            let _sz = reader.read_u32::<NetworkEndian>()? as usize;
            let count = reader.read_u32::<NetworkEndian>()? as usize / 2;
            let mut data: BTreeMap<Value, Value> = BTreeMap::new();
            for _num in 0..count {
                let key = decode_value(reader)?;
                let value = decode_value(reader)?;
                data.insert(key, value);
            }
            Ok(Value::Map(data))
        }
    }
}

#[repr(u8)]
#[derive(Clone, PartialEq, Debug, PartialOrd, Copy)]
pub enum TypeCode {
    Described = 0x00,
    Null = 0x40,
    Boolean = 0x56,
    Booleantrue = 0x41,
    Booleanfalse = 0x42,
    Ubyte = 0x50,
    Ushort = 0x60,
    Uint = 0x70,
    Uintsmall = 0x52,
    Uint0 = 0x43,
    Ulong = 0x80,
    Ulongsmall = 0x53,
    Ulong0 = 0x44,
    Byte = 0x51,
    Short = 0x61,
    Int = 0x71,
    Intsmall = 0x54,
    Long = 0x81,
    Longsmall = 0x55,
    Bin8 = 0xA0,
    Bin32 = 0xB0,
    Str8 = 0xA1,
    Str32 = 0xB1,
    Sym8 = 0xA3,
    Sym32 = 0xB3,
    List0 = 0x45,
    List8 = 0xC0,
    List32 = 0xD0,
    Map8 = 0xC1,
    Map32 = 0xD1,
    Array8 = 0xE0,
    Array32 = 0xF0,
}

fn decode_type(code: u8) -> Result<TypeCode> {
    match code {
        0x00 => Ok(TypeCode::Described),
        0x40 => Ok(TypeCode::Null),
        0x56 => Ok(TypeCode::Boolean),
        0x41 => Ok(TypeCode::Booleantrue),
        0x42 => Ok(TypeCode::Booleanfalse),
        0x50 => Ok(TypeCode::Ubyte),
        0x60 => Ok(TypeCode::Ushort),
        0x70 => Ok(TypeCode::Uint),
        0x52 => Ok(TypeCode::Uintsmall),
        0x43 => Ok(TypeCode::Uint0),
        0x80 => Ok(TypeCode::Ulong),
        0x53 => Ok(TypeCode::Ulongsmall),
        0x44 => Ok(TypeCode::Ulong0),
        0x51 => Ok(TypeCode::Byte),
        0x61 => Ok(TypeCode::Short),
        0x71 => Ok(TypeCode::Int),
        0x54 => Ok(TypeCode::Intsmall),
        0x81 => Ok(TypeCode::Long),
        0x55 => Ok(TypeCode::Longsmall),
        0xA0 => Ok(TypeCode::Bin8),
        0xA1 => Ok(TypeCode::Str8),
        0xA3 => Ok(TypeCode::Sym8),
        0xB0 => Ok(TypeCode::Bin32),
        0xB1 => Ok(TypeCode::Str32),
        0xB3 => Ok(TypeCode::Sym32),
        0x45 => Ok(TypeCode::List0),
        0xC0 => Ok(TypeCode::List8),
        0xD0 => Ok(TypeCode::List32),
        0xC1 => Ok(TypeCode::Map8),
        0xD1 => Ok(TypeCode::Map32),
        0xE0 => Ok(TypeCode::Array8),
        0xF0 => Ok(TypeCode::Array32),
        _ => Err(AmqpError::amqp_error(
            condition::DECODE_ERROR,
            Some(format!("Unknown type code: 0x{:X}", code).as_str()),
        )),
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::error::*;

    fn assert_type(value: &Value, expected_len: usize, expected_type: TypeCode) {
        let mut output: Vec<u8> = Vec::new();
        let ctype = value.encode(&mut output).unwrap();
        assert_eq!(expected_type, ctype);
        assert_eq!(expected_len, output.len());

        let decoded = decode_value(&mut &output[..]).unwrap();
        assert_eq!(&decoded, value);
    }

    #[test]
    fn check_types() {
        assert_type(&Value::Ulong(123), 2, TypeCode::Ulongsmall);
        assert_type(&Value::Ulong(1234), 9, TypeCode::Ulong);
        assert_type(
            &Value::String(String::from("Hello, world")),
            14,
            TypeCode::Str8,
        );
        assert_type(&Value::String(String::from("aaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbcccccccccccccccccccccccdddddddddddddddddddddddddeeeeeeeeeeeeeeeeeeeeeeeeeffffffffffffffffffffgggggggggggggggggggggggghhhhhhhhhhhhhhhhhhhhhhhiiiiiiiiiiiiiiiiiiiiiiiijjjjjjjjjjjjjjjjjjjkkkkkkkkkkkkkkkkkkkkkkllllllllllllllllllllmmmmmmmmmmmmmmmmmmmmnnnnnnnnnnnnnnnnnnnnooooooooooooooooooooppppppppppppppppppqqqqqqqqqqqqqqqq")), 370, TypeCode::Str32);
        assert_type(
            &Value::List(vec![
                Value::Ulong(1),
                Value::Ulong(42),
                Value::String(String::from("Hello, world")),
            ]),
            21,
            TypeCode::List8,
        );
    }
}
