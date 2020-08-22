/*
 * Copyright 2020, Ulf Lilleengen
 * License: Apache License 2.0 (see the file LICENSE or http://apache.org/licenses/LICENSE-2.0.html).
 */

use byteorder::NetworkEndian;
use byteorder::WriteBytesExt;
use std::io::Write;
use std::vec::Vec;

use crate::convert::*;
use crate::error::*;
use crate::types::*;

/**
 * An encoder helper type that provides convenient encoding of AMQP described list types,
 * such as frames.
 */
pub struct FrameEncoder {
    desc: Value,
    args: Vec<u8>,
    nelems: usize,
}

/**
 * A decoder helper type that provides convenient decoding of described list types,
 * such as frames and a few other AMQP types.
 */
#[allow(dead_code)]
pub struct FrameDecoder<'a> {
    desc: &'a Value,
    args: &'a mut Vec<Value>,
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

    pub fn decode_required<T: TryFromValue>(&mut self, value: &mut T) -> Result<()> {
        self.decode(value, true)
    }

    pub fn decode_optional<T: TryFromValue>(&mut self, value: &mut T) -> Result<()> {
        self.decode(value, false)
    }

    pub fn decode<T: TryFromValue>(&mut self, value: &mut T, required: bool) -> Result<()> {
        if self.args.len() == 0 {
            if required {
                return Err(AmqpError::amqp_error(
                    condition::DECODE_ERROR,
                    Some("Unexpected end of list"),
                ));
            } else {
                return Ok(());
            }
        }
        let mut drained = self.args.drain(0..1);
        println!("Next arg to decode: {:?}", drained);
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

impl Encoder for FrameEncoder {
    /**
     * Function duplicated from the list encoding to allow more efficient
     * encoding of frames.
     */
    fn encode(&self, writer: &mut dyn Write) -> Result<TypeCode> {
        writer.write_u8(0)?;
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
