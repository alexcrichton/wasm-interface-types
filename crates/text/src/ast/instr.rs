use wast::parser::{Parse, Parser, Result};

macro_rules! instructions {
    (pub enum Instruction<'a> {
        $(
            $name:ident $(($($arg:tt)*))? : [$($binary:tt)*] : $instr:tt,
        )*
    }) => (
        pub enum Instruction<'a> {
            $(
                $name $(( $($arg)* ))?,
            )*
        }

        #[allow(non_snake_case)]
        impl<'a> Parse<'a> for Instruction<'a> {
            fn parse(parser: Parser<'a>) -> Result<Self> {
                $(
                    fn $name<'a>(_parser: Parser<'a>) -> Result<Instruction<'a>> {
                        Ok(Instruction::$name $((
                            _parser.parse::<$($arg)*>()?,
                        ))?)
                    }
                )*
                let parse_remainder = parser.step(|c| {
                    let (kw, rest) = match c.keyword() {
                        Some(pair) => pair,
                        None => return Err(c.error("expected an instruction")),
                    };
                    match kw {
                        $($instr => Ok(($name as fn(_) -> _, rest)),)*
                        _ => return Err(c.error("unknown operator or unexpected token")),
                    }
                })?;
                parse_remainder(parser)
            }
        }

        impl crate::binary::Encode for Instruction<'_> {
            #[allow(non_snake_case)]
            fn encode(&self, v: &mut Vec<u8>) {
                match self {
                    $(
                        Instruction::$name $((instructions!(@first $($arg)*)))? => {
                            fn encode<'a>($(arg: &$($arg)*,)? v: &mut Vec<u8>) {
                                v.extend_from_slice(&[$($binary)*]);
                                $(<$($arg)* as crate::binary::Encode>::encode(arg, v);)?
                            }
                            encode($( instructions!(@first $($arg)*), )? v)
                        }
                    )*
                }
            }
        }
    );

    (@first $first:ident $($t:tt)*) => ($first);
}

instructions! {
    pub enum Instruction<'a> {
        ArgGet(wast::Index<'a>) : [0x00] : "arg.get",
        CallCore(wast::Index<'a>) : [0x01] : "call-core",
        End : [0x02] : "end",
    }
}
