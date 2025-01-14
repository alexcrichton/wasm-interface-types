use std::fmt;
use std::str;

#[derive(Clone)]
pub struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

#[derive(Debug)]
pub struct Error {
    inner: Box<ErrorInner>,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
struct ErrorInner {
    at: usize,
    kind: ErrorKind,
}

#[derive(Debug)]
enum ErrorKind {
    InvalidVersion(String),
    UlebTooBig(u64),
    UlebInvalid,
    UnexpectedEof,
    InvalidUtf8,
    InvalidSection(u8),
    InvalidValType(u8),
    InvalidInstruction(u8),
    Expected(usize),
    TrailingBytes,
}

impl<'a> Parser<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Parser<'a>> {
        let mut parser = Parser { bytes, pos: 0 };
        let version = <&str as Parse>::parse(&mut parser)?;
        if version != wit_schema_version::VERSION {
            parser.pos = 0;
            return Err(parser.error(ErrorKind::InvalidVersion(version.to_string())));
        }
        Ok(parser)
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn section(&mut self) -> Result<Section<'a>> {
        self.parse()
    }

    fn parse<T: Parse<'a>>(&mut self) -> Result<T> {
        T::parse(self)
    }

    fn parse_next_in_section<T: Parse<'a>>(&mut self, cnt: &mut u32) -> Option<Result<T>> {
        if *cnt == 0 {
            if self.bytes.len() != 0 {
                return Some(Err(self.error(ErrorKind::TrailingBytes)));
            }
            None
        } else {
            *cnt -= 1;
            Some(T::parse(self))
        }
    }

    fn error(&self, kind: ErrorKind) -> Error {
        Error {
            inner: Box::new(ErrorInner { at: self.pos, kind }),
        }
    }
}

pub trait Parse<'a>: Sized {
    fn parse(parser: &mut Parser<'a>) -> Result<Self>;
}

pub enum Section<'a> {
    Type(Types<'a>),
    Import(Imports<'a>),
    Export(Exports<'a>),
    Func(Funcs<'a>),
}

impl<'a> Parse<'a> for Section<'a> {
    fn parse(parser: &mut Parser<'a>) -> Result<Self> {
        let id_pos = parser.pos;
        let id = u8::parse(parser)?;
        let bytes = <&[u8]>::parse(parser)?;
        let mut parser = Parser {
            bytes,
            pos: parser.pos - bytes.len(),
        };
        match id {
            0 => {
                let cnt = parser.parse()?;
                Ok(Section::Type(Types { parser, cnt }))
            }
            1 => {
                let cnt = parser.parse()?;
                Ok(Section::Import(Imports { parser, cnt }))
            }
            2 => {
                let cnt = parser.parse()?;
                Ok(Section::Export(Exports { parser, cnt }))
            }
            3 => {
                let cnt = parser.parse()?;
                Ok(Section::Func(Funcs { parser, cnt }))
            }
            n => {
                parser.pos = id_pos;
                Err(parser.error(ErrorKind::InvalidSection(n)))
            }
        }
    }
}

impl<'a> Parse<'a> for u8 {
    fn parse(parser: &mut Parser<'a>) -> Result<Self> {
        match parser.bytes.get(0).cloned() {
            Some(byte) => {
                parser.pos += 1;
                parser.bytes = &parser.bytes[1..];
                Ok(byte)
            }
            None => Err(parser.error(ErrorKind::UnexpectedEof)),
        }
    }
}

impl<'a> Parse<'a> for &'a [u8] {
    fn parse(parser: &mut Parser<'a>) -> Result<Self> {
        let len = parser.parse::<u32>()? as usize;
        match parser.bytes.get(..len) {
            Some(n) => {
                parser.pos += len;
                parser.bytes = &parser.bytes[len..];
                Ok(n)
            }
            None => Err(parser.error(ErrorKind::Expected(len))),
        }
    }
}

impl<'a> Parse<'a> for &'a str {
    fn parse(parser: &mut Parser<'a>) -> Result<Self> {
        let pos = parser.pos;
        match str::from_utf8(parser.parse()?) {
            Ok(s) => Ok(s),
            Err(_) => {
                parser.pos = pos;
                Err(parser.error(ErrorKind::InvalidUtf8))
            }
        }
    }
}

impl<'a> Parse<'a> for u32 {
    fn parse(parser: &mut Parser<'a>) -> Result<Self> {
        let mut bytes = parser.bytes;
        match leb128::read::unsigned(&mut bytes) {
            Ok(n) if n <= u32::max_value() as u64 => {
                parser.pos += parser.bytes.len() - bytes.len();
                parser.bytes = bytes;
                Ok(n as u32)
            }
            Ok(n) => Err(parser.error(ErrorKind::UlebTooBig(n))),
            Err(_) => Err(parser.error(ErrorKind::UlebInvalid)),
        }
    }
}

pub struct Types<'a> {
    parser: Parser<'a>,
    cnt: u32,
}

impl<'a> Iterator for Types<'a> {
    type Item = Result<Type>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parser.parse_next_in_section(&mut self.cnt)
    }
}

pub struct Type {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

impl<'a> Parse<'a> for Type {
    fn parse(parser: &mut Parser<'a>) -> Result<Type> {
        let mut types = || -> Result<Vec<ValType>> {
            let cnt = parser.parse::<u32>()?;
            (0..cnt).map(|_| parser.parse()).collect()
        };
        Ok(Type {
            params: types()?,
            results: types()?,
        })
    }
}

pub enum ValType {
    S8,
    S16,
    S32,
    S64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    String,
}

impl<'a> Parse<'a> for ValType {
    fn parse(parser: &mut Parser<'a>) -> Result<ValType> {
        Ok(match parser.parse::<u8>()? {
            0 => ValType::String,
            1 => ValType::S8,
            2 => ValType::S16,
            3 => ValType::S32,
            4 => ValType::S64,
            5 => ValType::U8,
            6 => ValType::U16,
            7 => ValType::U32,
            8 => ValType::U64,
            9 => ValType::F32,
            10 => ValType::F64,
            n => return Err(parser.error(ErrorKind::InvalidValType(n))),
        })
    }
}

pub struct Imports<'a> {
    parser: Parser<'a>,
    cnt: u32,
}

impl<'a> Iterator for Imports<'a> {
    type Item = Result<Import<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parser.parse_next_in_section(&mut self.cnt)
    }
}

pub struct Import<'a> {
    pub module: &'a str,
    pub name: &'a str,
    pub ty: u32,
}

impl<'a> Parse<'a> for Import<'a> {
    fn parse(parser: &mut Parser<'a>) -> Result<Import<'a>> {
        Ok(Import {
            module: parser.parse()?,
            name: parser.parse()?,
            ty: parser.parse()?,
        })
    }
}

pub struct Exports<'a> {
    parser: Parser<'a>,
    cnt: u32,
}

impl<'a> Iterator for Exports<'a> {
    type Item = Result<Export<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parser.parse_next_in_section(&mut self.cnt)
    }
}

pub struct Export<'a> {
    pub func: u32,
    pub name: &'a str,
}

impl<'a> Parse<'a> for Export<'a> {
    fn parse(parser: &mut Parser<'a>) -> Result<Export<'a>> {
        Ok(Export {
            func: parser.parse()?,
            name: parser.parse()?,
        })
    }
}

pub struct Funcs<'a> {
    parser: Parser<'a>,
    cnt: u32,
}

impl<'a> Iterator for Funcs<'a> {
    type Item = Result<Func<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parser.parse_next_in_section(&mut self.cnt)
    }
}

pub struct Func<'a> {
    pub ty: u32,
    parser: Parser<'a>,
}

impl<'a> Parse<'a> for Func<'a> {
    fn parse(parser: &mut Parser<'a>) -> Result<Func<'a>> {
        let bytes = parser.parse::<&[u8]>()?;
        let mut parser = Parser {
            bytes,
            pos: parser.pos - bytes.len(),
        };
        Ok(Func {
            ty: parser.parse()?,
            parser,
        })
    }
}

impl<'a> Func<'a> {
    pub fn instrs(&self) -> Instructions<'a> {
        Instructions {
            parser: self.parser.clone(),
        }
    }
}

pub struct Instructions<'a> {
    parser: Parser<'a>,
}

impl<'a> Iterator for Instructions<'a> {
    type Item = Result<Instruction>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.parser.parse() {
            Ok(Instruction::End) => {
                if self.parser.is_empty() {
                    None
                } else {
                    Some(Err(self.parser.error(ErrorKind::TrailingBytes)))
                }
            }
            other => Some(other),
        }
    }
}

macro_rules! instructions {
    (pub enum Instruction {
        $(
            $name:ident $(($($arg:tt)*))? = $binary:tt,
        )*
    }) => (
        pub enum Instruction {
            $(
                $name $(( $($arg)* ))?,
            )*
        }

        #[allow(non_snake_case)]
        impl<'a> Parse<'a> for Instruction {
            fn parse(parser: &mut Parser<'a>) -> Result<Self> {
                $(
                    fn $name(_parser: &mut Parser<'_>) -> Result<Instruction> {
                        Ok(Instruction::$name $((
                            _parser.parse::<$($arg)*>()?,
                        ))?)
                    }
                )*
                let pos = parser.pos;
                match parser.parse::<u8>()? {
                    $(
                        $binary => $name(parser),
                    )*
                    n => {
                        parser.pos = pos;
                        Err(parser.error(ErrorKind::InvalidInstruction(n)))
                    }
                }
            }
        }
    );
}

instructions! {
    pub enum Instruction {
        ArgGet(u32) = 0x00,
        CallCore(u32) = 0x01,
        End = 0x02,
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to parse at byte {}: ", self.inner.at)?;
        match &self.inner.kind {
            ErrorKind::InvalidVersion(s) => write!(
                f,
                "schema version `{}` doesn't match `{}`",
                s,
                wit_schema_version::VERSION
            ),
            ErrorKind::UlebTooBig(_) => write!(f, "uleb encoded integer too big"),
            ErrorKind::UlebInvalid => write!(f, "failed to parse uleb integer"),
            ErrorKind::UnexpectedEof => write!(f, "unexpected end-of-file"),
            ErrorKind::InvalidUtf8 => write!(f, "invalid utf-8 string"),
            ErrorKind::InvalidSection(n) => write!(f, "invalid section id: {}", n),
            ErrorKind::InvalidValType(n) => write!(f, "invalid value type: {}", n),
            ErrorKind::InvalidInstruction(n) => write!(f, "invalid instruction: {}", n),
            ErrorKind::Expected(n) => write!(f, "expected {} more bytes but hit eof", n),
            ErrorKind::TrailingBytes => write!(f, "trailing bytes at the end of the section"),
        }
    }
}

impl std::error::Error for Error {}
