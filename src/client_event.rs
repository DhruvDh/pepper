use std::fmt;

use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize, Serialize};

use crate::{
    event_manager::ConnectionEvent,
    serialization::{DeserializationSlice, SerializationBuf},
};

#[derive(Debug, Clone, Copy)]
pub enum ClientEvent {
    None,
    Key(Key),
    Resize(u16, u16),
    Connection(ConnectionEvent),
}

#[derive(Debug)]
pub enum KeyParseError {
    UnexpectedEnd,
    InvalidCharacter(char),
}

impl fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "could not finish parsing key"),
            Self::InvalidCharacter(c) => write!(f, "invalid character {}", c),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    None,
    Backspace,
    Enter,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    Delete,
    F(u8),
    Char(char),
    Ctrl(char),
    Alt(char),
    Esc,
}

impl Key {
    pub fn parse(chars: &mut impl Iterator<Item = char>) -> Result<Self, KeyParseError> {
        macro_rules! next {
            () => {
                match chars.next() {
                    Some(element) => element,
                    None => return Err(KeyParseError::UnexpectedEnd),
                }
            };
        }

        macro_rules! consume {
            ($character:expr) => {
                let c = next!();
                if c != $character {
                    return Err(KeyParseError::InvalidCharacter(c));
                }
            };
        }

        macro_rules! consume_str {
            ($str:expr) => {
                for c in $str.chars() {
                    consume!(c);
                }
            };
        }

        let key = match next!() {
            '\\' => match next!() {
                '\\' => Key::Char('\\'),
                '<' => Key::Char('<'),
                c => return Err(KeyParseError::InvalidCharacter(c)),
            },
            '<' => match next!() {
                'b' => {
                    consume_str!("ackspace>");
                    Key::Backspace
                }
                's' => {
                    consume_str!("pace>");
                    Key::Char(' ')
                }
                'e' => match next!() {
                    'n' => match next!() {
                        't' => {
                            consume_str!("er>");
                            Key::Enter
                        }
                        'd' => {
                            consume!('>');
                            Key::End
                        }
                        c => return Err(KeyParseError::InvalidCharacter(c)),
                    },
                    's' => {
                        consume_str!("c>");
                        Key::Esc
                    }
                    c => return Err(KeyParseError::InvalidCharacter(c)),
                },
                'l' => {
                    consume_str!("eft>");
                    Key::Left
                }
                'r' => {
                    consume_str!("ight>");
                    Key::Right
                }
                'u' => {
                    consume_str!("p>");
                    Key::Up
                }
                'd' => match next!() {
                    'o' => {
                        consume_str!("wn>");
                        Key::Down
                    }
                    'e' => {
                        consume_str!("lete>");
                        Key::Delete
                    }
                    c => return Err(KeyParseError::InvalidCharacter(c)),
                },
                'h' => {
                    consume_str!("ome>");
                    Key::Home
                }
                'p' => {
                    consume_str!("age");
                    match next!() {
                        'u' => {
                            consume_str!("p>");
                            Key::PageUp
                        }
                        'd' => {
                            consume_str!("own>");
                            Key::PageDown
                        }
                        c => return Err(KeyParseError::InvalidCharacter(c)),
                    }
                }
                't' => {
                    consume_str!("ab>");
                    Key::Tab
                }
                'f' => {
                    let n = match next!() {
                        '1' => match next!() {
                            '>' => 1,
                            '0' => {
                                consume!('>');
                                10
                            }
                            '1' => {
                                consume!('>');
                                11
                            }
                            '2' => {
                                consume!('>');
                                12
                            }
                            c => return Err(KeyParseError::InvalidCharacter(c)),
                        },
                        c => {
                            consume!('>');
                            match c.to_digit(10) {
                                Some(n) => n,
                                None => return Err(KeyParseError::InvalidCharacter(c)),
                            }
                        }
                    };
                    Key::F(n as _)
                }
                'c' => {
                    consume!('-');
                    let c = next!();
                    let key = if c.is_ascii_alphanumeric() {
                        Key::Ctrl(c)
                    } else {
                        return Err(KeyParseError::InvalidCharacter(c));
                    };
                    consume!('>');
                    key
                }
                'a' => {
                    consume!('-');
                    let c = next!();
                    let key = if c.is_ascii_alphanumeric() {
                        Key::Alt(c)
                    } else {
                        return Err(KeyParseError::InvalidCharacter(c));
                    };
                    consume!('>');
                    key
                }
                c => return Err(KeyParseError::InvalidCharacter(c)),
            },
            c => {
                if c.is_ascii() {
                    Key::Char(c)
                } else {
                    return Err(KeyParseError::InvalidCharacter(c));
                }
            }
        };

        Ok(key)
    }
}

#[derive(Default)]
pub struct ClientEventSerializer(SerializationBuf);

impl ClientEventSerializer {
    pub fn serialize<T>(&mut self, input: T)
    where
        T: Serialize,
    {
        let _ = input.serialize(&mut self.0);
    }

    pub fn bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Debug)]
pub enum ClientEventDeserializeResult {
    Some(Key),
    None,
    Error,
}

pub struct ClientEventDeserializer<'a>(DeserializationSlice<'a>);

impl<'a> ClientEventDeserializer<'a> {
    pub fn from_slice(slice: &'a [u8]) -> Self {
        Self(DeserializationSlice::from_slice(slice))
    }

    pub fn deserialize_next(&mut self) -> ClientEventDeserializeResult {
        if self.0.as_slice().is_empty() {
            return ClientEventDeserializeResult::None;
        }

        match Key::deserialize(&mut self.0) {
            Ok(key) => ClientEventDeserializeResult::Some(key),
            Err(_) => ClientEventDeserializeResult::Error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key() {
        assert_eq!(
            Key::Backspace,
            Key::parse(&mut "<backspace>".chars()).unwrap()
        );
        assert_eq!(Key::Char(' '), Key::parse(&mut "<space>".chars()).unwrap());
        assert_eq!(Key::Enter, Key::parse(&mut "<enter>".chars()).unwrap());
        assert_eq!(Key::Left, Key::parse(&mut "<left>".chars()).unwrap());
        assert_eq!(Key::Right, Key::parse(&mut "<right>".chars()).unwrap());
        assert_eq!(Key::Up, Key::parse(&mut "<up>".chars()).unwrap());
        assert_eq!(Key::Down, Key::parse(&mut "<down>".chars()).unwrap());
        assert_eq!(Key::Home, Key::parse(&mut "<home>".chars()).unwrap());
        assert_eq!(Key::End, Key::parse(&mut "<end>".chars()).unwrap());
        assert_eq!(Key::PageUp, Key::parse(&mut "<pageup>".chars()).unwrap());
        assert_eq!(
            Key::PageDown,
            Key::parse(&mut "<pagedown>".chars()).unwrap()
        );
        assert_eq!(Key::Tab, Key::parse(&mut "<tab>".chars()).unwrap());
        assert_eq!(Key::Delete, Key::parse(&mut "<delete>".chars()).unwrap());
        assert_eq!(Key::Esc, Key::parse(&mut "<esc>".chars()).unwrap());

        for n in 1..=12 {
            let s = format!("<f{}>", n);
            assert_eq!(Key::F(n as _), Key::parse(&mut s.chars()).unwrap());
        }

        assert_eq!(Key::Ctrl('z'), Key::parse(&mut "<c-z>".chars()).unwrap());
        assert_eq!(Key::Ctrl('0'), Key::parse(&mut "<c-0>".chars()).unwrap());
        assert_eq!(Key::Ctrl('9'), Key::parse(&mut "<c-9>".chars()).unwrap());

        assert_eq!(Key::Alt('a'), Key::parse(&mut "<a-a>".chars()).unwrap());
        assert_eq!(Key::Alt('z'), Key::parse(&mut "<a-z>".chars()).unwrap());
        assert_eq!(Key::Alt('0'), Key::parse(&mut "<a-0>".chars()).unwrap());
        assert_eq!(Key::Alt('9'), Key::parse(&mut "<a-9>".chars()).unwrap());

        assert_eq!(Key::Char('a'), Key::parse(&mut "a".chars()).unwrap());
        assert_eq!(Key::Char('z'), Key::parse(&mut "z".chars()).unwrap());
        assert_eq!(Key::Char('0'), Key::parse(&mut "0".chars()).unwrap());
        assert_eq!(Key::Char('9'), Key::parse(&mut "9".chars()).unwrap());
        assert_eq!(Key::Char('_'), Key::parse(&mut "_".chars()).unwrap());
        assert_eq!(Key::Char('<'), Key::parse(&mut "\\<".chars()).unwrap());
        assert_eq!(Key::Char('\\'), Key::parse(&mut "\\\\".chars()).unwrap());
    }

    #[test]
    fn key_serialization() {
        macro_rules! assert_serialization {
            ($key:expr) => {
                let mut serializer = ClientEventSerializer::default();
                serializer.serialize($key);
                let slice = serializer.bytes();
                let mut deserializer = ClientEventDeserializer::from_slice(slice);
                if let ClientEventDeserializeResult::Some(key) = deserializer.deserialize_next() {
                    assert_eq!($key, key);
                } else {
                    assert!(false);
                }
            };
        }

        assert_serialization!(Key::None);
        assert_serialization!(Key::Backspace);
        assert_serialization!(Key::Enter);
        assert_serialization!(Key::Left);
        assert_serialization!(Key::Right);
        assert_serialization!(Key::Up);
        assert_serialization!(Key::Down);
        assert_serialization!(Key::Home);
        assert_serialization!(Key::End);
        assert_serialization!(Key::PageUp);
        assert_serialization!(Key::PageDown);
        assert_serialization!(Key::Tab);
        assert_serialization!(Key::Delete);
        assert_serialization!(Key::F(0));
        assert_serialization!(Key::F(9));
        assert_serialization!(Key::F(12));
        assert_serialization!(Key::Char('a'));
        assert_serialization!(Key::Char('z'));
        assert_serialization!(Key::Char('A'));
        assert_serialization!(Key::Char('Z'));
        assert_serialization!(Key::Char('0'));
        assert_serialization!(Key::Char('9'));
        assert_serialization!(Key::Char('$'));
        assert_serialization!(Key::Ctrl('a'));
        assert_serialization!(Key::Ctrl('z'));
        assert_serialization!(Key::Ctrl('A'));
        assert_serialization!(Key::Ctrl('Z'));
        assert_serialization!(Key::Ctrl('0'));
        assert_serialization!(Key::Ctrl('9'));
        assert_serialization!(Key::Ctrl('$'));
        assert_serialization!(Key::Alt('a'));
        assert_serialization!(Key::Alt('z'));
        assert_serialization!(Key::Alt('A'));
        assert_serialization!(Key::Alt('Z'));
        assert_serialization!(Key::Alt('0'));
        assert_serialization!(Key::Alt('9'));
        assert_serialization!(Key::Alt('$'));
        assert_serialization!(Key::Esc);
    }
}